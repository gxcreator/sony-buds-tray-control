//! Application state machine: connects to a headphone, drives the device
//! engine, and renders a UI-agnostic menu model consumed by the tray.
//!
//! Everything in this module is testable without any GUI or Bluetooth
//! hardware: transports and device discovery are injected.

use std::sync::Arc;
use std::time::Duration;

use crate::device::{DeviceEvent, Engine};
use crate::protocol::*;
use crate::transport::{Transport, TransportKind, BLE_SERVICE_UUID_TANDEM_OVER_BLE_HPC};

pub use crate::transport::discovery::{DeviceInfo, DeviceLister, StaticDeviceLister};

/// Ambient sound selection offered by the UI.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AmbientSel {
    Nc,
    Asm,
    Off,
}

/// Status dot overlaid on the tray icon, reflecting the NC/ASM state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NcDot {
    /// Disconnected or unknown — no overlay dot.
    Hidden,
    /// Noise cancelling on — green.
    NoiseCancelling,
    /// Ambient sound on — blue.
    Ambient,
    /// NC/ASM off — grey.
    Off,
}

/// Every user action reachable from the tray menu.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UiCommand {
    RefreshDevices,
    RefreshSync,
    SetTransport(TransportKind),
    Connect { mac: String },
    Disconnect,
    Shutdown,
    SetVolume(u8),
    VolumeUp,
    VolumeDown,
    PlayPause,
    PrevTrack,
    NextTrack,
    SetAmbientMode(AmbientSel),
    CycleAmbientMode,
    SetAmbientLevel(u8),
    AmbientUp,
    AmbientDown,
    SetVoicePassthrough(bool),
    SetAutoAsm(bool),
    SetNoiseSensitivity(NoiseAdaptiveSensitivity),
    SetSpeakToChat(bool),
    SetStcSensitivity(DetectSensitivity),
    SetStcModeOut(ModeOutTime),
    SetEqPreset(EqPresetId),
    SetDsee(bool),
    SetAutoPowerOff(AutoPowerOffElements),
    SetAutoPause(bool),
    SetVoiceGuidance(bool),
    SetVoiceGuidanceVolume(i8),
    SetTouchPresetLeft(Preset),
    SetTouchPresetRight(Preset),
    SetHeadGesture(bool),
    SetBgmMode(bool),
    SetBgmRoomSize(RoomSize),
    SetCinema(bool),
    SetGeneralSetting(usize, bool),
    SetAutoConnect(bool),
    Quit,
}

/// Hard bound on the whole connect operation (transport + SDP + handshake).
pub const CONNECT_TIMEOUT: Duration = Duration::from_secs(30);

/// Delay between automatic reconnection attempts after a lost link.
pub const RECONNECT_DELAY: Duration = Duration::from_secs(5);

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConnState {
    NotConnected,
    Connecting,
    Connected,
    Error(String),
}

/// A single tray menu item (UI-agnostic).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MenuItem {
    pub label: String,
    pub kind: ItemKind,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ItemKind {
    /// Plain action; `None` renders a disabled label.
    Action(Option<UiCommand>),
    Check {
        checked: bool,
        cmd: UiCommand,
    },
    Submenu(Vec<MenuItem>),
    Radio {
        checked: bool,
        cmd: UiCommand,
    },
    Separator,
}

impl MenuItem {
    pub fn action(label: impl Into<String>) -> Self {
        Self {
            label: label.into(),
            kind: ItemKind::Action(None),
        }
    }

    pub fn cmd(label: impl Into<String>, cmd: UiCommand) -> Self {
        Self {
            label: label.into(),
            kind: ItemKind::Action(Some(cmd)),
        }
    }

    pub fn check(label: impl Into<String>, checked: bool, cmd: UiCommand) -> Self {
        Self {
            label: label.into(),
            kind: ItemKind::Check { checked, cmd },
        }
    }

    pub fn radio(label: impl Into<String>, checked: bool, cmd: UiCommand) -> Self {
        Self {
            label: label.into(),
            kind: ItemKind::Radio { checked, cmd },
        }
    }

    pub fn submenu(label: impl Into<String>, items: Vec<MenuItem>) -> Self {
        Self {
            label: label.into(),
            kind: ItemKind::Submenu(items),
        }
    }

    pub fn separator() -> Self {
        Self {
            label: String::new(),
            kind: ItemKind::Separator,
        }
    }
}

/// The UI snapshot read by the tray.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UiSnapshot {
    pub conn_state: ConnState,
    pub menu: Vec<MenuItem>,
    pub tooltip: String,
    pub icon_name: String,
    pub title: String,
    /// Status dot color for the tray icon overlay.
    pub nc_dot: NcDot,
}

/// Creates concrete transports. Injected so tests can substitute the mock.
#[async_trait::async_trait]
pub trait TransportFactory: Send + Sync {
    async fn create(&self, kind: TransportKind) -> Result<Box<dyn Transport>, String>;
}

/// Real transports: Classic RFCOMM or BLE GATT via BlueZ.
pub struct RealTransportFactory {
    session: bluer::Session,
}

impl RealTransportFactory {
    pub fn new(session: bluer::Session) -> Self {
        Self { session }
    }
}

#[async_trait::async_trait]
impl TransportFactory for RealTransportFactory {
    async fn create(&self, kind: TransportKind) -> Result<Box<dyn Transport>, String> {
        match kind {
            TransportKind::Classic => {
                Ok(Box::new(crate::transport::classic::ClassicTransport::new()))
            }
            TransportKind::Ble => {
                let t = crate::transport::gatt::GattTransport::new(
                    self.session.clone(),
                    BLE_SERVICE_UUID_TANDEM_OVER_BLE_HPC,
                );
                Ok(Box::new(t))
            }
        }
    }
}

/// The application core. Owns the connection lifecycle and the engine.
pub struct AppCore {
    pub conn_state: ConnState,
    pub devices: Vec<DeviceInfo>,
    pub transport_kind: TransportKind,
    pub last_error: Option<String>,
    engine: Option<Engine<Box<dyn Transport>>>,
    connected_mac: Option<String>,
    lister: Arc<dyn DeviceLister>,
    factory: Arc<dyn TransportFactory>,
    sync_interval: Duration,
    last_sync: std::time::Instant,
    pub menu_dirty: bool,
    refresh_pending: bool,
    config: crate::config::Config,
    auto_connect_done: bool,
    /// Auto-reconnection is armed (link lost, waiting for the device to
    /// come back).
    reconnect: bool,
    next_reconnect: std::time::Instant,
    pub reconnect_delay: Duration,
}

impl AppCore {
    pub fn new(lister: Arc<dyn DeviceLister>, factory: Arc<dyn TransportFactory>) -> Self {
        Self::new_with_config(lister, factory, crate::config::Config::load())
    }

    /// Like [`AppCore::new`] but with an explicit config (used by tests).
    pub fn new_with_config(
        lister: Arc<dyn DeviceLister>,
        factory: Arc<dyn TransportFactory>,
        config: crate::config::Config,
    ) -> Self {
        Self {
            conn_state: ConnState::NotConnected,
            devices: Vec::new(),
            transport_kind: TransportKind::Classic,
            last_error: None,
            engine: None,
            connected_mac: None,
            lister,
            factory,
            sync_interval: Duration::from_secs(120),
            last_sync: std::time::Instant::now() - Duration::from_secs(120),
            menu_dirty: true,
            refresh_pending: true,
            config,
            auto_connect_done: false,
            reconnect: false,
            next_reconnect: std::time::Instant::now(),
            reconnect_delay: RECONNECT_DELAY,
        }
    }

    pub fn is_connected(&self) -> bool {
        self.conn_state == ConnState::Connected
    }

    /// Applies a user command. Connection lifecycle operations are performed
    /// by the async `tick` loop; this method only mutates local state.
    pub fn apply_command(&mut self, cmd: UiCommand) {
        use UiCommand::*;
        match cmd {
            RefreshDevices => self.refresh_pending = true,
            RefreshSync => {
                if let Some(engine) = self.engine.as_mut() {
                    let _ = engine.request_sync();
                }
            }
            SetTransport(kind) => self.transport_kind = kind,
            Connect { mac } => {
                if self.conn_state == ConnState::Connected
                    || self.conn_state == ConnState::Connecting
                {
                    return;
                }
                self.config.last_device = Some(mac.clone());
                self.config.save();
                self.connected_mac = Some(mac);
                self.conn_state = ConnState::Connecting;
            }
            Disconnect => {
                self.teardown(None);
            }
            Shutdown => {
                if let Some(engine) = self.engine.as_mut() {
                    engine.props.shutdown.desired = true;
                }
            }
            SetVolume(v) => {
                if let Some(engine) = self.engine.as_mut() {
                    engine.props.set_volume(v as i32);
                }
            }
            VolumeUp => self.nudge_volume(1),
            VolumeDown => self.nudge_volume(-1),
            PlayPause => {
                if let Some(engine) = self.engine.as_mut() {
                    let cmd = match engine.state.play_status {
                        PlaybackStatus::Play => PlaybackControl::Pause,
                        _ => PlaybackControl::Play,
                    };
                    engine.props.play_control.desired = cmd;
                }
            }
            PrevTrack => {
                if let Some(engine) = self.engine.as_mut() {
                    engine.props.play_control.desired = PlaybackControl::TrackDown;
                }
            }
            NextTrack => {
                if let Some(engine) = self.engine.as_mut() {
                    engine.props.play_control.desired = PlaybackControl::TrackUp;
                }
            }
            SetAmbientMode(mode) => {
                if let Some(engine) = self.engine.as_mut() {
                    let props = &mut engine.props;
                    props.nc_asm_enabled.desired = mode != AmbientSel::Off;
                    props.nc_asm_mode.desired = match mode {
                        AmbientSel::Nc => NcAsmMode::Nc,
                        AmbientSel::Asm => NcAsmMode::Asm,
                        AmbientSel::Off => NcAsmMode::Nc,
                    };
                    if mode == AmbientSel::Asm && props.nc_asm_ambient_level.desired == 0 {
                        props.nc_asm_ambient_level.desired = 20;
                    }
                }
            }
            CycleAmbientMode => {
                let ambient = self
                    .engine
                    .as_ref()
                    .map(|e| self.ambient_supported(e))
                    .unwrap_or(false);
                if let Some(engine) = self.engine.as_mut() {
                    let props = &mut engine.props;
                    let (enable, mode) = cycle_ambient(
                        ambient,
                        props.nc_asm_enabled.current,
                        props.nc_asm_mode.current,
                    );
                    props.nc_asm_enabled.desired = enable;
                    props.nc_asm_mode.desired = mode;
                    if enable && mode == NcAsmMode::Asm && props.nc_asm_ambient_level.desired == 0 {
                        props.nc_asm_ambient_level.desired = 20;
                    }
                }
            }
            SetAmbientLevel(v) => {
                if let Some(engine) = self.engine.as_mut() {
                    engine.props.nc_asm_ambient_level.desired = v.clamp(1, 20);
                }
            }
            AmbientUp => self.nudge_ambient(1),
            AmbientDown => self.nudge_ambient(-1),
            SetVoicePassthrough(v) => {
                if let Some(engine) = self.engine.as_mut() {
                    engine.props.nc_asm_focus_on_voice.desired = v;
                }
            }
            SetAutoAsm(v) => {
                if let Some(engine) = self.engine.as_mut() {
                    engine.props.nc_asm_auto_asm_enabled.desired = v;
                }
            }
            SetNoiseSensitivity(v) => {
                if let Some(engine) = self.engine.as_mut() {
                    engine.props.nc_asm_noise_adaptive_sensitivity.desired = v;
                }
            }
            SetSpeakToChat(v) => {
                if let Some(engine) = self.engine.as_mut() {
                    engine.props.speak_to_chat_enabled.desired = v;
                }
            }
            SetStcSensitivity(v) => {
                if let Some(engine) = self.engine.as_mut() {
                    engine.props.speak_to_chat_detect_sensitivity.desired = v;
                }
            }
            SetStcModeOut(v) => {
                if let Some(engine) = self.engine.as_mut() {
                    engine.props.speak_to_mode_out_time.desired = v;
                }
            }
            SetEqPreset(v) => {
                if let Some(engine) = self.engine.as_mut() {
                    engine.props.eq_preset_id.desired = v;
                }
            }
            SetDsee(v) => {
                if let Some(engine) = self.engine.as_mut() {
                    engine.props.upscaling_enabled.desired = v;
                }
            }
            SetAutoPowerOff(v) => {
                if let Some(engine) = self.engine.as_mut() {
                    engine.props.auto_power_off.desired = v;
                }
            }
            SetAutoPause(v) => {
                if let Some(engine) = self.engine.as_mut() {
                    engine.props.auto_pause_enabled.desired = v;
                }
            }
            SetVoiceGuidance(v) => {
                if let Some(engine) = self.engine.as_mut() {
                    engine.props.voice_guidance_enabled.desired = v;
                }
            }
            SetVoiceGuidanceVolume(v) => {
                if let Some(engine) = self.engine.as_mut() {
                    engine.props.voice_guidance_volume.desired = v.clamp(-2, 2);
                }
            }
            SetTouchPresetLeft(v) => {
                if let Some(engine) = self.engine.as_mut() {
                    engine.props.touch_function_left.desired = v;
                }
            }
            SetTouchPresetRight(v) => {
                if let Some(engine) = self.engine.as_mut() {
                    engine.props.touch_function_right.desired = v;
                }
            }
            SetHeadGesture(v) => {
                if let Some(engine) = self.engine.as_mut() {
                    engine.props.head_gesture_enabled.desired = v;
                }
            }
            SetBgmMode(v) => {
                if let Some(engine) = self.engine.as_mut() {
                    engine.props.bgm_mode_enabled.desired = v;
                }
            }
            SetBgmRoomSize(v) => {
                if let Some(engine) = self.engine.as_mut() {
                    engine.props.bgm_mode_room_size.desired = v;
                }
            }
            SetCinema(v) => {
                if let Some(engine) = self.engine.as_mut() {
                    engine.props.upmix_cinema_enabled.desired = v;
                }
            }
            SetGeneralSetting(idx, v) => {
                if let Some(engine) = self.engine.as_mut() {
                    if let Some(p) = engine.props.gs_param_bool.get_mut(idx) {
                        p.desired = v;
                    }
                }
            }
            SetAutoConnect(v) => {
                self.config.auto_connect = v;
                if !v {
                    self.reconnect = false;
                }
                self.config.save();
            }
            Quit => {}
        }
        self.menu_dirty = true;
    }

    fn nudge_volume(&mut self, delta: i32) {
        if let Some(engine) = self.engine.as_mut() {
            engine.props.nudge_volume(delta);
        }
    }

    fn nudge_ambient(&mut self, delta: i32) {
        if let Some(engine) = self.engine.as_mut() {
            engine.props.nudge_ambient_level(delta);
        }
    }

    /// Refreshes the cached device list through the async lister.
    async fn refresh_devices_async(&mut self) {
        match self.lister.list_devices().await {
            Ok(list) => {
                self.devices = list;
                self.menu_dirty = true;
            }
            Err(e) => {
                self.last_error = Some(format!("Device scan failed: {e}"));
                self.menu_dirty = true;
            }
        }
    }

    /// The async tick: connects, polls the engine, commits dirty properties
    /// and performs periodic syncs.
    pub async fn tick(&mut self) {
        if self.refresh_pending {
            self.refresh_pending = false;
            self.refresh_devices_async().await;
        }

        // Auto-connect to the last device once, at startup.
        if !self.auto_connect_done {
            self.auto_connect_done = true;
            if self.config.auto_connect
                && self.conn_state == ConnState::NotConnected
                && self.connected_mac.is_none()
            {
                if let Some(mac) = self.config.last_device.clone() {
                    log::info!("auto-connecting to last device {mac}");
                    self.connected_mac = Some(mac);
                    self.conn_state = ConnState::Connecting;
                    self.menu_dirty = true;
                }
            }
        }

        // Drive the connection process.
        match &self.conn_state {
            ConnState::Connecting => {
                let mac = self.connected_mac.clone().unwrap_or_default();
                log::info!("connecting to {mac} via {}", self.transport_kind.as_str());
                let created = self.factory.create(self.transport_kind).await;
                match created {
                    Ok(mut transport) => {
                        // Bound the whole connect so a hung transport can
                        // never leave the UI stuck in "Connecting…".
                        match tokio::time::timeout(CONNECT_TIMEOUT, transport.connect(&mac)).await {
                            Ok(Ok(())) => {
                                log::info!("transport connected to {mac}");
                                let mut engine = Engine::new(transport);
                                if let Err(e) = engine.request_init() {
                                    log::error!("init scheduling failed: {e}");
                                    self.conn_state = ConnState::Error(e.to_string());
                                } else {
                                    self.engine = Some(engine);
                                    self.conn_state = ConnState::Connected;
                                    self.reconnect = false;
                                    log::info!("init handshake started");
                                }
                            }
                            Ok(Err(e)) => {
                                log::error!("connect to {mac} failed: {e}");
                                self.conn_state = ConnState::Error(format!("Connect failed: {e}"));
                                self.last_error = Some(self.conn_state_str());
                                self.arm_reconnect();
                            }
                            Err(_) => {
                                log::error!("connect to {mac} timed out after {CONNECT_TIMEOUT:?}");
                                self.conn_state = ConnState::Error(format!(
                                    "Connect timed out after {CONNECT_TIMEOUT:?}"
                                ));
                                self.last_error = Some(self.conn_state_str());
                                self.arm_reconnect();
                            }
                        }
                    }
                    Err(e) => {
                        log::error!("transport factory error: {e}");
                        self.conn_state = ConnState::Error(e);
                        self.last_error = Some(self.conn_state_str());
                        self.arm_reconnect();
                    }
                }
                self.menu_dirty = true;
            }
            ConnState::Connected => {
                let mut fatal: Option<String> = None;
                {
                    let engine = self.engine.as_mut().expect("connected");
                    // Poll the device.
                    let events = poll_engine(engine, Duration::from_millis(50)).await;
                    for ev in events {
                        self.menu_dirty = true;
                        match ev {
                            DeviceEvent::InitOk => {
                                log::info!("device initialized");
                                let _ = engine.request_sync();
                                self.last_sync = std::time::Instant::now();
                            }
                            DeviceEvent::SyncOk => {
                                log::debug!("device sync complete");
                            }
                            DeviceEvent::CommitOk => {
                                log::debug!("device commit complete");
                            }
                            DeviceEvent::Error(e) => {
                                log::error!("device error: {e}");
                                fatal = Some(format!("Device error: {e}"));
                                break;
                            }
                            DeviceEvent::Disconnected => {
                                log::warn!("device disconnected");
                                fatal = Some("Connection lost".to_string());
                                break;
                            }
                            _ => {}
                        }
                    }
                    // Auto-commit dirty properties when idle.
                    if fatal.is_none() && engine.is_ready() && engine.props.is_dirty() {
                        let _ = engine.request_commit();
                    }
                    // Periodic sync for battery etc.
                    if fatal.is_none()
                        && engine.is_ready()
                        && self.last_sync.elapsed() >= self.sync_interval
                    {
                        let _ = engine.request_sync();
                        self.last_sync = std::time::Instant::now();
                    }
                }
                if let Some(msg) = fatal {
                    self.teardown(Some(msg));
                }
            }
            ConnState::NotConnected | ConnState::Error(_) => {
                // Passive auto-reconnect: never attempt connections while the
                // device is away (that spams failing SDP/RFCOMM connects).
                // Instead, rescan BlueZ periodically and attach only once the
                // headphone is back and connected (e.g. via the system UI).
                if self.reconnect && std::time::Instant::now() >= self.next_reconnect {
                    self.next_reconnect = std::time::Instant::now() + self.reconnect_delay;
                    if let Some(mac) = self.config.last_device.clone() {
                        self.refresh_devices_async().await;
                        let back = self.devices.iter().any(|d| d.mac == mac && d.connected);
                        if back {
                            log::info!("device {mac} is back, attaching");
                            self.connected_mac = Some(mac);
                            self.conn_state = ConnState::Connecting;
                            self.menu_dirty = true;
                        }
                    }
                }
            }
        }
    }

    fn conn_state_str(&self) -> String {
        match &self.conn_state {
            ConnState::Error(e) => e.clone(),
            ConnState::NotConnected => "Not connected".to_string(),
            ConnState::Connecting => "Connecting…".to_string(),
            ConnState::Connected => "Connected".to_string(),
        }
    }

    fn teardown(&mut self, error: Option<String>) {
        if let Some(e) = &error {
            log::warn!("connection torn down: {e}");
        }
        self.engine = None;
        self.connected_mac = None;
        let lost = error.is_some();
        self.conn_state = match error {
            Some(e) => ConnState::Error(e),
            None => ConnState::NotConnected,
        };
        // A lost link re-arms automatic reconnection; a manual disconnect
        // (or cancel) stops it.
        if lost {
            self.arm_reconnect();
        } else {
            self.reconnect = false;
        }
        self.menu_dirty = true;
    }

    /// Arms passive reconnection if the user opted in: the app waits for the
    /// headphone to come back (seen connected via BlueZ) instead of actively
    /// retrying connections.
    fn arm_reconnect(&mut self) {
        if self.config.auto_connect && self.config.last_device.is_some() {
            self.reconnect = true;
            self.next_reconnect = std::time::Instant::now() + self.reconnect_delay;
            self.menu_dirty = true;
        }
    }

    // ------------------------------------------------------------------
    // UI snapshot
    // ------------------------------------------------------------------

    pub fn snapshot(&self) -> UiSnapshot {
        let conn_state = self.conn_state.clone();
        let menu = self.build_menu();
        let (title, tooltip, icon_name) = self.tray_info();
        let nc_dot = match &self.conn_state {
            ConnState::Connected => {
                let props = &self.engine.as_ref().expect("connected").props;
                if !props.nc_asm_enabled.current {
                    NcDot::Off
                } else {
                    match props.nc_asm_mode.current {
                        NcAsmMode::Asm => NcDot::Ambient,
                        _ => NcDot::NoiseCancelling,
                    }
                }
            }
            _ => NcDot::Hidden,
        };
        UiSnapshot {
            conn_state,
            menu,
            tooltip,
            icon_name,
            title,
            nc_dot,
        }
    }

    fn tray_info(&self) -> (String, String, String) {
        let title = "Sony Buds Control".to_string();
        let icon = "sony-buds-disconnected".to_string();
        let tooltip = match &self.conn_state {
            ConnState::NotConnected => "Sony Buds — not connected".to_string(),
            ConnState::Connecting => "Sony Buds — connecting…".to_string(),
            ConnState::Error(e) => format!("Sony Buds — {e}"),
            ConnState::Connected => {
                let engine = self.engine.as_ref().expect("connected");
                let s = &engine.state;
                let mut lines = vec![format!("{}", model_name(s))];
                lines.push(battery_line(s));
                if !s.play_title.is_empty() {
                    let artist = if s.play_artist.is_empty() {
                        ""
                    } else {
                        &s.play_artist
                    };
                    lines.push(format!(
                        "♪ {}{}",
                        s.play_title,
                        if artist.is_empty() {
                            String::new()
                        } else {
                            format!(" — {artist}")
                        }
                    ));
                }
                lines.join("\n")
            }
        };
        (title, tooltip, icon)
    }

    fn build_menu(&self) -> Vec<MenuItem> {
        let mut items = Vec::new();
        match &self.conn_state {
            ConnState::NotConnected | ConnState::Error(_) => {
                let header = match &self.conn_state {
                    ConnState::Error(e) if !self.reconnect => format!("⚠️ {e}"),
                    _ if self.reconnect => "🔄 Waiting for device…".to_string(),
                    _ => "🔌 Not connected".to_string(),
                };
                items.push(MenuItem::action(header));
                items.push(MenuItem::separator());
                items.push(self.build_connect_menu());
                if self.reconnect {
                    items.push(MenuItem::cmd("✖ Cancel auto-reconnect", UiCommand::Disconnect));
                }
            }
            ConnState::Connecting => {
                items.push(MenuItem::action("⏳ Connecting…"));
                items.push(MenuItem::cmd("✖ Cancel", UiCommand::Disconnect));
            }
            ConnState::Connected => {
                let engine = self.engine.as_ref().expect("connected");
                let s = &engine.state;
                let p = &engine.props;
                items.push(MenuItem::action(format!(
                    "🎧 {}{}",
                    model_name(s),
                    if s.audio_codec == crate::protocol::AudioCodec::Unsettled {
                        String::new()
                    } else {
                        format!(" · {}", s.audio_codec)
                    }
                )));
                items.push(MenuItem::action(format!("🔋 {}", battery_line(s))));
                if !s.play_title.is_empty() {
                    items.push(MenuItem::action(now_playing_line(s)));
                }
                items.push(MenuItem::separator());

                // Playback: transport controls + volume.
                let play_label = match s.play_status {
                    PlaybackStatus::Play => "⏸️Pause",
                    _ => "▶️Play",
                };
                // Ambient sound.
                if self.ambient_supported(engine) {
                    items.push(MenuItem::submenu(
                        "🍃 ANC",
                        self.build_ambient_menu(engine),
                    ));
                }
                items.push(MenuItem::submenu(
                    "⏯️ Playback",
                    vec![
                        MenuItem::cmd("⏮ Prev", UiCommand::PrevTrack),
                        MenuItem::cmd(play_label, UiCommand::PlayPause),
                        MenuItem::cmd("⏭ Next", UiCommand::NextTrack),
                        MenuItem::separator(),
                        MenuItem::cmd("−5", UiCommand::VolumeDown),
                        MenuItem::action(format!("Volume: {} / 30", p.play_volume.current)),
                        MenuItem::cmd("+5", UiCommand::VolumeUp),
                    ],
                ));

                // Speak to Chat.
                if self.stc_supported(engine) {
                    items.push(MenuItem::submenu(
                        "💬 Speak to Chat",
                        self.build_stc_menu(engine),
                    ));
                }
                // EQ & DSEE.
                items.push(MenuItem::submenu("🎚️ Equalizer", self.build_eq_menu(engine)));
                // System settings.
                items.push(MenuItem::submenu(
                    "⚙️ Settings",
                    self.build_settings_menu(engine),
                ));

                items.push(MenuItem::separator());
                items.push(MenuItem::submenu(
                    "ℹ️ About",
                    vec![
                        MenuItem::action(format!("Model: {}", model_name(s))),
                        MenuItem::action(format!("Firmware: {}", s.fw_version)),
                        MenuItem::action(format!("Codec: {}", s.audio_codec)),
                        if s.unique_id.is_empty() {
                            MenuItem::action("MAC: —")
                        } else {
                            MenuItem::action(format!("MAC: {}", s.unique_id))
                        },
                    ],
                ));
                items.push(MenuItem::cmd("🔄 Refresh", UiCommand::RefreshSync));
                items.push(MenuItem::separator());
                items.push(MenuItem::cmd("🔌 Disconnect", UiCommand::Disconnect));
                if engine.state.support.contains_t1(FunctionTable1::PowerOff) {
                    items.push(MenuItem::cmd("⏻ Shutdown headphones", UiCommand::Shutdown));
                }
            }
        }
        items.push(MenuItem::cmd("🚪 Quit", UiCommand::Quit));
        items
    }

    fn build_connect_menu(&self) -> MenuItem {
        let mut items = vec![
            MenuItem::cmd("Scan for devices", UiCommand::RefreshDevices),
            MenuItem::separator(),
            MenuItem::check(
                "Connection: Classic Bluetooth",
                self.transport_kind == TransportKind::Classic,
                UiCommand::SetTransport(TransportKind::Classic),
            ),
            MenuItem::check(
                "Connection: BLE (GATT)",
                self.transport_kind == TransportKind::Ble,
                UiCommand::SetTransport(TransportKind::Ble),
            ),
            MenuItem::check(
                "Auto-connect at startup",
                self.config.auto_connect,
                UiCommand::SetAutoConnect(!self.config.auto_connect),
            ),
            MenuItem::separator(),
        ];
        if self.devices.is_empty() {
            items.push(MenuItem::action("No devices found"));
        } else {
            for d in &self.devices {
                let label = if d.name.is_empty() {
                    d.mac.to_string()
                } else {
                    format!("{} ({})", d.name, d.mac)
                };
                items.push(MenuItem::radio(
                    label,
                    false,
                    UiCommand::Connect { mac: d.mac.clone() },
                ));
            }
        }
        MenuItem::submenu("🔌 Connect", items)
    }

    fn ambient_supported(&self, engine: &Engine<Box<dyn Transport>>) -> bool {
        use FunctionTable1 as F1;
        let s = &engine.state.support;
        s.contains_t1(F1::NoiseCancellingOnOff)
            || s.contains_t1(F1::NoiseCancellingOnOffAndAmbientSoundModeOnOff)
            || s.contains_t1(F1::AmbientSoundModeOnOff)
            || s.contains_t1(F1::AmbientSoundModeLevelAdjustment)
            || s.contains_t1(F1::ModeNcAsmNoiseCancellingDualAmbientSoundModeLevelAdjustment)
            || s.contains_t1(
                F1::ModeNcAsmNoiseCancellingDualAmbientSoundModeLevelAdjustmentNoiseAdaptation,
            )
            || s.contains_t1(F1::ModeNcAsmNoiseCancellingDualSingleAmbientSoundModeLevelAdjustment)
    }

    fn build_ambient_menu(&self, engine: &Engine<Box<dyn Transport>>) -> Vec<MenuItem> {
        let p = &engine.props;
        let mode = if !p.nc_asm_enabled.current {
            AmbientSel::Off
        } else if p.nc_asm_mode.current == NcAsmMode::Nc {
            AmbientSel::Nc
        } else {
            AmbientSel::Asm
        };
        let mut items = vec![
            MenuItem::radio(
                "Noise Cancelling",
                mode == AmbientSel::Nc,
                UiCommand::SetAmbientMode(AmbientSel::Nc),
            ),
            MenuItem::radio(
                "Ambient Sound",
                mode == AmbientSel::Asm,
                UiCommand::SetAmbientMode(AmbientSel::Asm),
            ),
            MenuItem::radio(
                "Off",
                mode == AmbientSel::Off,
                UiCommand::SetAmbientMode(AmbientSel::Off),
            ),
            MenuItem::separator(),
            MenuItem::submenu(
                "Ambient volume",
                (1..=20)
                    .map(|v| {
                        MenuItem::radio(
                            format!("{v}"),
                            p.nc_asm_ambient_level.current == v,
                            UiCommand::SetAmbientLevel(v),
                        )
                    })
                    .collect(),
            ),
            MenuItem::check(
                "Voice passthrough",
                p.nc_asm_focus_on_voice.current,
                UiCommand::SetVoicePassthrough(!p.nc_asm_focus_on_voice.current),
            ),
        ];
        if engine.state.support.contains_t1(
            FunctionTable1::ModeNcAsmNoiseCancellingDualAmbientSoundModeLevelAdjustmentNoiseAdaptation,
        ) {
            items.push(MenuItem::separator());
            items.push(MenuItem::check(
                "Auto ambient sound",
                p.nc_asm_auto_asm_enabled.current,
                UiCommand::SetAutoAsm(!p.nc_asm_auto_asm_enabled.current),
            ));
            items.push(MenuItem::radio(
                "Sensitivity: Standard",
                p.nc_asm_noise_adaptive_sensitivity.current == NoiseAdaptiveSensitivity::Standard,
                UiCommand::SetNoiseSensitivity(NoiseAdaptiveSensitivity::Standard),
            ));
            items.push(MenuItem::radio(
                "Sensitivity: High",
                p.nc_asm_noise_adaptive_sensitivity.current == NoiseAdaptiveSensitivity::High,
                UiCommand::SetNoiseSensitivity(NoiseAdaptiveSensitivity::High),
            ));
            items.push(MenuItem::radio(
                "Sensitivity: Low",
                p.nc_asm_noise_adaptive_sensitivity.current == NoiseAdaptiveSensitivity::Low,
                UiCommand::SetNoiseSensitivity(NoiseAdaptiveSensitivity::Low),
            ));
        }
        items
    }

    fn stc_supported(&self, engine: &Engine<Box<dyn Transport>>) -> bool {
        engine
            .state
            .support
            .contains_t1(FunctionTable1::SmartTalkingModeType2)
    }

    fn build_stc_menu(&self, engine: &Engine<Box<dyn Transport>>) -> Vec<MenuItem> {
        let p = &engine.props;
        let mut items = vec![
            MenuItem::check(
                "Enabled",
                p.speak_to_chat_enabled.current,
                UiCommand::SetSpeakToChat(!p.speak_to_chat_enabled.current),
            ),
            MenuItem::separator(),
        ];
        for (label, v) in [
            ("Sensitivity: Auto", DetectSensitivity::Auto),
            ("Sensitivity: High", DetectSensitivity::High),
            ("Sensitivity: Low", DetectSensitivity::Low),
        ] {
            items.push(MenuItem::radio(
                label,
                p.speak_to_chat_detect_sensitivity.current == v,
                UiCommand::SetStcSensitivity(v),
            ));
        }
        items.push(MenuItem::separator());
        for (label, v) in [
            ("Duration: Short (~5s)", ModeOutTime::Fast),
            ("Duration: Standard (~15s)", ModeOutTime::Mid),
            ("Duration: Long (~30s)", ModeOutTime::Slow),
            ("Duration: Don't end automatically", ModeOutTime::None),
        ] {
            items.push(MenuItem::radio(
                label,
                p.speak_to_mode_out_time.current == v,
                UiCommand::SetStcModeOut(v),
            ));
        }
        items
    }

    fn build_eq_menu(&self, engine: &Engine<Box<dyn Transport>>) -> Vec<MenuItem> {
        let p = &engine.props;
        let mut items = Vec::new();
        if p.eq_available.current || p.eq_preset_id.current != EqPresetId::Off {
            for preset in EqPresetId::ALL {
                items.push(MenuItem::radio(
                    preset.to_string(),
                    p.eq_preset_id.current == preset,
                    UiCommand::SetEqPreset(preset),
                ));
            }
        } else {
            items.push(MenuItem::action("EQ unavailable on this device"));
        }
        if engine
            .state
            .support
            .contains_t1(FunctionTable1::UpscalingAutoOff)
        {
            items.push(MenuItem::separator());
            items.push(MenuItem::check(
                "DSEE upscaling",
                p.upscaling_enabled.current,
                UiCommand::SetDsee(!p.upscaling_enabled.current),
            ));
        }
        items
    }

    fn build_settings_menu(&self, engine: &Engine<Box<dyn Transport>>) -> Vec<MenuItem> {
        let p = &engine.props;
        let s = &engine.state;
        let mut items = Vec::new();

        // Speak to Chat toggle is in its own submenu; keep settings focused on
        // the remaining options.

        // Listening mode (BGM / cinema).
        if s.support.contains_t1(FunctionTable1::ListeningOption) {
            let mut listening = vec![MenuItem::check(
                "BGM mode",
                p.bgm_mode_enabled.current,
                UiCommand::SetBgmMode(!p.bgm_mode_enabled.current),
            )];
            listening.push(MenuItem::radio(
                "Room: My Room",
                p.bgm_mode_room_size.current == RoomSize::Small,
                UiCommand::SetBgmRoomSize(RoomSize::Small),
            ));
            listening.push(MenuItem::radio(
                "Room: Living Room",
                p.bgm_mode_room_size.current == RoomSize::Middle,
                UiCommand::SetBgmRoomSize(RoomSize::Middle),
            ));
            listening.push(MenuItem::radio(
                "Room: Cafe",
                p.bgm_mode_room_size.current == RoomSize::Large,
                UiCommand::SetBgmRoomSize(RoomSize::Large),
            ));
            listening.push(MenuItem::check(
                "Cinema (upmix)",
                p.upmix_cinema_enabled.current,
                UiCommand::SetCinema(!p.upmix_cinema_enabled.current),
            ));
            items.push(MenuItem::submenu("Listening Mode", listening));
        }

        // Auto power off.
        if s.support.contains_t1(FunctionTable1::AutoPowerOff)
            || s.support
                .contains_t1(FunctionTable1::AutoPowerOffWithWearingDetection)
        {
            let mut auto_off = Vec::new();
            for v in AutoPowerOffElements::ALL {
                auto_off.push(MenuItem::radio(
                    format!("Off after {v}"),
                    p.auto_power_off.current == v,
                    UiCommand::SetAutoPowerOff(v),
                ));
            }
            items.push(MenuItem::submenu("Auto Power Off", auto_off));
        }

        if s.support
            .contains_t1(FunctionTable1::PlaybackControlByWearingRemovingHeadphoneOnOff)
        {
            items.push(MenuItem::check(
                "Pause when removed",
                p.auto_pause_enabled.current,
                UiCommand::SetAutoPause(!p.auto_pause_enabled.current),
            ));
        }
        if s.support
            .contains_t1(FunctionTable1::HeadGestureOnOffTraining)
        {
            items.push(MenuItem::check(
                "Head gesture",
                p.head_gesture_enabled.current,
                UiCommand::SetHeadGesture(!p.head_gesture_enabled.current),
            ));
        }
        if s.has_table2 {
            let has_volume = s.support.contains_t2(
                FunctionTable2::VoiceGuidanceSettingMtkTransferWithoutDisconnectionSupportLanguageSwitchAndVolumeAdjustment,
            );
            let mut vg = vec![MenuItem::check(
                "Enabled",
                p.voice_guidance_enabled.current,
                UiCommand::SetVoiceGuidance(!p.voice_guidance_enabled.current),
            )];
            if has_volume {
                vg.push(MenuItem::separator());
                for v in -2i8..=2 {
                    vg.push(MenuItem::radio(
                        format!("Volume: {v:+}"),
                        p.voice_guidance_volume.current == v,
                        UiCommand::SetVoiceGuidanceVolume(v),
                    ));
                }
            }
            items.push(MenuItem::submenu("Voice Guidance", vg));
        }
        if s.support.contains_t1(FunctionTable1::AssignableSetting) {
            items.push(MenuItem::submenu(
                "Touch presets",
                self.build_touch_menu(engine),
            ));
        }
        // General settings.
        if !s.gs_capabilities.is_empty() {
            for (i, cap) in s.gs_capabilities.iter().enumerate() {
                let subject = gs_subject(&cap.subject);
                let checked = p.gs_param_bool.get(i).map(|x| x.current).unwrap_or(false);
                items.push(MenuItem::check(
                    subject,
                    checked,
                    UiCommand::SetGeneralSetting(i, !checked),
                ));
            }
        }
        items
    }

    fn build_touch_menu(&self, engine: &Engine<Box<dyn Transport>>) -> Vec<MenuItem> {
        let p = &engine.props;
        let mut items = Vec::new();
        for (label, v) in [
            ("Playback Control", Preset::PlaybackControl),
            (
                "Ambient Sound Control",
                Preset::AmbientSoundControlQuickAccess,
            ),
            ("No Function", Preset::NoFunction),
        ] {
            items.push(MenuItem::radio(
                format!("Left: {label}"),
                p.touch_function_left.current == v,
                UiCommand::SetTouchPresetLeft(v),
            ));
        }
        items.push(MenuItem::separator());
        for (label, v) in [
            ("Playback Control", Preset::PlaybackControl),
            (
                "Ambient Sound Control",
                Preset::AmbientSoundControlQuickAccess,
            ),
            ("No Function", Preset::NoFunction),
        ] {
            items.push(MenuItem::radio(
                format!("Right: {label}"),
                p.touch_function_right.current == v,
                UiCommand::SetTouchPresetRight(v),
            ));
        }
        items
    }
}

/// Cycles the NC/ambient mode one step forward: NC → Ambient Sound → Off →
/// NC. Devices without ambient sound support just toggle NC on/off.
fn cycle_ambient(
    ambient_supported: bool,
    enabled: bool,
    mode: NcAsmMode,
) -> (bool, NcAsmMode) {
    if ambient_supported {
        match (enabled, mode) {
            (true, NcAsmMode::Nc) => (true, NcAsmMode::Asm),
            (true, _) => (false, NcAsmMode::Nc),
            (false, _) => (true, NcAsmMode::Nc),
        }
    } else if enabled {
        (false, NcAsmMode::Nc)
    } else {
        (true, NcAsmMode::Nc)
    }
}

fn model_name(s: &crate::device::DeviceState) -> &str {
    if s.model_name.is_empty() {
        "Headphones"
    } else {
        &s.model_name
    }
}

fn battery_line(s: &crate::device::DeviceState) -> String {
    let l = &s.battery_left;
    let r = &s.battery_right;
    let c = &s.battery_case;
    let charge = |b: &crate::device::BatteryState| match b.charging {
        BatteryChargingStatus::Charging => " (charging)",
        BatteryChargingStatus::Charged => " (charged)",
        _ => "",
    };
    if r.is_reported() && l.is_reported() {
        let extra = if c.is_reported() {
            format!(" · Case: {}%{}", c.level, charge(c))
        } else {
            String::new()
        };
        format!(
            "Battery: L {}%{} R {}%{}{}",
            l.level,
            charge(l),
            r.level,
            charge(r),
            extra
        )
    } else if l.is_reported() {
        format!("Battery: {}%{}", l.level, charge(l))
    } else {
        "Battery: —".to_string()
    }
}

fn now_playing_line(s: &crate::device::DeviceState) -> String {
    match (s.play_title.as_str(), s.play_artist.as_str()) {
        (title, artist) if !title.is_empty() && !artist.is_empty() => {
            format!("🎵 {title} — {artist}")
        }
        (title, _) if !title.is_empty() => format!("🎵 {title}"),
        _ => "🎵".to_string(),
    }
}

fn gs_subject(raw: &str) -> String {
    match raw {
        "MULTIPOINT_SETTING" => "Connect to 2 devices simultaneously".to_string(),
        "SIDETONE_SETTING" => "Capture Voice During a Phone Call".to_string(),
        "TOUCH_PANEL_SETTING" => "Touch sensor control panel".to_string(),
        "" => "General setting".to_string(),
        other => other.to_string(),
    }
}

/// Polls the engine, collecting every event produced within the timeout.
async fn poll_engine(
    engine: &mut Engine<Box<dyn Transport>>,
    timeout: Duration,
) -> Vec<DeviceEvent> {
    let mut events = Vec::new();
    for _ in 0..64 {
        match engine.poll(timeout).await {
            Some(ev) => events.push(ev),
            None => break,
        }
        if events.len() >= 8 {
            break;
        }
    }
    events
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::transport::MockTransport;
    use std::sync::Mutex;

    struct MockFactory(Mutex<Vec<MockTransport>>);

    #[async_trait::async_trait]
    impl TransportFactory for MockFactory {
        async fn create(&self, _kind: TransportKind) -> Result<Box<dyn Transport>, String> {
            let (host, device) = MockTransport::pair();
            let mut devices = self.0.lock().unwrap();
            devices.push(device);
            Ok(Box::new(host))
        }
    }

    #[test]
    fn disconnected_menu_offers_connect_and_quit() {
        let lister: Arc<dyn DeviceLister> = Arc::new(StaticDeviceLister(vec![]));
        let factory: Arc<dyn TransportFactory> = Arc::new(MockFactory(Mutex::new(Vec::new())));
        let app = AppCore::new_with_config(lister, factory, crate::config::Config::default());
        let snap = app.snapshot();
        let labels: Vec<String> = flatten(&snap.menu).into_iter().map(|m| m.label).collect();
        assert!(labels.iter().any(|l| l.contains("Connect")));
        assert!(labels.iter().any(|l| l.contains("Quit")));
        assert!(labels.iter().any(|l| l == "No devices found"));
    }

    #[test]
    fn devices_appear_in_connect_menu() {
        let lister: Arc<dyn DeviceLister> = Arc::new(StaticDeviceLister(vec![]));
        let factory: Arc<dyn TransportFactory> = Arc::new(MockFactory(Mutex::new(Vec::new())));
        let mut app = AppCore::new_with_config(lister, factory, crate::config::Config::default());
        app.devices = vec![DeviceInfo {
            name: "WH-1000XM5".into(),
            mac: "AA:BB:CC:DD:EE:FF".into(),
            paired: true,
            connected: false,
        }];
        let snap = app.snapshot();
        let labels: Vec<String> = flatten(&snap.menu).into_iter().map(|m| m.label).collect();
        assert!(labels.iter().any(|l| l.contains("WH-1000XM5")));
    }

    #[test]
    fn cycle_ambient_cycles_nc_asm_off() {
        use NcAsmMode::*;
        assert_eq!(cycle_ambient(true, true, Nc), (true, Asm));
        assert_eq!(cycle_ambient(true, true, Asm), (false, Nc));
        assert_eq!(cycle_ambient(true, false, Nc), (true, Nc));
        assert_eq!(cycle_ambient(true, false, Asm), (true, Nc));
        assert_eq!(cycle_ambient(false, true, Nc), (false, Nc));
        assert_eq!(cycle_ambient(false, false, Nc), (true, Nc));
    }

    fn flatten(items: &[MenuItem]) -> Vec<MenuItem> {
        let mut out = Vec::new();
        for i in items {
            out.push(i.clone());
            if let ItemKind::Submenu(children) = &i.kind {
                out.extend(flatten(children));
            }
        }
        out
    }
}
