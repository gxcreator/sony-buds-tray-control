//! The headphone engine: a faithful reimplementation of `MDRHeadphones` from
//! the reference client — receive buffering, frame parsing, command dispatch,
//! state updates and the init/sync/commit task state machine.
//!
//! The C++ coroutine tasks are modelled as explicit step queues; each step
//! either sends a command (waiting for an ACK or a specific response frame)
//! or waits for a particular frame type. Steps carry an optional list of
//! property commits that are applied once the step's wait is satisfied.

use std::collections::VecDeque;
use std::time::Duration;
use tokio::time::Instant;

use thiserror::Error;

use crate::protocol::codec;
use crate::protocol::*;
use crate::transport::{PollStatus, Transport, TransportError};

use super::state::{DeviceState, MultipointDevice, MultipointRequest, Properties};

const STEP_TIMEOUT: Duration = Duration::from_secs(3);
/// How often a step's command is retransmitted on timeout before the task
/// fails (the reference client retries 10 times; we keep the worst case
/// bounded at ~12s with the 3s step timeout).
const MAX_STEP_RETRIES: u8 = 3;
const MAX_FRAME: usize = MAX_PACKET_SIZE;

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum EngineError {
    #[error("command timed out")]
    Timeout,
    #[error("not supported: {0}")]
    NotSupported(&'static str),
    #[error("transport: {0}")]
    Transport(#[from] TransportError),
    #[error("malformed frame")]
    MalformedFrame,
    #[error("another task is in progress")]
    TaskInProgress,
    #[error("internal: {0}")]
    Internal(&'static str),
}

/// Events surfaced to the application layer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DeviceEvent {
    /// The init sequence finished successfully.
    InitOk,
    /// A sync sequence finished successfully.
    SyncOk,
    /// A commit sequence finished successfully.
    CommitOk,
    /// Device info (model, FW, color...) was refreshed.
    DeviceInfo,
    /// The support function tables were refreshed.
    SupportFunctions,
    /// Audio codec changed.
    Codec,
    /// NC/ASM state changed.
    NcAsm,
    /// Battery levels changed.
    Battery,
    /// Now-playing metadata changed.
    PlaybackMetadata,
    /// Playback status (play/pause) changed.
    PlaybackStatus,
    /// Volume changed.
    Volume,
    /// Speak-to-chat changed.
    SpeakToChat,
    /// EQ state changed.
    Equalizer,
    /// DSEE changed.
    Upscaling,
    /// Auto power off changed.
    AutoPowerOff,
    /// Auto pause changed.
    AutoPause,
    /// Voice guidance changed.
    VoiceGuidance,
    /// Touch presets changed.
    TouchFunctions,
    /// Listening mode (BGM/cinema) changed.
    ListeningMode,
    /// General settings changed.
    GeneralSetting,
    /// Multipoint device list / playback device changed.
    Multipoint,
    /// An alert message from the device.
    Alert(AlertMessageType),
    /// Fatal: the engine is unusable; the connection must be dropped.
    Error(EngineError),
    /// The transport went away (EOF / closed).
    Disconnected,
}

/// What a step waits for before the next step can run.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Wait {
    Ack,
    ProtocolInfo,
    SupportFunction,
}

/// Property groups that get committed when a step's wait is satisfied.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PropCommit {
    Shutdown,
    NcAsmGroup,
    NcAsmButton,
    PlayVolume,
    StcEnabled,
    StcExt,
    BgmMode,
    UpmixCinema,
    EqPreset,
    EqConfig,
    Upscaling,
    AutoPowerOff,
    AutoPause,
    VoiceGuidanceEnabled,
    VoiceGuidanceVolume,
    TouchFunctions,
    HeadGesture,
    Gs1,
    Gs2,
    Gs3,
    Gs4,
}

impl PropCommit {
    fn apply(self, props: &mut Properties) {
        match self {
            PropCommit::Shutdown => props.shutdown.commit(),
            PropCommit::NcAsmGroup => {
                props.nc_asm_enabled.commit();
                props.nc_asm_mode.commit();
                props.nc_asm_ambient_level.commit();
                props.nc_asm_focus_on_voice.commit();
                props.nc_asm_auto_asm_enabled.commit();
                props.nc_asm_noise_adaptive_sensitivity.commit();
            }
            PropCommit::NcAsmButton => props.nc_asm_button_function.commit(),
            PropCommit::PlayVolume => props.play_volume.commit(),
            PropCommit::StcEnabled => props.speak_to_chat_enabled.commit(),
            PropCommit::StcExt => {
                props.speak_to_chat_detect_sensitivity.commit();
                props.speak_to_mode_out_time.commit();
            }
            PropCommit::BgmMode => {
                props.bgm_mode_enabled.commit();
                props.bgm_mode_room_size.commit();
            }
            PropCommit::UpmixCinema => props.upmix_cinema_enabled.commit(),
            PropCommit::EqPreset => props.eq_preset_id.commit(),
            PropCommit::EqConfig => {
                props.eq_config.commit();
                props.eq_clear_bass.commit();
            }
            PropCommit::Upscaling => props.upscaling_enabled.commit(),
            PropCommit::AutoPowerOff => props.auto_power_off.commit(),
            PropCommit::AutoPause => props.auto_pause_enabled.commit(),
            PropCommit::VoiceGuidanceEnabled => props.voice_guidance_enabled.commit(),
            PropCommit::VoiceGuidanceVolume => props.voice_guidance_volume.commit(),
            PropCommit::TouchFunctions => {
                props.touch_function_left.commit();
                props.touch_function_right.commit();
            }
            PropCommit::HeadGesture => props.head_gesture_enabled.commit(),
            PropCommit::Gs1 => props.gs_param_bool[0].commit(),
            PropCommit::Gs2 => props.gs_param_bool[1].commit(),
            PropCommit::Gs3 => props.gs_param_bool[2].commit(),
            PropCommit::Gs4 => props.gs_param_bool[3].commit(),
        }
    }
}

/// A single task step.
#[derive(Debug, Clone)]
enum Step {
    /// Send a command; wait for `wait` once sent.
    Send {
        payload: Vec<u8>,
        data_type: DataType,
        wait: Wait,
        commit: Option<PropCommit>,
    },
    /// Wait for a specific frame type.
    Await(Wait),
    /// Run `steps` only if table 2 is supported (known after protocol info).
    IfTable2(Vec<Step>),
    /// Run `steps` only if the table-1 function is supported (known after
    /// the support-function frame arrives).
    IfSupport1(FunctionTable1, Vec<Step>),
    /// Run `steps` only if any of the table-2 functions is supported (known
    /// after the support-function frame arrives).
    IfSupport2Any(Vec<FunctionTable2>, Vec<Step>),
    /// Resolve the NC/ASM inquired type from the advertised support.
    NcAsmQuery,
    /// Task completed; emit `event`.
    Done(DeviceEvent),
}

impl Step {
    fn send(payload: Vec<u8>) -> Step {
        Step::Send {
            payload,
            data_type: DataType::DataMdr,
            wait: Wait::Ack,
            commit: None,
        }
    }

    fn send_t2(payload: Vec<u8>) -> Step {
        Step::Send {
            payload,
            data_type: DataType::DataMdrNo2,
            wait: Wait::Ack,
            commit: None,
        }
    }

    fn send_commit(payload: Vec<u8>, commit: PropCommit) -> Step {
        Step::Send {
            payload,
            data_type: DataType::DataMdr,
            wait: Wait::Ack,
            commit: Some(commit),
        }
    }
}

#[derive(Debug, Default)]
struct Task {
    steps: VecDeque<Step>,
    /// Deadline of the current step's wait.
    deadline: Option<Instant>,
    /// What the current step is waiting for.
    current_wait: Option<Wait>,
    /// Payload of the last command sent, for retransmission on timeout.
    last_send: Option<(Vec<u8>, DataType)>,
    /// How many times the current step has already been retransmitted.
    retries: u8,
}

/// The engine. Generic over the transport so tests can use the mock.
pub struct Engine<T: Transport> {
    pub state: DeviceState,
    pub props: Properties,
    conn: T,
    recv_buf: VecDeque<u8>,
    send_buf: VecDeque<u8>,
    /// Independent tx sequence number, toggled per data frame sent.
    tx_seq: u8,
    task: Option<Task>,
    last_error: Option<String>,
}

impl<T: Transport> Engine<T> {
    pub fn new(conn: T) -> Self {
        Self {
            state: DeviceState::default(),
            props: Properties::default(),
            conn,
            recv_buf: VecDeque::with_capacity(4096),
            send_buf: VecDeque::with_capacity(4096),
            tx_seq: 0,
            task: None,
            last_error: None,
        }
    }

    pub fn last_error(&self) -> Option<&str> {
        self.last_error.as_deref()
    }

    pub fn is_ready(&self) -> bool {
        self.task.is_none()
    }

    /// Test-only: bytes buffered from the device.
    pub fn recv_len_for_tests(&self) -> usize {
        self.recv_buf.len()
    }

    /// Test-only: bytes queued for sending.
    pub fn send_len_for_tests(&self) -> usize {
        self.send_buf.len()
    }

    pub fn into_conn(self) -> T {
        self.conn
    }

    // ------------------------------------------------------------------
    // Task scheduling
    // ------------------------------------------------------------------

    pub fn request_init(&mut self) -> Result<(), EngineError> {
        if self.task.is_some() {
            return Err(EngineError::TaskInProgress);
        }
        self.task = Some(Task {
            steps: build_init_steps(&self.state).into(),
            deadline: None,
            current_wait: None,
            last_send: None,
            retries: 0,
        });
        self.advance_task();
        Ok(())
    }

    pub fn request_sync(&mut self) -> Result<(), EngineError> {
        if self.task.is_some() {
            return Err(EngineError::TaskInProgress);
        }
        self.task = Some(Task {
            steps: build_sync_steps(&self.state).into(),
            deadline: None,
            current_wait: None,
            last_send: None,
            retries: 0,
        });
        self.advance_task();
        Ok(())
    }

    pub fn request_commit(&mut self) -> Result<(), EngineError> {
        if self.task.is_some() {
            return Err(EngineError::TaskInProgress);
        }
        if !self.props.is_dirty() {
            return Ok(());
        }
        let steps = build_commit_steps(&self.state, &mut self.props);
        if steps.is_empty() {
            return Ok(());
        }
        self.task = Some(Task {
            steps: steps.into(),
            deadline: None,
            current_wait: None,
            last_send: None,
            retries: 0,
        });
        self.advance_task();
        Ok(())
    }

    /// Advances the task: pops completed steps and starts the next one.
    /// Advances the task: pops completed steps and starts the next one.
    fn advance_task(&mut self) {
        loop {
            // Extract what we need from the front step first so we can mutate
            // the task afterwards.
            enum Next {
                Send(Vec<u8>, DataType, Wait),
                Await(Wait),
                Conditional,
                Done,
            }
            let next = {
                let Some(task) = self.task.as_mut() else {
                    return;
                };
                let Some(step) = task.steps.front() else {
                    return;
                };
                match step {
                    Step::Send {
                        payload,
                        data_type,
                        wait,
                        ..
                    } => Next::Send(payload.clone(), *data_type, *wait),
                    Step::Await(wait) => Next::Await(*wait),
                    Step::IfTable2(_)
                    | Step::IfSupport1(_, _)
                    | Step::IfSupport2Any(_, _)
                    | Step::NcAsmQuery => Next::Conditional,
                    Step::Done(_) => Next::Done,
                }
            };
            match next {
                Next::Send(payload, data_type, wait) => {
                    self.send_command(&payload, data_type);
                    let task = self.task.as_mut().unwrap();
                    task.last_send = Some((payload, data_type));
                    task.retries = 0;
                    task.deadline = Some(Instant::now() + STEP_TIMEOUT);
                    task.current_wait = Some(wait);
                    return;
                }
                Next::Await(wait) => {
                    let task = self.task.as_mut().unwrap();
                    task.retries = 0;
                    task.deadline = Some(Instant::now() + STEP_TIMEOUT);
                    task.current_wait = Some(wait);
                    return;
                }
                Next::Conditional => {
                    let task = self.task.as_mut().unwrap();
                    let Some(front) = task.steps.pop_front() else {
                        return;
                    };
                    let steps: Vec<Step> = match front {
                        Step::IfTable2(inner) => {
                            if self.state.has_table2 {
                                inner
                            } else {
                                continue;
                            }
                        }
                        Step::IfSupport1(func, inner) => {
                            if self.state.support.contains_t1(func) {
                                inner
                            } else {
                                continue;
                            }
                        }
                        Step::IfSupport2Any(funcs, inner) => {
                            if funcs.iter().any(|f| self.state.support.contains_t2(*f)) {
                                inner
                            } else {
                                continue;
                            }
                        }
                        Step::NcAsmQuery => {
                            use FunctionTable1 as F1;
                            let support = &self.state.support;
                            if support.contains_t1(
                                F1::ModeNcAsmNoiseCancellingDualAmbientSoundModeLevelAdjustment,
                            ) {
                                vec![Step::send(
                                    NcAsmGetParam {
                                        type_: NcAsmInquiredType::ModeNcAsmDualNcModeSwitchAndAsmSeamless,
                                    }
                                    .serialize(),
                                )]
                            } else if support.contains_t1(
                                F1::ModeNcAsmNoiseCancellingDualAmbientSoundModeLevelAdjustmentNoiseAdaptation,
                            ) {
                                vec![Step::send(
                                    NcAsmGetParam {
                                        type_: NcAsmInquiredType::ModeNcAsmDualNcModeSwitchAndAsmSeamlessNa,
                                    }
                                    .serialize(),
                                )]
                            } else if support.contains_t1(F1::AmbientSoundModeLevelAdjustment) {
                                vec![Step::send(
                                    NcAsmGetParam { type_: NcAsmInquiredType::AsmSeamless }
                                        .serialize(),
                                )]
                            } else {
                                continue;
                            }
                        }
                        _ => unreachable!("conditional step already handled"),
                    };
                    let mut steps = steps;
                    let rest: Vec<Step> = task.steps.drain(..).collect();
                    steps.extend(rest);
                    task.steps = steps.into();
                    // Loop to process the next step.
                }
                Next::Done => return,
            }
        }
    }

    /// Checks whether the current step's wait has been satisfied by `frame`
    /// and advances the task accordingly. Returns the emitted event, if any.
    fn step_completed(&mut self) -> Option<DeviceEvent> {
        let task = self.task.as_mut()?;
        match task.steps.front() {
            Some(Step::Done(ev)) => {
                let ev = ev.clone();
                task.steps.pop_front();
                self.task = None;
                log::debug!("engine: task completed with {ev:?}");
                Some(ev)
            }
            _ => None,
        }
    }

    fn advance_on(&mut self, wait: Wait) -> Option<DeviceEvent> {
        let satisfied = self
            .task
            .as_ref()
            .map(|t| t.current_wait == Some(wait))
            .unwrap_or(false);
        if !satisfied {
            return None;
        }
        let task = self.task.as_mut().unwrap();
        task.current_wait = None;
        task.deadline = None;
        // Apply any commit attached to the completed step.
        if let Some(Step::Send {
            commit: Some(c), ..
        }) = task.steps.front()
        {
            c.apply(&mut self.props);
        }
        // A Done step may follow immediately.
        task.steps.pop_front();
        // `IfTable2` needs to be expanded before the next step starts.
        self.advance_task();
        self.step_completed()
    }

    /// Times out the current step if its deadline passed.
    fn check_deadline(&mut self) -> Option<DeviceEvent> {
        let timed_out = self
            .task
            .as_ref()
            .and_then(|t| t.deadline)
            .map(|d| Instant::now() >= d)
            .unwrap_or(false);
        if !timed_out {
            return None;
        }
        // Retransmit the command a few times before failing the task: the
        // device may have dropped the frame (e.g. as a seq duplicate during
        // a notification burst) or it was lost on the air. The reference
        // client retries up to 10 times (`kAwaitAckRetries`); we cap it to
        // keep the worst case bounded. Each resend goes out with a fresh
        // toggled seq, which also restores alternation after a duplicate.
        let resend = {
            let task = self.task.as_mut().unwrap();
            if task.retries < MAX_STEP_RETRIES {
                task.last_send.clone()
            } else {
                None
            }
        };
        if let Some((payload, data_type)) = resend {
            let attempt = self.task.as_ref().unwrap().retries + 2;
            log::warn!("engine: step timed out, retransmitting (attempt {attempt})");
            self.send_command(&payload, data_type);
            let task = self.task.as_mut().unwrap();
            task.retries += 1;
            task.deadline = Some(Instant::now() + STEP_TIMEOUT);
            return None;
        }
        self.fail_task(EngineError::Timeout)
    }

    fn fail_task(&mut self, error: EngineError) -> Option<DeviceEvent> {
        log::warn!("engine: task failed: {error}");
        self.task = None;
        self.last_error = Some(error.to_string());
        Some(DeviceEvent::Error(error))
    }

    // ------------------------------------------------------------------
    // I/O
    // ------------------------------------------------------------------

    fn send_command(&mut self, payload: &[u8], data_type: DataType) {
        // The device tracks the seq of the last data frame it received from
        // us and silently drops a repeat as a retransmission (observed on
        // the WH-1000XM6: no ACK, no response). Echoing the seq of incoming
        // frames (the reference client's approach) can still produce repeats
        // when notifications arrive between sends, so we maintain our own
        // alternating counter instead.
        let packet = pack(data_type, self.tx_seq, payload);
        self.tx_seq = 1 - self.tx_seq;
        self.send_buf.extend(packet);
    }

    /// Sends queued bytes; `Ok(())` when everything was flushed.
    async fn flush_send_buf(&mut self) -> Result<(), TransportError> {
        while !self.send_buf.is_empty() {
            let mut chunk = [0u8; 512];
            let n = self.send_buf.len().min(chunk.len());
            for (i, b) in self.send_buf.drain(..n).enumerate() {
                chunk[i] = b;
            }
            let sent = self.conn.send(&chunk[..n]).await?;
            log::trace!("engine: tx {sent} bytes");
            if sent == 0 {
                return Err(TransportError::NoConnection);
            }
            // If the transport only accepted part of the chunk, put the rest
            // back (the mock always takes everything).
            let rest = &chunk[sent.min(n)..n];
            if !rest.is_empty() {
                let mut back = VecDeque::from(rest.to_vec());
                back.append(&mut self.send_buf);
                self.send_buf = back;
            }
            if sent < n {
                break; // Try again on the next poll.
            }
        }
        Ok(())
    }

    async fn recv_into_buffer(&mut self) -> Result<usize, TransportError> {
        let mut buf = [0u8; MAX_FRAME];
        let n = self.conn.recv(&mut buf).await?;
        if n == 0 {
            return Err(TransportError::Closed);
        }
        self.recv_buf.extend(buf[..n].iter().copied());
        Ok(n)
    }

    /// Extracts and processes one complete frame from the receive buffer.
    /// Returns the event the frame produced (if any).
    fn process_frame(&mut self) -> Option<DeviceEvent> {
        if self.recv_buf.is_empty() {
            return None;
        }
        // Find start marker.
        let start = self.recv_buf.iter().position(|&b| b == START_MARKER)?;
        if start > 0 {
            self.recv_buf.drain(..start);
        }
        // Find end marker after the start.
        let end = self.recv_buf.iter().position(|&b| b == END_MARKER)?;
        if end == 0 {
            // A lone end marker with no start: drop it.
            self.recv_buf.pop_front();
            return None;
        }
        let frame: Vec<u8> = self.recv_buf.drain(..=end).collect();
        match unpack_full(&frame) {
            Ok((data_type, seq, data)) => self.handle_frame(data_type, seq, data),
            Err(UnpackResult::Incomplete) => {
                // Frame might be split across reads — but we already consumed
                // the buffer. If the frame is truncated (no end marker), we
                // must have it back to await the rest.
                if frame[frame.len() - 1] != END_MARKER {
                    let mut back = frame.into_iter().collect::<VecDeque<_>>();
                    back.append(&mut self.recv_buf);
                    self.recv_buf = back;
                }
                None
            }
            Err(e) => {
                log::warn!("engine: dropping rejected frame: {e:?} ({:02X?})", frame);
                None
            }
        }
    }

    fn handle_frame(&mut self, data_type: DataType, seq: u8, data: Vec<u8>) -> Option<DeviceEvent> {
        log::trace!("engine: rx {data_type:?} seq={seq}: {:02X?}", data);
        match data_type {
            DataType::Ack => self.advance_on(Wait::Ack),
            DataType::DataMdr => {
                self.send_ack(seq);
                self.handle_command_t1(&data)
            }
            DataType::DataMdrNo2 => {
                self.send_ack(seq);
                self.handle_command_t2(&data)
            }
            _ => None,
        }
    }

    /// ACKs a received data frame: seq = 1 - received seq (mirrors the
    /// reference client's `SendACK`).
    fn send_ack(&mut self, seq: u8) {
        let packet = pack(DataType::Ack, 1 - seq, &[]);
        self.send_buf.extend(packet);
    }

    /// One poll iteration. Returns the next event, or `None` when idle.
    pub async fn poll(&mut self, io_timeout: Duration) -> Option<DeviceEvent> {
        // 1. Step deadline.
        if let Some(ev) = self.check_deadline() {
            return Some(ev);
        }

        // 2. Flush outgoing data.
        if let Err(e) = self.flush_send_buf().await {
            log::warn!("engine: send failed: {e}");
            return Some(DeviceEvent::Error(EngineError::Transport(e)));
        }

        // 3. Incoming data.
        match self.conn.poll_read(io_timeout).await {
            Ok(PollStatus::Ready) => {
                if let Err(e) = self.recv_into_buffer().await {
                    log::warn!("engine: recv failed: {e}");
                    return Some(DeviceEvent::Error(EngineError::Transport(e)));
                }
            }
            // A quiet poll is not an error.
            Ok(PollStatus::Timeout) | Err(TransportError::Timeout) => {}
            Err(e) => {
                log::warn!("engine: poll failed: {e}");
                return Some(DeviceEvent::Error(EngineError::Transport(e)));
            }
        }

        // 4. Process a complete frame if one is buffered — even if the
        //    transport reported no new data (frames are consumed one per
        //    poll, and the device may have responded with several at once).
        if let Some(ev) = self.process_frame() {
            return Some(ev);
        }

        // 5. A frame may have completed the current task (e.g. an ACK
        //    arriving for a fire-and-forget command).
        self.step_completed()
    }

    // ------------------------------------------------------------------
    // Command dispatch (Table 1)
    // ------------------------------------------------------------------

    fn handle_command_t1(&mut self, data: &[u8]) -> Option<DeviceEvent> {
        let cmd = *data.first()?;
        match CommandT1::from_u8(cmd) {
            CommandT1::ConnectRetProtocolInfo => {
                let Ok(p) = ConnectRetProtocolInfo::deserialize(data) else {
                    return None;
                };
                self.state.protocol_version = p.protocol_version;
                self.state.has_table1 = p.support_table1 == EnableDisable::Enable;
                self.state.has_table2 = p.support_table2 == EnableDisable::Enable;
                self.advance_on(Wait::ProtocolInfo)
            }
            CommandT1::ConnectRetCapabilityInfo => {
                // [0x03, 0x00, counter, prefixed uniqueID]
                let mut r = codec::Reader::new(data);
                let _ = (r.u8(), r.u8());
                if let (Ok(_), Ok(uid)) = (r.u8(), codec::read_prefixed_string(&mut r)) {
                    self.state.unique_id = uid;
                }
                None
            }
            CommandT1::ConnectRetDeviceInfo => match DeviceInfoResponse::deserialize(data) {
                Ok(DeviceInfoResponse::ModelName(n)) => {
                    self.state.model_name = n;
                    Some(DeviceEvent::DeviceInfo)
                }
                Ok(DeviceInfoResponse::FwVersion(v)) => {
                    self.state.fw_version = v;
                    Some(DeviceEvent::DeviceInfo)
                }
                Ok(DeviceInfoResponse::SeriesAndColor { series, color }) => {
                    self.state.model_series = series;
                    self.state.model_color = color;
                    Some(DeviceEvent::DeviceInfo)
                }
                Err(_) => None,
            },
            CommandT1::ConnectRetSupportFunction => {
                if let Ok(p) = ConnectRetSupportFunction::deserialize(data) {
                    for f in p.functions {
                        self.state.support.set_t1(f.function, true);
                    }
                    self.advance_on(Wait::SupportFunction)
                } else {
                    None
                }
            }
            CommandT1::CommonRetStatus | CommandT1::CommonNtfyStatus => {
                if let Ok(p) = CommonStatusAudioCodec::deserialize(data) {
                    self.state.audio_codec = p.audio_codec;
                    Some(DeviceEvent::Codec)
                } else {
                    None
                }
            }
            CommandT1::PowerRetStatus => {
                if let Ok(p) = PowerRetStatusBattery::deserialize(data) {
                    use super::state::BatteryState;
                    let b = |lvl: u8, ch: BatteryChargingStatus, th: u8| BatteryState {
                        level: lvl,
                        threshold: th,
                        charging: ch,
                    };
                    match p.type_ {
                        PowerInquiredType::LeftRightBattery
                        | PowerInquiredType::LrBatteryWithThreshold => {
                            self.state.battery_left =
                                b(p.left.level, p.left.charging, p.left.threshold);
                            self.state.battery_right =
                                b(p.right.level, p.right.charging, p.right.threshold);
                        }
                        PowerInquiredType::CradleBattery
                        | PowerInquiredType::CradleBatteryWithThreshold => {
                            self.state.battery_case =
                                b(p.case_.level, p.case_.charging, p.case_.threshold);
                        }
                        _ => {
                            self.state.battery_left =
                                b(p.left.level, p.left.charging, p.left.threshold);
                        }
                    }
                    Some(DeviceEvent::Battery)
                } else {
                    None
                }
            }
            CommandT1::PowerRetParam | CommandT1::PowerNtfyParam => {
                if let Ok(p) = PowerParamAutoPowerOff::deserialize(data) {
                    match p.type_ {
                        PowerInquiredType::AutoPowerOff => {
                            self.props
                                .auto_power_off
                                .overwrite(AutoPowerOffElements::from_u8(p.current));
                            Some(DeviceEvent::AutoPowerOff)
                        }
                        PowerInquiredType::AutoPowerOffWearingDetection => {
                            self.props
                                .auto_power_off
                                .overwrite(AutoPowerOffElements::from_u8(p.current));
                            Some(DeviceEvent::AutoPowerOff)
                        }
                        _ => None,
                    }
                } else {
                    None
                }
            }
            CommandT1::EqEbbRetStatus | CommandT1::EqEbbNtfyStatus => {
                // [0x53, 0x00, onOff]
                if data.len() >= 3 {
                    let available = data[2] == OnOffSetting::On.to_u8();
                    self.props.eq_available.overwrite(available);
                    log::debug!(
                        "engine: EQ status: available={available} (raw {:02X?})",
                        &data[..data.len().min(6)]
                    );
                    Some(DeviceEvent::Equalizer)
                } else {
                    log::warn!("engine: EQ status frame too short: {:02X?}", data);
                    None
                }
            }
            CommandT1::EqEbbRetParam | CommandT1::EqEbbNtfyParam => {
                if let Ok(p) = EqEbbParamEq::deserialize(data) {
                    log::debug!(
                        "engine: EQ param: preset={:?} {} band(s) (raw {:02X?})",
                        p.preset_id,
                        p.bands.len(),
                        &data[..data.len().min(24)]
                    );
                    self.props.eq_preset_id.overwrite(p.preset_id);
                    // Mirrors the reference client: a 6-band report carries
                    // clear bass + five bands, a 10-band report carries ten.
                    match p.bands.len() {
                        6 => {
                            self.props.eq_clear_bass.overwrite(p.bands[0] as i8 - 10);
                            self.props
                                .eq_config
                                .overwrite(p.bands[1..].iter().map(|&b| b as i8 - 10).collect());
                        }
                        10 => {
                            self.props.eq_clear_bass.overwrite(0);
                            self.props
                                .eq_config
                                .overwrite(p.bands.iter().map(|&b| b as i8 - 6).collect());
                        }
                        _ => {}
                    }
                    Some(DeviceEvent::Equalizer)
                } else {
                    None
                }
            }
            CommandT1::NcAsmRetParam | CommandT1::NcAsmNtfyParam => {
                let &ty = data.get(1)?;
                match NcAsmInquiredType::from_u8(ty) {
                    NcAsmInquiredType::ModeNcAsmDualNcModeSwitchAndAsmSeamless => {
                        if let Ok(p) = NcAsmParamModeNcDualModeSwitchAsmSeamless::deserialize(data)
                        {
                            apply_nc_asm(
                                &mut self.props,
                                p.base.nc_asm_total_effect,
                                Some(p.nc_asm_mode),
                                p.ambient_sound_mode,
                                p.ambient_sound_level,
                                None,
                                None,
                            );
                            Some(DeviceEvent::NcAsm)
                        } else {
                            None
                        }
                    }
                    NcAsmInquiredType::ModeNcAsmDualNcModeSwitchAndAsmSeamlessNa => {
                        if let Ok(p) =
                            NcAsmParamModeNcDualModeSwitchAsmSeamlessNa::deserialize(data)
                        {
                            apply_nc_asm(
                                &mut self.props,
                                p.base.nc_asm_total_effect,
                                Some(p.nc_asm_mode),
                                p.ambient_sound_mode,
                                p.ambient_sound_level,
                                Some(p.noise_adaptive_on_off),
                                Some(p.noise_adaptive_sensitivity),
                            );
                            Some(DeviceEvent::NcAsm)
                        } else {
                            None
                        }
                    }
                    NcAsmInquiredType::AsmSeamless => {
                        if let Ok(p) = NcAsmParamAsmSeamless::deserialize(data) {
                            apply_nc_asm(
                                &mut self.props,
                                p.base.nc_asm_total_effect,
                                None,
                                p.ambient_sound_mode,
                                p.ambient_sound_level,
                                None,
                                None,
                            );
                            Some(DeviceEvent::NcAsm)
                        } else {
                            None
                        }
                    }
                    NcAsmInquiredType::NcAmbToggle => {
                        if let Ok(p) = NcAsmParamNcAmbToggle::deserialize(data) {
                            self.props.nc_asm_button_function.overwrite(p.function);
                            Some(DeviceEvent::NcAsm)
                        } else {
                            None
                        }
                    }
                    _ => None,
                }
            }
            CommandT1::PlayRetStatus | CommandT1::PlayNtfyStatus => {
                if let Ok(p) = PlayStatusPlaybackController::deserialize(data) {
                    log::debug!(
                        "engine: playback status {:?} (raw {data:02X?}, status={:?}, music_call={:?})",
                        p.playback_status,
                        p.status,
                        p.music_call_status
                    );
                    self.state.play_status = p.playback_status;
                    Some(DeviceEvent::PlaybackStatus)
                } else {
                    None
                }
            }
            CommandT1::PlayRetParam | CommandT1::PlayNtfyParam => {
                let &ty = data.get(1)?;
                match PlayInquiredType::from_u8(ty) {
                    PlayInquiredType::PlaybackControlWithCallVolumeAdjustment
                    | PlayInquiredType::PlaybackControlWithCallVolumeAdjustmentAndFunctionChange
                    | PlayInquiredType::PlaybackControlWithFunctionChange => {
                        if let Ok(p) = PlayParamPlaybackControllerName::deserialize(data) {
                            if p.names.len() >= 3 {
                                self.state.play_title = p.names[0].name.clone();
                                self.state.play_album = p.names[1].name.clone();
                                self.state.play_artist = p.names[2].name.clone();
                            }
                            Some(DeviceEvent::PlaybackMetadata)
                        } else {
                            None
                        }
                    }
                    PlayInquiredType::MusicVolume
                    | PlayInquiredType::MusicVolumeWithMute
                    | PlayInquiredType::CallVolume
                    | PlayInquiredType::CallVolumeWithMute => {
                        if let Ok(p) = PlayParamPlaybackControllerVolume::deserialize(data) {
                            self.state.play_volume = p.volume;
                            self.props.play_volume.overwrite(p.volume);
                            Some(DeviceEvent::Volume)
                        } else {
                            None
                        }
                    }
                    _ => None,
                }
            }
            CommandT1::GeneralSettingRetCapability => {
                if let Ok(c) = GsRetCapability::deserialize(data) {
                    self.state.gs_capabilities.push(c.into_capability());
                    Some(DeviceEvent::GeneralSetting)
                } else {
                    None
                }
            }
            CommandT1::GeneralSettingRetParam | CommandT1::GeneralSettingNtfyParam => {
                if let Ok(p) = GsParamBoolean::deserialize(data) {
                    let idx = match p.type_ {
                        GsInquiredType::GeneralSetting1 => 0,
                        GsInquiredType::GeneralSetting2 => 1,
                        GsInquiredType::GeneralSetting3 => 2,
                        _ => 3,
                    };
                    self.props.gs_param_bool[idx].overwrite(p.setting_value == GsSettingValue::On);
                    Some(DeviceEvent::GeneralSetting)
                } else {
                    None
                }
            }
            CommandT1::AudioRetCapability => {
                if let Ok(p) = AudioRetCapabilityUpscaling::deserialize(data) {
                    self.state.upscaling_type = p.upscaling_type;
                    self.state.upscaling_available = true;
                    Some(DeviceEvent::Upscaling)
                } else {
                    None
                }
            }
            CommandT1::AudioRetStatus | CommandT1::AudioNtfyStatus => {
                if let Ok(p) = AudioStatusCommon::deserialize(data) {
                    match p.type_ {
                        AudioInquiredType::Upscaling => {
                            self.props
                                .upscaling_enabled
                                .overwrite(p.status == EnableDisable::Enable);
                            Some(DeviceEvent::Upscaling)
                        }
                        _ => None,
                    }
                } else {
                    None
                }
            }
            CommandT1::AudioRetParam | CommandT1::AudioNtfyParam => {
                let &ty = data.get(1)?;
                match AudioInquiredType::from_u8(ty) {
                    AudioInquiredType::Upscaling => {
                        if let Ok(p) = AudioParamUpscaling::deserialize(data) {
                            self.props
                                .upscaling_enabled
                                .overwrite(p.setting_value == UpscalingTypeAutoOff::Auto);
                            Some(DeviceEvent::Upscaling)
                        } else {
                            None
                        }
                    }
                    AudioInquiredType::ConnectionMode => {
                        if let Ok(_p) = AudioParamConnection::deserialize(data) {
                            // Not surfaced in the tray UI; tracked for completeness.
                            Some(DeviceEvent::Codec)
                        } else {
                            None
                        }
                    }
                    AudioInquiredType::BgmMode | AudioInquiredType::BgmModeAndErrorCode => {
                        if let Ok(p) = AudioParamBGMMode::deserialize(data) {
                            self.props
                                .bgm_mode_enabled
                                .overwrite(p.on_off == EnableDisable::Enable);
                            self.props.bgm_mode_room_size.overwrite(p.target_room_size);
                            Some(DeviceEvent::ListeningMode)
                        } else {
                            None
                        }
                    }
                    AudioInquiredType::UpmixCinema => {
                        if let Ok(p) = AudioParamUpmixCinema::deserialize(data) {
                            self.props
                                .upmix_cinema_enabled
                                .overwrite(p.on_off == EnableDisable::Enable);
                            Some(DeviceEvent::ListeningMode)
                        } else {
                            None
                        }
                    }
                    _ => None,
                }
            }
            CommandT1::SystemRetParam | CommandT1::SystemNtfyParam => {
                let &ty = data.get(1)?;
                match SystemInquiredType::from_u8(ty) {
                    SystemInquiredType::SmartTalkingModeType2 => {
                        if let Ok(p) = SystemParamSmartTalking::deserialize(data) {
                            self.props
                                .speak_to_chat_enabled
                                .overwrite(p.on_off == EnableDisable::Enable);
                            Some(DeviceEvent::SpeakToChat)
                        } else {
                            None
                        }
                    }
                    SystemInquiredType::AssignableSettings => {
                        if let Ok(p) = SystemParamAssignableSettings::deserialize(data) {
                            if p.presets.len() >= 2 {
                                self.props.touch_function_left.overwrite(p.presets[0]);
                                self.props.touch_function_right.overwrite(p.presets[1]);
                            }
                            Some(DeviceEvent::TouchFunctions)
                        } else {
                            None
                        }
                    }
                    SystemInquiredType::PlaybackControlByWearing => {
                        if let Ok(p) = SystemParamCommon::deserialize(data) {
                            self.props
                                .auto_pause_enabled
                                .overwrite(p.setting_value == EnableDisable::Enable);
                            Some(DeviceEvent::AutoPause)
                        } else {
                            None
                        }
                    }
                    SystemInquiredType::HeadGestureOnOff => {
                        if let Ok(p) = SystemParamCommon::deserialize(data) {
                            self.props
                                .head_gesture_enabled
                                .overwrite(p.setting_value == EnableDisable::Enable);
                            Some(DeviceEvent::AutoPause)
                        } else {
                            None
                        }
                    }
                    _ => None,
                }
            }
            CommandT1::SystemRetExtParam | CommandT1::SystemNtfyExtParam => {
                if let Ok(p) = SystemExtParamSmartTalkingMode2::deserialize(data) {
                    self.props
                        .speak_to_chat_detect_sensitivity
                        .overwrite(p.detect_sensitivity);
                    self.props.speak_to_mode_out_time.overwrite(p.mode_off_time);
                    Some(DeviceEvent::SpeakToChat)
                } else {
                    None
                }
            }
            CommandT1::AlertNtfyParam => {
                if let Ok(p) = AlertNotifyParamFixedMessage::deserialize(data) {
                    Some(DeviceEvent::Alert(p.message_type))
                } else {
                    None
                }
            }
            _ => None,
        }
    }

    // ------------------------------------------------------------------
    // Command dispatch (Table 2)
    // ------------------------------------------------------------------

    fn handle_command_t2(&mut self, data: &[u8]) -> Option<DeviceEvent> {
        let cmd = *data.first()?;
        match CommandT2::from_u8(cmd) {
            CommandT2::ConnectRetSupportFunction => {
                if let Ok(p) = ConnectRetSupportFunction::deserialize(data) {
                    for f in p.functions {
                        self.state.support.set_t2(f.function, true);
                    }
                    self.advance_on(Wait::SupportFunction)
                } else {
                    None
                }
            }
            CommandT2::VoiceGuidanceRetParam | CommandT2::VoiceGuidanceNtfyParam => {
                let &ty = data.get(1)?;
                match VoiceGuidanceInquiredType::from_u8(ty) {
                    VoiceGuidanceInquiredType::MtkTransferWoDisconnectionSupportLanguageSwitch
                    | VoiceGuidanceInquiredType::OnlyOnOffSetting => {
                        if let Ok(p) = VoiceGuidanceParamSettingMtk::deserialize(data) {
                            self.props
                                .voice_guidance_enabled
                                .overwrite(p.setting_value == OnOffSetting::On);
                            Some(DeviceEvent::VoiceGuidance)
                        } else {
                            None
                        }
                    }
                    VoiceGuidanceInquiredType::Volume => {
                        if let Ok(p) = VoiceGuidanceSetParamVolume::deserialize(data) {
                            self.props.voice_guidance_volume.overwrite(p.volume);
                            Some(DeviceEvent::VoiceGuidance)
                        } else {
                            None
                        }
                    }
                    _ => None,
                }
            }
            CommandT2::PeriRetParam => {
                if let Ok(p) = PeripheralRetParamDeviceList::deserialize(data) {
                    self.apply_multipoint_list(p);
                    Some(DeviceEvent::Multipoint)
                } else {
                    None
                }
            }
            CommandT2::PeriNtfyExtendedParam => {
                let ty = *data.get(1).unwrap_or(&0xFF);
                if ty == PeripheralInquiredType::SourceSwitchControl.to_u8() {
                    if let Ok(p) = PeripheralNotifyExtendedParamSourceSwitch::deserialize(data) {
                        if p.result == SourceSwitchControlResult::Success {
                            log::debug!("engine: multipoint switched to {}", p.address);
                            self.state.multipoint_playback = self
                                .state
                                .multipoint_devices
                                .iter()
                                .position(|d| d.address == p.address);
                            Some(DeviceEvent::Multipoint)
                        } else {
                            log::warn!("engine: multipoint switch failed: {:?}", p.result);
                            None
                        }
                    } else {
                        None
                    }
                } else if let Ok(p) =
                    PeripheralNotifyExtendedParamDeviceManagement::deserialize(data)
                {
                    log::debug!(
                        "engine: peripheral {:?} result 0x{:02X} for {}",
                        p.action,
                        p.result,
                        p.address
                    );
                    None
                } else {
                    None
                }
            }
            _ => None,
        }
    }

    /// Adopts a fresh multipoint device list. The playback device is the one
    /// whose connection-slot status matches the reported `playback_device`
    /// byte (mirroring the reference client, which compares
    /// `connectedStatus == playbackDevice` rather than indexing the list).
    fn apply_multipoint_list(&mut self, p: PeripheralRetParamDeviceList) {
        self.state.multipoint_devices = p
            .devices
            .into_iter()
            .map(|d| MultipointDevice {
                address: d.address,
                name: d.name,
                connected_status: d.connected_status,
            })
            .collect();
        self.state.multipoint_playback = self
            .state
            .multipoint_devices
            .iter()
            .position(|d| d.connected_status == p.playback_device);
        log::debug!(
            "engine: multipoint list ({} device(s), playback #{:?})",
            self.state.multipoint_devices.len(),
            self.state.multipoint_playback
        );
    }
}

fn apply_nc_asm(
    props: &mut Properties,
    total_effect: NcAsmOnOffValue,
    mode: Option<NcAsmMode>,
    ambient_mode: AmbientSoundMode,
    level: u8,
    noise_adaptive: Option<NcAsmOnOffValue>,
    sensitivity: Option<NoiseAdaptiveSensitivity>,
) {
    props
        .nc_asm_enabled
        .overwrite(total_effect == NcAsmOnOffValue::On);
    if let Some(m) = mode {
        props.nc_asm_mode.overwrite(m);
    }
    props
        .nc_asm_focus_on_voice
        .overwrite(ambient_mode == AmbientSoundMode::Voice);
    if level > 0 {
        props.nc_asm_ambient_level.overwrite(level.clamp(1, 20));
    }
    if let Some(na) = noise_adaptive {
        props
            .nc_asm_auto_asm_enabled
            .overwrite(na == NcAsmOnOffValue::On);
    }
    if let Some(s) = sensitivity {
        props.nc_asm_noise_adaptive_sensitivity.overwrite(s);
    }
}

// ---------------------------------------------------------------------------
// Task builders
// ---------------------------------------------------------------------------

fn gs_type(idx: usize) -> GsInquiredType {
    match idx {
        0 => GsInquiredType::GeneralSetting1,
        1 => GsInquiredType::GeneralSetting2,
        2 => GsInquiredType::GeneralSetting3,
        _ => GsInquiredType::GeneralSetting4,
    }
}

fn build_init_steps(state: &DeviceState) -> Vec<Step> {
    use FunctionTable1 as F1;
    use FunctionTable2 as F2;
    let _s = &state.support;
    let mut steps = vec![Step::send(ConnectGetProtocolInfo.serialize())];
    steps.push(Step::Await(Wait::ProtocolInfo));
    steps.push(Step::send(ConnectGetCapabilityInfo.serialize()));
    steps.push(Step::send(
        ConnectGetDeviceInfo {
            device_info_type: DeviceInfoType::FwVersion,
        }
        .serialize(),
    ));
    steps.push(Step::send(
        ConnectGetDeviceInfo {
            device_info_type: DeviceInfoType::ModelName,
        }
        .serialize(),
    ));
    steps.push(Step::send(
        ConnectGetDeviceInfo {
            device_info_type: DeviceInfoType::SeriesAndColorInfo,
        }
        .serialize(),
    ));
    steps.push(Step::send(ConnectGetSupportFunction.serialize()));
    steps.push(Step::Await(Wait::SupportFunction));
    steps.push(Step::IfTable2(vec![
        Step::send_t2(T2ConnectGetSupportFunction.serialize()),
        Step::Await(Wait::SupportFunction),
    ]));

    // Multipoint: paired/connected device list. Both variants are queried
    // when advertised (the class-of-device one takes precedence on the
    // device side, mirroring `RequestInitV2` in the reference client).
    steps.push(Step::IfSupport2Any(
        vec![
            F2::PairingDeviceManagementWithBluetoothClassOfDeviceClassicBt,
            F2::PairingDeviceManagementWithBluetoothClassOfDeviceClassicLe,
        ],
        vec![Step::send_t2(
            PeripheralGetParam {
                type_: PeripheralInquiredType::PairingDeviceManagementWithBluetoothClassOfDevice,
            }
            .serialize(),
        )],
    ));
    steps.push(Step::IfSupport2Any(
        vec![F2::PairingDeviceManagementClassicBt],
        vec![Step::send_t2(
            PeripheralGetParam {
                type_: PeripheralInquiredType::PairingDeviceManagementClassicBt,
            }
            .serialize(),
        )],
    ));

    // General settings (capabilities + values).
    for i in 0..4 {
        let f = match i {
            0 => F1::GeneralSetting1,
            1 => F1::GeneralSetting2,
            2 => F1::GeneralSetting3,
            _ => F1::GeneralSetting4,
        };
        let inner = vec![
            Step::send(
                GsGetCapability {
                    type_: gs_type(i),
                    display_language: DisplayLanguage::English,
                }
                .serialize(),
            ),
            Step::send(GsGetParam { type_: gs_type(i) }.serialize()),
        ];
        steps.push(Step::IfSupport1(f, inner));
    }

    // DSEE capability.
    steps.push(Step::IfSupport1(
        F1::UpscalingAutoOff,
        vec![Step::send(
            AudioGetCapability {
                type_: AudioInquiredType::Upscaling,
            }
            .serialize(),
        )],
    ));
    // Fixed message alerts.
    steps.push(Step::IfSupport1(
        F1::FixedMessage,
        vec![Step::send(
            AlertSetStatusFixedMessage {
                status: EnableDisable::Enable,
            }
            .serialize(),
        )],
    ));
    // Codec.
    steps.push(Step::IfSupport1(
        F1::CodecIndicator,
        vec![Step::send(
            CommonGetStatus {
                type_: CommonInquiredType::AudioCodec,
            }
            .serialize(),
        )],
    ));
    // Playback metadata + volume + status.
    steps.push(Step::send(
        GetPlayParam {
            type_: PlayInquiredType::PlaybackControlWithCallVolumeAdjustment,
        }
        .serialize(),
    ));
    steps.push(Step::send(
        GetPlayParam {
            type_: PlayInquiredType::MusicVolume,
        }
        .serialize(),
    ));
    steps.push(Step::send(
        GetPlayStatus {
            type_: PlayInquiredType::PlaybackControlWithCallVolumeAdjustment,
        }
        .serialize(),
    ));

    // NC/ASM state (resolved lazily from the advertised support).
    steps.push(Step::NcAsmQuery);

    // Speak to Chat.
    steps.push(Step::IfSupport1(
        F1::SmartTalkingModeType2,
        vec![
            Step::send(
                SystemGetParam {
                    type_: SystemInquiredType::SmartTalkingModeType2,
                }
                .serialize(),
            ),
            Step::send(
                SystemGetExtParam {
                    type_: SystemInquiredType::SmartTalkingModeType2,
                }
                .serialize(),
            ),
        ],
    ));
    // Listening mode.
    steps.push(Step::IfSupport1(
        F1::ListeningOption,
        vec![
            Step::send(
                AudioGetParam {
                    type_: AudioInquiredType::BgmModeAndErrorCode,
                }
                .serialize(),
            ),
            Step::send(
                AudioGetParam {
                    type_: AudioInquiredType::UpmixCinema,
                }
                .serialize(),
            ),
        ],
    ));
    // EQ.
    steps.push(Step::send(EqEbbGetStatus.serialize()));
    steps.push(Step::send(EqEbbGetParam.serialize()));
    // Connection quality.
    steps.push(Step::IfSupport1(
        F1::ConnectionModeSoundQualityConnectionQuality,
        vec![Step::send(
            AudioGetParam {
                type_: AudioInquiredType::ConnectionMode,
            }
            .serialize(),
        )],
    ));
    // DSEE state.
    steps.push(Step::IfSupport1(
        F1::UpscalingAutoOff,
        vec![
            Step::send(
                AudioGetStatus {
                    type_: AudioInquiredType::Upscaling,
                }
                .serialize(),
            ),
            Step::send(
                AudioGetParam {
                    type_: AudioInquiredType::Upscaling,
                }
                .serialize(),
            ),
        ],
    ));
    // Touch presets.
    steps.push(Step::IfSupport1(
        F1::AssignableSetting,
        vec![Step::send(
            SystemGetParam {
                type_: SystemInquiredType::AssignableSettings,
            }
            .serialize(),
        )],
    ));
    // NC/AMB button.
    steps.push(Step::IfSupport1(
        F1::AmbientSoundControlModeSelect,
        vec![Step::send(
            NcAsmGetParam {
                type_: NcAsmInquiredType::NcAmbToggle,
            }
            .serialize(),
        )],
    ));
    // Head gesture.
    steps.push(Step::IfSupport1(
        F1::HeadGestureOnOffTraining,
        vec![Step::send(
            SystemGetParam {
                type_: SystemInquiredType::HeadGestureOnOff,
            }
            .serialize(),
        )],
    ));
    // Auto power off.
    steps.push(Step::IfSupport1(
        F1::AutoPowerOff,
        vec![Step::send(
            PowerGetParam {
                type_: PowerInquiredType::AutoPowerOff,
            }
            .serialize(),
        )],
    ));
    steps.push(Step::IfSupport1(
        F1::AutoPowerOffWithWearingDetection,
        vec![Step::send(
            PowerGetParam {
                type_: PowerInquiredType::AutoPowerOffWearingDetection,
            }
            .serialize(),
        )],
    ));
    // Auto pause.
    steps.push(Step::send(
        SystemGetParam {
            type_: SystemInquiredType::PlaybackControlByWearing,
        }
        .serialize(),
    ));

    // Voice guidance (Table 2).
    steps.push(Step::IfTable2(vec![
        Step::send_t2(
            VoiceGuidanceGetParam {
                type_: VoiceGuidanceInquiredType::MtkTransferWoDisconnectionSupportLanguageSwitch,
            }
            .serialize(),
        ),
        Step::send_t2(
            VoiceGuidanceGetParam {
                type_: VoiceGuidanceInquiredType::Volume,
            }
            .serialize(),
        ),
    ]));

    // LOG_SET_STATUS.
    steps.push(Step::send(log_set_status_payload()));
    steps.push(Step::Done(DeviceEvent::InitOk));
    steps
}

fn build_sync_steps(state: &DeviceState) -> Vec<Step> {
    use FunctionTable1 as F1;
    use FunctionTable2 as F2;
    let s = &state.support;
    let mut steps = Vec::new();

    if s.contains_t1(F1::BatteryLevelIndicator) {
        steps.push(Step::send(
            PowerGetStatus {
                type_: PowerInquiredType::Battery,
            }
            .serialize(),
        ));
    } else if s.contains_t1(F1::BatteryLevelWithThreshold) {
        steps.push(Step::send(
            PowerGetStatus {
                type_: PowerInquiredType::BatteryWithThreshold,
            }
            .serialize(),
        ));
    }
    if s.contains_t1(F1::LeftRightBatteryLevelIndicator) {
        steps.push(Step::send(
            PowerGetStatus {
                type_: PowerInquiredType::LeftRightBattery,
            }
            .serialize(),
        ));
    } else if s.contains_t1(F1::LrBatteryLevelWithThreshold) {
        steps.push(Step::send(
            PowerGetStatus {
                type_: PowerInquiredType::LrBatteryWithThreshold,
            }
            .serialize(),
        ));
    }
    if s.contains_t1(F1::CradleBatteryLevelIndicator) {
        steps.push(Step::send(
            PowerGetStatus {
                type_: PowerInquiredType::CradleBattery,
            }
            .serialize(),
        ));
    } else if s.contains_t1(F1::CradleBatteryLevelWithThreshold) {
        steps.push(Step::send(
            PowerGetStatus {
                type_: PowerInquiredType::CradleBatteryWithThreshold,
            }
            .serialize(),
        ));
    }

    // Multipoint device list refresh (both advertised variants; the second
    // overwrites the first with the same data plus the class-of-device field).
    steps.push(Step::IfSupport2Any(
        vec![
            F2::PairingDeviceManagementWithBluetoothClassOfDeviceClassicBt,
            F2::PairingDeviceManagementWithBluetoothClassOfDeviceClassicLe,
        ],
        vec![Step::send_t2(
            PeripheralGetParam {
                type_: PeripheralInquiredType::PairingDeviceManagementWithBluetoothClassOfDevice,
            }
            .serialize(),
        )],
    ));
    steps.push(Step::IfSupport2Any(
        vec![F2::PairingDeviceManagementClassicBt],
        vec![Step::send_t2(
            PeripheralGetParam {
                type_: PeripheralInquiredType::PairingDeviceManagementClassicBt,
            }
            .serialize(),
        )],
    ));

    steps.push(Step::Done(DeviceEvent::SyncOk));
    steps
}

fn build_commit_steps(state: &DeviceState, props: &mut Properties) -> Vec<Step> {
    use FunctionTable1 as F1;
    use FunctionTable2 as F2;
    let s = &state.support;
    let mut steps = Vec::new();

    // Shutdown.
    if props.shutdown.dirty() {
        if s.contains_t1(F1::PowerOff) && props.shutdown.desired {
            steps.push(Step::send_commit(
                PowerSetStatusPowerOff.serialize(),
                PropCommit::Shutdown,
            ));
        } else {
            props.shutdown.overwrite(false);
        }
    }
    // NC/ASM group.
    if props.nc_asm_enabled.dirty()
        || props.nc_asm_mode.dirty()
        || props.nc_asm_ambient_level.dirty()
        || props.nc_asm_focus_on_voice.dirty()
        || props.nc_asm_auto_asm_enabled.dirty()
        || props.nc_asm_noise_adaptive_sensitivity.dirty()
    {
        let base = NcAsmParamBase {
            type_: NcAsmInquiredType::Unknown,
            value_change_status: ValueChangeStatus::Changed,
            nc_asm_total_effect: if props.nc_asm_enabled.desired {
                NcAsmOnOffValue::On
            } else {
                NcAsmOnOffValue::Off
            },
        };
        if s.contains_t1(
            F1::ModeNcAsmNoiseCancellingDualAmbientSoundModeLevelAdjustmentNoiseAdaptation,
        ) {
            steps.push(Step::send_commit(
                NcAsmParamModeNcDualModeSwitchAsmSeamlessNa {
                    base: NcAsmParamBase {
                        type_: NcAsmInquiredType::ModeNcAsmDualNcModeSwitchAndAsmSeamlessNa,
                        ..base
                    },
                    nc_asm_mode: props.nc_asm_mode.desired,
                    ambient_sound_mode: if props.nc_asm_focus_on_voice.desired {
                        AmbientSoundMode::Voice
                    } else {
                        AmbientSoundMode::Normal
                    },
                    ambient_sound_level: props.nc_asm_ambient_level.desired,
                    noise_adaptive_on_off: if props.nc_asm_auto_asm_enabled.desired {
                        NcAsmOnOffValue::On
                    } else {
                        NcAsmOnOffValue::Off
                    },
                    noise_adaptive_sensitivity: props.nc_asm_noise_adaptive_sensitivity.desired,
                }
                .serialize(),
                PropCommit::NcAsmGroup,
            ));
        } else if s.contains_t1(F1::AmbientSoundModeLevelAdjustment) {
            steps.push(Step::send_commit(
                NcAsmParamAsmSeamless {
                    base: NcAsmParamBase {
                        type_: NcAsmInquiredType::AsmSeamless,
                        ..base
                    },
                    ambient_sound_mode: if props.nc_asm_focus_on_voice.desired {
                        AmbientSoundMode::Voice
                    } else {
                        AmbientSoundMode::Normal
                    },
                    ambient_sound_level: props.nc_asm_ambient_level.desired,
                }
                .serialize(),
                PropCommit::NcAsmGroup,
            ));
        } else {
            steps.push(Step::send_commit(
                NcAsmParamModeNcDualModeSwitchAsmSeamless {
                    base: NcAsmParamBase {
                        type_: NcAsmInquiredType::ModeNcAsmDualNcModeSwitchAndAsmSeamless,
                        ..base
                    },
                    nc_asm_mode: props.nc_asm_mode.desired,
                    ambient_sound_mode: if props.nc_asm_focus_on_voice.desired {
                        AmbientSoundMode::Voice
                    } else {
                        AmbientSoundMode::Normal
                    },
                    ambient_sound_level: props.nc_asm_ambient_level.desired,
                }
                .serialize(),
                PropCommit::NcAsmGroup,
            ));
        }
    }
    // NC/AMB button function.
    if s.contains_t1(F1::AmbientSoundControlModeSelect) && props.nc_asm_button_function.dirty() {
        steps.push(Step::send_commit(
            NcAsmParamNcAmbToggle {
                type_: NcAsmInquiredType::NcAmbToggle,
                function: props.nc_asm_button_function.desired,
            }
            .serialize(),
            PropCommit::NcAsmButton,
        ));
    }
    // Volume.
    if props.play_volume.dirty() {
        steps.push(Step::send_commit(
            PlayParamPlaybackControllerVolume {
                type_: PlayInquiredType::MusicVolume,
                volume: props.play_volume.desired,
            }
            .serialize(),
            PropCommit::PlayVolume,
        ));
    }
    // Play control (one-shot).
    if props.play_control.dirty() {
        let control = props.play_control.desired;
        props.play_control.overwrite(PlaybackControl::KeyOff);
        steps.push(Step::send(
            PlayStatusSetPlaybackController {
                status: EnableDisable::Enable,
                control,
            }
            .serialize(),
        ));
    }
    // Multipoint request (one-shot): switch playback or connect a device.
    if let Some(req) = props.multipoint_request.desired.clone() {
        props.multipoint_request.overwrite(None);
        let payload: Option<Vec<u8>> = match req {
            MultipointRequest::Switch { address } => {
                log::info!("engine: switching multipoint playback to {address}");
                Some(PeripheralSetExtendedParamSourceSwitch { address }.serialize())
            }
            MultipointRequest::Connect { address } => {
                log::info!("engine: connecting multipoint device {address}");
                // The reference client prefers the class-of-device variant
                // when advertised, falling back to the classic one.
                let t = if s
                    .contains_t2(F2::PairingDeviceManagementWithBluetoothClassOfDeviceClassicBt)
                    || s.contains_t2(F2::PairingDeviceManagementWithBluetoothClassOfDeviceClassicLe)
                {
                    PeripheralInquiredType::PairingDeviceManagementWithBluetoothClassOfDevice
                } else {
                    PeripheralInquiredType::PairingDeviceManagementClassicBt
                };
                steps.push(Step::send_t2(
                    PeripheralSetExtendedParamDeviceManagement {
                        type_: t,
                        action: ConnectivityActionType::Connect,
                        address,
                    }
                    .serialize(),
                ));
                // Re-query the list so the new connection state shows up in
                // the menu (the device's result notification carries no list).
                steps.push(Step::send_t2(PeripheralGetParam { type_: t }.serialize()));
                None
            }
        };
        if let Some(payload) = payload {
            steps.push(Step::send_t2(payload));
        }
    }
    // Speak to Chat.
    if s.contains_t1(F1::SmartTalkingModeType2) {
        if props.speak_to_chat_enabled.dirty() {
            steps.push(Step::send_commit(
                SystemParamSmartTalking {
                    type_: SystemInquiredType::SmartTalkingModeType2,
                    on_off: if props.speak_to_chat_enabled.desired {
                        EnableDisable::Enable
                    } else {
                        EnableDisable::Disable
                    },
                    preview_mode_on_off: EnableDisable::Disable,
                }
                .serialize(),
                PropCommit::StcEnabled,
            ));
        }
        if props.speak_to_chat_detect_sensitivity.dirty() || props.speak_to_mode_out_time.dirty() {
            steps.push(Step::send_commit(
                SystemExtParamSmartTalkingMode2 {
                    type_: SystemInquiredType::SmartTalkingModeType2,
                    detect_sensitivity: props.speak_to_chat_detect_sensitivity.desired,
                    mode_off_time: props.speak_to_mode_out_time.desired,
                }
                .serialize(),
                PropCommit::StcExt,
            ));
        }
    }
    // Listening mode.
    if s.contains_t1(F1::ListeningOption) {
        if props.bgm_mode_enabled.dirty() || props.bgm_mode_room_size.dirty() {
            steps.push(Step::send_commit(
                AudioParamBGMMode {
                    type_: AudioInquiredType::BgmModeAndErrorCode,
                    on_off: if props.bgm_mode_enabled.desired {
                        EnableDisable::Enable
                    } else {
                        EnableDisable::Disable
                    },
                    target_room_size: props.bgm_mode_room_size.desired,
                }
                .serialize(),
                PropCommit::BgmMode,
            ));
        }
        if props.upmix_cinema_enabled.dirty() {
            steps.push(Step::send_commit(
                AudioParamUpmixCinema {
                    type_: AudioInquiredType::UpmixCinema,
                    on_off: if props.upmix_cinema_enabled.desired {
                        EnableDisable::Enable
                    } else {
                        EnableDisable::Disable
                    },
                }
                .serialize(),
                PropCommit::UpmixCinema,
            ));
        }
    }
    // EQ preset.
    if props.eq_preset_id.dirty() {
        steps.push(Step::send_commit(
            EqEbbParamEq {
                preset_id: props.eq_preset_id.desired,
                bands: Vec::new(),
            }
            .serialize(),
            PropCommit::EqPreset,
        ));
        // Refresh bands afterwards.
        steps.push(Step::send(EqEbbGetParam.serialize()));
    }
    // EQ config / clear bass.
    if props.eq_config.dirty() || props.eq_clear_bass.dirty() {
        let bands = props.eq_config.desired.clone();
        if bands.is_empty() {
            // Nothing to send (e.g. the device reports no bands); just adopt
            // the desired values so the properties stop being dirty.
            props.eq_config.commit();
            props.eq_clear_bass.commit();
        } else {
            let wire: Vec<u8> = if bands.len() == 5 {
                std::iter::once(props.eq_clear_bass.desired + 10)
                    .chain(bands.iter().map(|&b| b + 10))
                    .map(|v| v as u8)
                    .collect()
            } else {
                bands.iter().map(|&b| (b + 6) as u8).collect()
            };
            steps.push(Step::send_commit(
                EqEbbParamEq {
                    preset_id: props.eq_preset_id.desired,
                    bands: wire,
                }
                .serialize(),
                PropCommit::EqConfig,
            ));
            steps.push(Step::send(EqEbbGetParam.serialize()));
        }
    }
    // DSEE.
    if s.contains_t1(F1::UpscalingAutoOff) && props.upscaling_enabled.dirty() {
        steps.push(Step::send_commit(
            AudioParamUpscaling {
                type_: AudioInquiredType::Upscaling,
                setting_value: if props.upscaling_enabled.desired {
                    UpscalingTypeAutoOff::Auto
                } else {
                    UpscalingTypeAutoOff::Off
                },
            }
            .serialize(),
            PropCommit::Upscaling,
        ));
    }
    // Touch functions.
    if s.contains_t1(F1::AssignableSetting)
        && (props.touch_function_left.dirty() || props.touch_function_right.dirty())
    {
        steps.push(Step::send_commit(
            SystemParamAssignableSettings {
                presets: vec![
                    props.touch_function_left.desired,
                    props.touch_function_right.desired,
                ],
            }
            .serialize(),
            PropCommit::TouchFunctions,
        ));
    }
    // Head gesture.
    if s.contains_t1(F1::HeadGestureOnOffTraining) && props.head_gesture_enabled.dirty() {
        steps.push(Step::send_commit(
            SystemParamCommon {
                type_: SystemInquiredType::HeadGestureOnOff,
                setting_value: if props.head_gesture_enabled.desired {
                    EnableDisable::Enable
                } else {
                    EnableDisable::Disable
                },
            }
            .serialize(),
            PropCommit::HeadGesture,
        ));
    }
    // Auto power off.
    if props.auto_power_off.dirty() {
        let ty = if s.contains_t1(F1::AutoPowerOff) {
            PowerInquiredType::AutoPowerOff
        } else {
            PowerInquiredType::AutoPowerOffWearingDetection
        };
        steps.push(Step::send_commit(
            PowerParamAutoPowerOff {
                type_: ty,
                current: props.auto_power_off.desired.to_u8(),
                last_select: AutoPowerOffElements::PowerOffIn5Min.to_u8(),
            }
            .serialize(),
            PropCommit::AutoPowerOff,
        ));
    }
    // Auto pause.
    if s.contains_t1(F1::PlaybackControlByWearingRemovingHeadphoneOnOff)
        && props.auto_pause_enabled.dirty()
    {
        steps.push(Step::send_commit(
            SystemParamCommon {
                type_: SystemInquiredType::PlaybackControlByWearing,
                setting_value: if props.auto_pause_enabled.desired {
                    EnableDisable::Enable
                } else {
                    EnableDisable::Disable
                },
            }
            .serialize(),
            PropCommit::AutoPause,
        ));
    }
    // Voice guidance.
    if props.voice_guidance_enabled.dirty() {
        steps.push(Step::Send {
            payload: VoiceGuidanceParamSettingMtk {
                type_: VoiceGuidanceInquiredType::MtkTransferWoDisconnectionSupportLanguageSwitch,
                setting_value: if props.voice_guidance_enabled.desired {
                    OnOffSetting::On
                } else {
                    OnOffSetting::Off
                },
            }
            .serialize(),
            data_type: DataType::DataMdrNo2,
            wait: Wait::Ack,
            commit: Some(PropCommit::VoiceGuidanceEnabled),
        });
    }
    if state
        .support
        .contains_t2(FunctionTable2::VoiceGuidanceSettingMtkTransferWithoutDisconnectionSupportLanguageSwitchAndVolumeAdjustment)
        && props.voice_guidance_volume.dirty()
    {
        steps.push(Step::Send {
            payload: VoiceGuidanceSetParamVolume {
                volume: props.voice_guidance_volume.desired,
                feedback_sound: OnOffSetting::On,
            }
            .serialize(),
            data_type: DataType::DataMdrNo2,
            wait: Wait::Ack,
            commit: Some(PropCommit::VoiceGuidanceVolume),
        });
    }
    // General settings.
    for i in 0..4 {
        let f = match i {
            0 => F1::GeneralSetting1,
            1 => F1::GeneralSetting2,
            2 => F1::GeneralSetting3,
            _ => F1::GeneralSetting4,
        };
        if s.contains_t1(f) && props.gs_param_bool[i].dirty() {
            let commit = match i {
                0 => PropCommit::Gs1,
                1 => PropCommit::Gs2,
                2 => PropCommit::Gs3,
                _ => PropCommit::Gs4,
            };
            steps.push(Step::send_commit(
                GsParamBoolean {
                    type_: gs_type(i),
                    setting_value: if props.gs_param_bool[i].desired {
                        GsSettingValue::On
                    } else {
                        GsSettingValue::Off
                    },
                }
                .serialize(),
                commit,
            ));
        }
    }

    steps.push(Step::Done(DeviceEvent::CommitOk));
    steps
}
