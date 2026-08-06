//! Shared test helpers: a simulated Sony headphone speaking the MDR V2
//! protocol over an in-memory pipe, mirroring the reference client's device
//! behaviour byte-for-byte.

#![allow(dead_code)]

use sony_buds_tray_control::protocol::*;
use sony_buds_tray_control::transport::MockTransport;
use sony_buds_tray_control::transport::Transport;

pub struct DeviceSimState {
    pub volume: u8,
    pub battery: u8,
    pub charging: BatteryChargingStatus,
    pub nc_asm_enabled: bool,
    pub nc_asm_mode: NcAsmMode,
    pub ambient_level: u8,
    pub focus_on_voice: bool,
    pub auto_asm: bool,
    pub sensitivity: NoiseAdaptiveSensitivity,
    pub nc_amb_function: Function,
    pub play_status: PlaybackStatus,
    pub title: String,
    pub album: String,
    pub artist: String,
    pub eq_preset: EqPresetId,
    pub eq_bands: Vec<u8>,
    pub dsee: bool,
    pub stc_enabled: bool,
    pub stc_sensitivity: DetectSensitivity,
    pub stc_mode_out: ModeOutTime,
    pub auto_power_off: u8,
    pub auto_pause: bool,
    pub voice_guidance: bool,
    pub voice_guidance_volume: i8,
    pub touch_left: Preset,
    pub touch_right: Preset,
    pub head_gesture: bool,
    pub gs_values: [bool; 4],
    pub bgm_enabled: bool,
    pub bgm_room: RoomSize,
    pub cinema: bool,
    /// Multipoint devices: (address, name, connected).
    pub multipoint_devices: Vec<(String, String, bool)>,
    /// Index into `multipoint_devices` of the playback device.
    pub multipoint_playback: u8,
}

impl Default for DeviceSimState {
    fn default() -> Self {
        Self {
            volume: 12,
            battery: 87,
            charging: BatteryChargingStatus::NotCharging,
            nc_asm_enabled: true,
            nc_asm_mode: NcAsmMode::Nc,
            ambient_level: 12,
            focus_on_voice: false,
            auto_asm: false,
            sensitivity: NoiseAdaptiveSensitivity::Standard,
            nc_amb_function: Function::NcAsm,
            play_status: PlaybackStatus::Play,
            title: "Test Song".into(),
            album: "Test Album".into(),
            artist: "Test Artist".into(),
            eq_preset: EqPresetId::Off,
            eq_bands: vec![0; 10],
            dsee: false,
            stc_enabled: false,
            stc_sensitivity: DetectSensitivity::Auto,
            stc_mode_out: ModeOutTime::Mid,
            auto_power_off: 0x11,
            auto_pause: false,
            voice_guidance: true,
            voice_guidance_volume: 0,
            touch_left: Preset::PlaybackControl,
            touch_right: Preset::PlaybackControl,
            head_gesture: false,
            gs_values: [false; 4],
            bgm_enabled: false,
            bgm_room: RoomSize::Small,
            cinema: false,
            multipoint_devices: vec![
                ("AA:11:22:33:44:55".into(), "My Phone".into(), true),
                ("BB:11:22:33:44:55".into(), "Laptop".into(), false),
            ],
            multipoint_playback: 0,
        }
    }
}

/// Overrides the command byte of a serialized response (the shared payload
/// structs default to the SET command; reports must use the RET command).
fn ret(mut v: Vec<u8>, cmd: u8) -> Vec<u8> {
    v[0] = cmd;
    v
}

/// Which support functions the simulated device advertises.
pub struct DeviceProfile {
    pub has_table2: bool,
    pub nc_asm_type: NcAsmInquiredType,
    pub support_t1: Vec<u8>,
    pub support_t2: Vec<u8>,
}

impl DeviceProfile {
    /// A WH-1000XM5-like device: classic ambient control, table 2, most
    /// common features.
    pub fn xm5() -> Self {
        use FunctionTable1 as F1;
        let t1 = vec![
            F1::CodecIndicator,
            F1::BatteryLevelIndicator,
            F1::LeftRightBatteryLevelIndicator,
            F1::PowerOff,
            F1::AutoPowerOff,
            F1::FixedMessage,
            F1::PresetEq,
            F1::CustomEq,
            F1::NoiseCancellingOnOffAndAmbientSoundModeLevelAdjustment,
            F1::AmbientSoundModeLevelAdjustment,
            F1::AmbientSoundControlModeSelect,
            F1::GeneralSetting1,
            F1::GeneralSetting2,
            F1::GeneralSetting3,
            F1::UpscalingAutoOff,
            F1::ListeningOption,
            F1::AssignableSetting,
            F1::SmartTalkingModeType2,
            F1::PlaybackControlByWearingRemovingHeadphoneOnOff,
            F1::HeadGestureOnOffTraining,
        ];
        use FunctionTable2 as F2;
        let t2 = [F2::VoiceGuidanceSettingMtkTransferWithoutDisconnectionSupportLanguageSwitchAndVolumeAdjustment,
            F2::PairingDeviceManagementClassicBt,
            F2::PairingDeviceManagementWithBluetoothClassOfDeviceClassicBt];
        Self {
            has_table2: true,
            nc_asm_type: NcAsmInquiredType::AsmSeamless,
            support_t1: t1.iter().map(|f| f.to_u8()).collect(),
            support_t2: t2.iter().map(|f| f.to_u8()).collect(),
        }
    }

    /// A WH-1000XM6-like device: noise-adaptation NC/ASM variant.
    pub fn xm6() -> Self {
        let mut p = Self::xm5();
        p.nc_asm_type = NcAsmInquiredType::ModeNcAsmDualNcModeSwitchAndAsmSeamlessNa;
        p.support_t1.push(FunctionTable1::ModeNcAsmNoiseCancellingDualAmbientSoundModeLevelAdjustmentNoiseAdaptation.to_u8());
        p.support_t1
            .push(FunctionTable1::LeftRightBatteryLevelIndicator.to_u8());
        p
    }
}

/// The device side of a mock pair: responds to the engine like real
/// hardware.
pub struct MockDevice {
    tx: MockTransport,
    pub profile: DeviceProfile,
    pub state: DeviceSimState,
    /// Sequence numbers seen from the host (for ACK responses).
    seq: u8,
    /// Frames received from the host (test inspection).
    pub received: Vec<Vec<u8>>,
    recv_buf: std::collections::VecDeque<u8>,
    /// When true, the device consumes frames without responding
    /// (used to simulate an unresponsive headphone).
    pub silent: bool,
}

impl MockDevice {
    pub fn new(tx: MockTransport, profile: DeviceProfile) -> Self {
        Self {
            tx,
            profile,
            state: DeviceSimState::default(),
            seq: 0,
            received: Vec::new(),
            recv_buf: std::collections::VecDeque::new(),
            silent: false,
        }
    }

    /// Runs the device until the pipe closes.
    pub async fn run(&mut self) {
        let mut buf = [0u8; 2048];
        loop {
            match self.tx.recv(&mut buf).await {
                Ok(0) | Err(_) => break,
                Ok(n) => {
                    for &b in &buf[..n] {
                        self.push_byte(b);
                        if !self.recv_buf.is_empty() && self.recv_buf.back() == Some(&END_MARKER) {
                            let frame: Vec<u8> = self.recv_buf.drain(..).collect();
                            self.handle_frame(&frame).await;
                        }
                    }
                }
            }
        }
    }

    fn push_byte(&mut self, b: u8) {
        self.recv_buf.push_back(b);
    }

    async fn handle_frame(&mut self, frame: &[u8]) {
        let Ok((data_type, seq, data)) = unpack_full(frame) else {
            return;
        };
        self.seq = seq;
        self.received.push(data.clone());
        let responses = self.respond(data_type, &data);
        for (resp_type, payload) in responses {
            let resp = pack(resp_type, 0, &payload);
            let _r = self.tx.send(&resp).await;
        }
    }

    /// Builds the device's reply for a received payload.
    fn respond(&mut self, data_type: DataType, data: &[u8]) -> Vec<(DataType, Vec<u8>)> {
        let mut out = Vec::new();
        // Real devices ACK data frames (seq = 1 - received seq) but never
        // ACK our ACKs — no echo storms.
        if data_type != DataType::Ack {
            out.push((DataType::Ack, Vec::new()));
        }

        let cmd = *data.first().unwrap_or(&0xFF);
        if data_type == DataType::DataMdrNo2 {
            let mut t2 = self.respond_t2(cmd, data);
            out.append(&mut t2);
            return out;
        }
        match CommandT1::from_u8(cmd) {
            CommandT1::ConnectGetProtocolInfo => {
                out.push((
                    DataType::DataMdr,
                    ConnectRetProtocolInfo {
                        protocol_version: 1,
                        support_table1: EnableDisable::Enable,
                        support_table2: if self.profile.has_table2 {
                            EnableDisable::Enable
                        } else {
                            EnableDisable::Disable
                        },
                    }
                    .serialize(),
                ));
            }
            CommandT1::ConnectGetCapabilityInfo => {
                out.push((
                    DataType::DataMdr,
                    vec![
                        0x03, 0x00, 0x00, 0x09, b'M', b'D', b'R', b'-', b'T', b'E', b'S', b'T',
                        b'1',
                    ],
                ));
            }
            CommandT1::ConnectGetDeviceInfo => {
                let ty = data.get(1).copied().unwrap_or(0);
                match DeviceInfoType::from_u8(ty) {
                    DeviceInfoType::ModelName => {
                        out.push((
                            DataType::DataMdr,
                            DeviceInfoResponse::ModelName("WH-1000XM5".into()).serialize(),
                        ));
                    }
                    DeviceInfoType::FwVersion => {
                        out.push((
                            DataType::DataMdr,
                            DeviceInfoResponse::FwVersion("2.0.5".into()).serialize(),
                        ));
                    }
                    _ => {
                        out.push((
                            DataType::DataMdr,
                            DeviceInfoResponse::SeriesAndColor {
                                series: ModelSeriesType::Premium,
                                color: ModelColor::Black,
                            }
                            .serialize(),
                        ));
                    }
                }
            }
            CommandT1::ConnectGetSupportFunction => {
                out.push((
                    DataType::DataMdr,
                    ConnectRetSupportFunction {
                        functions: self
                            .profile
                            .support_t1
                            .iter()
                            .map(|&f| SupportFunction {
                                function: f,
                                priority: 0,
                            })
                            .collect(),
                    }
                    .serialize(),
                ));
            }
            CommandT1::CommonGetStatus => {
                out.push((
                    DataType::DataMdr,
                    CommonStatusAudioCodec {
                        audio_codec: AudioCodec::Ldac,
                    }
                    .serialize(),
                ));
            }
            CommandT1::AlertSetStatus => {
                // No response needed.
            }
            CommandT1::PlayGetParam => {
                let ty = data.get(1).copied().unwrap_or(0);
                match PlayInquiredType::from_u8(ty) {
                    PlayInquiredType::PlaybackControlWithCallVolumeAdjustment => {
                        out.push((
                            DataType::DataMdr,
                            PlayParamPlaybackControllerName {
                                type_: PlayInquiredType::PlaybackControlWithCallVolumeAdjustment,
                                names: vec![
                                    PlaybackName {
                                        status: PlaybackNameStatus::Settled,
                                        name: self.state.title.clone(),
                                    },
                                    PlaybackName {
                                        status: PlaybackNameStatus::Settled,
                                        name: self.state.album.clone(),
                                    },
                                    PlaybackName {
                                        status: PlaybackNameStatus::Settled,
                                        name: self.state.artist.clone(),
                                    },
                                    PlaybackName {
                                        status: PlaybackNameStatus::Nothing,
                                        name: String::new(),
                                    },
                                ],
                            }
                            .serialize(),
                        ));
                    }
                    _ => {
                        out.push((
                            DataType::DataMdr,
                            ret(
                                PlayParamPlaybackControllerVolume {
                                    type_: PlayInquiredType::MusicVolume,
                                    volume: self.state.volume,
                                }
                                .serialize(),
                                CommandT1::PlayRetParam.to_u8(),
                            ),
                        ));
                    }
                }
            }
            CommandT1::PlayGetStatus => {
                out.push((
                    DataType::DataMdr,
                    PlayStatusPlaybackController {
                        type_: PlayInquiredType::PlaybackControlWithCallVolumeAdjustment,
                        status: EnableDisable::Enable,
                        playback_status: self.state.play_status,
                        music_call_status: MusicCallStatus::Music,
                    }
                    .serialize(),
                ));
            }
            CommandT1::NcAsmGetParam => {
                let ty = data.get(1).copied().unwrap_or(0);
                match NcAsmInquiredType::from_u8(ty) {
                    NcAsmInquiredType::AsmSeamless => out.push((
                        DataType::DataMdr,
                        ret(NcAsmParamAsmSeamless {
                            base: NcAsmParamBase {
                                type_: NcAsmInquiredType::AsmSeamless,
                                value_change_status: ValueChangeStatus::Changed,
                                nc_asm_total_effect: if self.state.nc_asm_enabled {
                                    NcAsmOnOffValue::On
                                } else {
                                    NcAsmOnOffValue::Off
                                },
                            },
                            ambient_sound_mode: if self.state.focus_on_voice {
                                AmbientSoundMode::Voice
                            } else {
                                AmbientSoundMode::Normal
                            },
                            ambient_sound_level: self.state.ambient_level,
                        }
                        .serialize(), CommandT1::NcAsmRetParam.to_u8()),
                    )),
                    NcAsmInquiredType::ModeNcAsmDualNcModeSwitchAndAsmSeamless => out.push((
                        DataType::DataMdr,
                        ret(NcAsmParamModeNcDualModeSwitchAsmSeamless {
                            base: NcAsmParamBase {
                                type_: NcAsmInquiredType::ModeNcAsmDualNcModeSwitchAndAsmSeamless,
                                value_change_status: ValueChangeStatus::Changed,
                                nc_asm_total_effect: if self.state.nc_asm_enabled {
                                    NcAsmOnOffValue::On
                                } else {
                                    NcAsmOnOffValue::Off
                                },
                            },
                            nc_asm_mode: self.state.nc_asm_mode,
                            ambient_sound_mode: if self.state.focus_on_voice {
                                AmbientSoundMode::Voice
                            } else {
                                AmbientSoundMode::Normal
                            },
                            ambient_sound_level: self.state.ambient_level,
                        }
                        .serialize(), CommandT1::NcAsmRetParam.to_u8()),
                    )),
                    NcAsmInquiredType::ModeNcAsmDualNcModeSwitchAndAsmSeamlessNa => out.push((
                        DataType::DataMdr,
                        ret(NcAsmParamModeNcDualModeSwitchAsmSeamlessNa {
                            base: NcAsmParamBase {
                                type_: NcAsmInquiredType::ModeNcAsmDualNcModeSwitchAndAsmSeamlessNa,
                                value_change_status: ValueChangeStatus::Changed,
                                nc_asm_total_effect: if self.state.nc_asm_enabled {
                                    NcAsmOnOffValue::On
                                } else {
                                    NcAsmOnOffValue::Off
                                },
                            },
                            nc_asm_mode: self.state.nc_asm_mode,
                            ambient_sound_mode: if self.state.focus_on_voice {
                                AmbientSoundMode::Voice
                            } else {
                                AmbientSoundMode::Normal
                            },
                            ambient_sound_level: self.state.ambient_level,
                            noise_adaptive_on_off: if self.state.auto_asm {
                                NcAsmOnOffValue::On
                            } else {
                                NcAsmOnOffValue::Off
                            },
                            noise_adaptive_sensitivity: self.state.sensitivity,
                        }
                        .serialize(), CommandT1::NcAsmRetParam.to_u8()),
                    )),
                    NcAsmInquiredType::NcAmbToggle => out.push((
                        DataType::DataMdr,
                        ret(NcAsmParamNcAmbToggle {
                            type_: NcAsmInquiredType::NcAmbToggle,
                            function: self.state.nc_amb_function,
                        }
                        .serialize(), CommandT1::NcAsmRetParam.to_u8()),
                    )),
                    _ => {}
                }
            }
            CommandT1::EqEbbGetStatus => {
                out.push((DataType::DataMdr, vec![0x53, 0x00, 0x00]));
            }
            CommandT1::EqEbbGetParam => {
                let bands = if self.state.eq_preset == EqPresetId::Custom {
                    self.state.eq_bands.clone()
                } else {
                    Vec::new()
                };
                out.push((
                    DataType::DataMdr,
                    ret(
                        EqEbbParamEq {
                            preset_id: self.state.eq_preset,
                            bands,
                        }
                        .serialize(),
                        CommandT1::EqEbbRetParam.to_u8(),
                    ),
                ));
            }
            CommandT1::AudioGetCapability => {
                out.push((
                    DataType::DataMdr,
                    AudioRetCapabilityUpscaling {
                        upscaling_type: UpscalingType::DseeHx,
                    }
                    .serialize(),
                ));
            }
            CommandT1::AudioGetStatus => {
                out.push((
                    DataType::DataMdr,
                    AudioStatusCommon {
                        type_: AudioInquiredType::Upscaling,
                        status: if self.state.dsee {
                            EnableDisable::Enable
                        } else {
                            EnableDisable::Disable
                        },
                    }
                    .serialize(),
                ));
            }
            CommandT1::AudioGetParam => {
                let ty = data.get(1).copied().unwrap_or(0);
                match AudioInquiredType::from_u8(ty) {
                    AudioInquiredType::Upscaling => out.push((
                        DataType::DataMdr,
                        ret(
                            AudioParamUpscaling {
                                type_: AudioInquiredType::Upscaling,
                                setting_value: if self.state.dsee {
                                    UpscalingTypeAutoOff::Auto
                                } else {
                                    UpscalingTypeAutoOff::Off
                                },
                            }
                            .serialize(),
                            CommandT1::AudioRetParam.to_u8(),
                        ),
                    )),
                    AudioInquiredType::BgmModeAndErrorCode => out.push((
                        DataType::DataMdr,
                        ret(
                            AudioParamBGMMode {
                                type_: AudioInquiredType::BgmModeAndErrorCode,
                                on_off: if self.state.bgm_enabled {
                                    EnableDisable::Enable
                                } else {
                                    EnableDisable::Disable
                                },
                                target_room_size: self.state.bgm_room,
                            }
                            .serialize(),
                            CommandT1::AudioRetParam.to_u8(),
                        ),
                    )),
                    AudioInquiredType::UpmixCinema => out.push((
                        DataType::DataMdr,
                        ret(
                            AudioParamUpmixCinema {
                                type_: AudioInquiredType::UpmixCinema,
                                on_off: if self.state.cinema {
                                    EnableDisable::Enable
                                } else {
                                    EnableDisable::Disable
                                },
                            }
                            .serialize(),
                            CommandT1::AudioRetParam.to_u8(),
                        ),
                    )),
                    _ => {}
                }
            }
            CommandT1::SystemGetParam => {
                let ty = data.get(1).copied().unwrap_or(0);
                match SystemInquiredType::from_u8(ty) {
                    SystemInquiredType::SmartTalkingModeType2 => out.push((
                        DataType::DataMdr,
                        ret(
                            SystemParamSmartTalking {
                                type_: SystemInquiredType::SmartTalkingModeType2,
                                on_off: if self.state.stc_enabled {
                                    EnableDisable::Enable
                                } else {
                                    EnableDisable::Disable
                                },
                                preview_mode_on_off: EnableDisable::Disable,
                            }
                            .serialize(),
                            CommandT1::SystemRetParam.to_u8(),
                        ),
                    )),
                    SystemInquiredType::AssignableSettings => out.push((
                        DataType::DataMdr,
                        ret(
                            SystemParamAssignableSettings {
                                presets: vec![self.state.touch_left, self.state.touch_right],
                            }
                            .serialize(),
                            CommandT1::SystemRetParam.to_u8(),
                        ),
                    )),
                    SystemInquiredType::PlaybackControlByWearing => out.push((
                        DataType::DataMdr,
                        ret(
                            SystemParamCommon {
                                type_: SystemInquiredType::PlaybackControlByWearing,
                                setting_value: if self.state.auto_pause {
                                    EnableDisable::Enable
                                } else {
                                    EnableDisable::Disable
                                },
                            }
                            .serialize(),
                            CommandT1::SystemRetParam.to_u8(),
                        ),
                    )),
                    SystemInquiredType::HeadGestureOnOff => out.push((
                        DataType::DataMdr,
                        ret(
                            SystemParamCommon {
                                type_: SystemInquiredType::HeadGestureOnOff,
                                setting_value: if self.state.head_gesture {
                                    EnableDisable::Enable
                                } else {
                                    EnableDisable::Disable
                                },
                            }
                            .serialize(),
                            CommandT1::SystemRetParam.to_u8(),
                        ),
                    )),
                    _ => {}
                }
            }
            CommandT1::SystemGetExtParam => {
                out.push((
                    DataType::DataMdr,
                    ret(
                        SystemExtParamSmartTalkingMode2 {
                            type_: SystemInquiredType::SmartTalkingModeType2,
                            detect_sensitivity: self.state.stc_sensitivity,
                            mode_off_time: self.state.stc_mode_out,
                        }
                        .serialize(),
                        CommandT1::SystemRetExtParam.to_u8(),
                    ),
                ));
            }
            CommandT1::PowerGetStatus => {
                let ty = data.get(1).copied().unwrap_or(0);
                let bat = |level, charging| BatteryStatus {
                    level,
                    charging,
                    threshold: 0,
                };
                let payload = PowerRetStatusBattery {
                    type_: PowerInquiredType::from_u8(ty),
                    left: bat(self.state.battery, self.state.charging),
                    right: bat(self.state.battery, self.state.charging),
                    case_: bat(60, BatteryChargingStatus::NotCharging),
                }
                .serialize();
                out.push((DataType::DataMdr, payload));
            }
            CommandT1::PowerGetParam => {
                let ty = data.get(1).copied().unwrap_or(0);
                if ty == PowerInquiredType::AutoPowerOff.to_u8() {
                    out.push((
                        DataType::DataMdr,
                        ret(
                            PowerParamAutoPowerOff {
                                type_: PowerInquiredType::AutoPowerOff,
                                current: self.state.auto_power_off,
                                last_select: 0x00,
                            }
                            .serialize(),
                            CommandT1::PowerRetParam.to_u8(),
                        ),
                    ));
                }
            }
            CommandT1::GeneralSettingGetCapability => {
                let ty = data.get(1).copied().unwrap_or(0);
                let (subject, summary) = match GsInquiredType::from_u8(ty) {
                    GsInquiredType::GeneralSetting1 => (
                        "SIDETONE_SETTING",
                        "Your own voice will be easier to hear during calls.",
                    ),
                    GsInquiredType::GeneralSetting2 => (
                        "MULTIPOINT_SETTING",
                        "For example, when using the audio device with both a PC and a smartphone.",
                    ),
                    GsInquiredType::GeneralSetting3 => {
                        ("TOUCH_PANEL_SETTING", "Touch sensor control panel.")
                    }
                    _ => ("", ""),
                };
                let mut w = codec::Writer::new(256);
                w.u8(CommandT1::GeneralSettingRetCapability.to_u8())
                    .unwrap();
                w.u8(ty).unwrap();
                w.u8(GsSettingType::BooleanType.to_u8()).unwrap();
                w.u8(GsStringFormat::EnumName.to_u8()).unwrap();
                w.prefixed_string(subject).unwrap();
                w.prefixed_string(summary).unwrap();
                out.push((DataType::DataMdr, w.into_inner()));
            }
            CommandT1::GeneralSettingGetParam => {
                let ty = data.get(1).copied().unwrap_or(0);
                let idx = match GsInquiredType::from_u8(ty) {
                    GsInquiredType::GeneralSetting1 => 0,
                    GsInquiredType::GeneralSetting2 => 1,
                    GsInquiredType::GeneralSetting3 => 2,
                    _ => 3,
                };
                let resp = ret(
                    GsParamBoolean {
                        type_: GsInquiredType::from_u8(ty),
                        setting_value: if self.state.gs_values[idx] {
                            GsSettingValue::On
                        } else {
                            GsSettingValue::Off
                        },
                    }
                    .serialize(),
                    CommandT1::GeneralSettingRetParam.to_u8(),
                );
                out.push((DataType::DataMdr, resp));
            }
            // ---- Setters ----
            CommandT1::PlaySetParam => {
                if let Ok(p) = PlayParamPlaybackControllerVolume::deserialize(data) {
                    self.state.volume = p.volume;
                }
            }
            CommandT1::PlaySetStatus => {
                if let Ok(p) = PlayStatusSetPlaybackController::deserialize(data) {
                    match p.control {
                        PlaybackControl::Play => self.state.play_status = PlaybackStatus::Play,
                        PlaybackControl::Pause => self.state.play_status = PlaybackStatus::Pause,
                        PlaybackControl::TrackUp | PlaybackControl::TrackDown => {}
                        _ => {}
                    }
                }
            }
            CommandT1::NcAsmSetParam => {
                let ty = data.get(1).copied().unwrap_or(0);
                match NcAsmInquiredType::from_u8(ty) {
                    NcAsmInquiredType::AsmSeamless => {
                        if let Ok(p) = NcAsmParamAsmSeamless::deserialize(data) {
                            self.state.nc_asm_enabled =
                                p.base.nc_asm_total_effect == NcAsmOnOffValue::On;
                            // ASM_SEAMLESS carries no explicit mode: enabled
                            // implies Ambient Sound, disabled implies Off.
                            if self.state.nc_asm_enabled {
                                self.state.nc_asm_mode = NcAsmMode::Asm;
                            }
                            self.state.ambient_level = p.ambient_sound_level;
                            self.state.focus_on_voice =
                                p.ambient_sound_mode == AmbientSoundMode::Voice;
                        }
                    }
                    NcAsmInquiredType::ModeNcAsmDualNcModeSwitchAndAsmSeamless => {
                        if let Ok(p) = NcAsmParamModeNcDualModeSwitchAsmSeamless::deserialize(data)
                        {
                            self.state.nc_asm_enabled =
                                p.base.nc_asm_total_effect == NcAsmOnOffValue::On;
                            self.state.nc_asm_mode = p.nc_asm_mode;
                            self.state.ambient_level = p.ambient_sound_level;
                            self.state.focus_on_voice =
                                p.ambient_sound_mode == AmbientSoundMode::Voice;
                        }
                    }
                    NcAsmInquiredType::ModeNcAsmDualNcModeSwitchAndAsmSeamlessNa => {
                        if let Ok(p) =
                            NcAsmParamModeNcDualModeSwitchAsmSeamlessNa::deserialize(data)
                        {
                            self.state.nc_asm_enabled =
                                p.base.nc_asm_total_effect == NcAsmOnOffValue::On;
                            self.state.nc_asm_mode = p.nc_asm_mode;
                            self.state.ambient_level = p.ambient_sound_level;
                            self.state.focus_on_voice =
                                p.ambient_sound_mode == AmbientSoundMode::Voice;
                            self.state.auto_asm = p.noise_adaptive_on_off == NcAsmOnOffValue::On;
                            self.state.sensitivity = p.noise_adaptive_sensitivity;
                        }
                    }
                    NcAsmInquiredType::NcAmbToggle => {
                        if let Ok(p) = NcAsmParamNcAmbToggle::deserialize(data) {
                            self.state.nc_amb_function = p.function;
                        }
                    }
                    _ => {}
                }
            }
            CommandT1::EqEbbSetParam => {
                if let Ok(p) = EqEbbParamEq::deserialize(data) {
                    self.state.eq_preset = p.preset_id;
                    if !p.bands.is_empty() {
                        self.state.eq_bands = p.bands;
                    }
                }
            }
            CommandT1::AudioSetParam => {
                let ty = data.get(1).copied().unwrap_or(0);
                match AudioInquiredType::from_u8(ty) {
                    AudioInquiredType::Upscaling => {
                        if let Ok(p) = AudioParamUpscaling::deserialize(data) {
                            self.state.dsee = p.setting_value == UpscalingTypeAutoOff::Auto;
                        }
                    }
                    AudioInquiredType::BgmModeAndErrorCode => {
                        if let Ok(p) = AudioParamBGMMode::deserialize(data) {
                            self.state.bgm_enabled = p.on_off == EnableDisable::Enable;
                            self.state.bgm_room = p.target_room_size;
                        }
                    }
                    AudioInquiredType::UpmixCinema => {
                        if let Ok(p) = AudioParamUpmixCinema::deserialize(data) {
                            self.state.cinema = p.on_off == EnableDisable::Enable;
                        }
                    }
                    _ => {}
                }
            }
            CommandT1::SystemSetParam => {
                let ty = data.get(1).copied().unwrap_or(0);
                match SystemInquiredType::from_u8(ty) {
                    SystemInquiredType::SmartTalkingModeType2 => {
                        if let Ok(p) = SystemParamSmartTalking::deserialize(data) {
                            self.state.stc_enabled = p.on_off == EnableDisable::Enable;
                        }
                    }
                    SystemInquiredType::AssignableSettings => {
                        if let Ok(p) = SystemParamAssignableSettings::deserialize(data) {
                            if p.presets.len() >= 2 {
                                self.state.touch_left = p.presets[0];
                                self.state.touch_right = p.presets[1];
                            }
                        }
                    }
                    SystemInquiredType::PlaybackControlByWearing => {
                        if let Ok(p) = SystemParamCommon::deserialize(data) {
                            self.state.auto_pause = p.setting_value == EnableDisable::Enable;
                        }
                    }
                    SystemInquiredType::HeadGestureOnOff => {
                        if let Ok(p) = SystemParamCommon::deserialize(data) {
                            self.state.head_gesture = p.setting_value == EnableDisable::Enable;
                        }
                    }
                    _ => {}
                }
            }
            CommandT1::SystemSetExtParam => {
                if let Ok(p) = SystemExtParamSmartTalkingMode2::deserialize(data) {
                    self.state.stc_sensitivity = p.detect_sensitivity;
                    self.state.stc_mode_out = p.mode_off_time;
                }
            }
            CommandT1::PowerSetParam => {
                if let Ok(p) = PowerParamAutoPowerOff::deserialize(data) {
                    self.state.auto_power_off = p.current;
                }
            }
            CommandT1::PowerSetStatus => {
                // Power off: nothing more to do.
            }
            CommandT1::GeneralSettingSetParam => {
                if let Ok(p) = GsParamBoolean::deserialize(data) {
                    let idx = match p.type_ {
                        GsInquiredType::GeneralSetting1 => 0,
                        GsInquiredType::GeneralSetting2 => 1,
                        GsInquiredType::GeneralSetting3 => 2,
                        _ => 3,
                    };
                    self.state.gs_values[idx] = p.setting_value == GsSettingValue::On;
                }
            }
            _ => {}
        }
        out
    }

    fn respond_t2(&mut self, cmd: u8, data: &[u8]) -> Vec<(DataType, Vec<u8>)> {
        let mut out = Vec::new();
        match CommandT2::from_u8(cmd) {
            CommandT2::ConnectGetSupportFunction => {
                out.push((
                    DataType::DataMdrNo2,
                    ConnectRetSupportFunction {
                        functions: self
                            .profile
                            .support_t2
                            .iter()
                            .map(|&f| SupportFunction {
                                function: f,
                                priority: 0,
                            })
                            .collect(),
                    }
                    .serialize(),
                ));
            }
            CommandT2::VoiceGuidanceGetParam => {
                let ty = data.get(1).copied().unwrap_or(0);
                match VoiceGuidanceInquiredType::from_u8(ty) {
                    VoiceGuidanceInquiredType::MtkTransferWoDisconnectionSupportLanguageSwitch => {
                        out.push((
                            DataType::DataMdrNo2,
                            ret(
                                VoiceGuidanceParamSettingMtk {
                                    type_: VoiceGuidanceInquiredType::MtkTransferWoDisconnectionSupportLanguageSwitch,
                                    setting_value: if self.state.voice_guidance {
                                        OnOffSetting::On
                                    } else {
                                        OnOffSetting::Off
                                    },
                                }
                                .serialize(),
                                CommandT2::VoiceGuidanceRetParam.to_u8(),
                            ),
                        ));
                    }
                    _ => {
                        out.push((
                            DataType::DataMdrNo2,
                            ret(
                                VoiceGuidanceSetParamVolume {
                                    volume: self.state.voice_guidance_volume,
                                    feedback_sound: OnOffSetting::On,
                                }
                                .serialize(),
                                CommandT2::VoiceGuidanceRetParam.to_u8(),
                            ),
                        ));
                    }
                }
            }
            CommandT2::VoiceGuidanceSetParam => {
                let ty = data.get(1).copied().unwrap_or(0);
                match VoiceGuidanceInquiredType::from_u8(ty) {
                    VoiceGuidanceInquiredType::MtkTransferWoDisconnectionSupportLanguageSwitch => {
                        if let Ok(p) = VoiceGuidanceParamSettingMtk::deserialize(data) {
                            self.state.voice_guidance = p.setting_value == OnOffSetting::On;
                        }
                    }
                    _ => {
                        if let Ok(p) = VoiceGuidanceSetParamVolume::deserialize(data) {
                            self.state.voice_guidance_volume = p.volume;
                        }
                    }
                }
            }
            CommandT2::PeriGetParam => {
                let ty = data.get(1).copied().unwrap_or(0xFF);
                let type_ = match PeripheralInquiredType::from_u8(ty) {
                    PeripheralInquiredType::PairingDeviceManagementClassicBt
                    | PeripheralInquiredType::PairingDeviceManagementWithBluetoothClassOfDevice => {
                        PeripheralInquiredType::from_u8(ty)
                    }
                    _ => return out,
                };
                out.push((
                    DataType::DataMdrNo2,
                    PeripheralRetParamDeviceList {
                        type_,
                        devices: self
                            .state
                            .multipoint_devices
                            .iter()
                            .map(|(a, n, c)| PeripheralDeviceInfo {
                                address: a.clone(),
                                connected: *c,
                                name: n.clone(),
                                class_of_device: Some(0x5A020C),
                            })
                            .collect(),
                        playback_device: self.state.multipoint_playback,
                    }
                    .serialize(),
                ));
            }
            CommandT2::PeriSetExtendedParam => {
                let ty = data.get(1).copied().unwrap_or(0xFF);
                if ty == PeripheralInquiredType::SourceSwitchControl.to_u8() {
                    if let Ok(p) = PeripheralSetExtendedParamSourceSwitch::deserialize(data) {
                        if let Some(i) = self
                            .state
                            .multipoint_devices
                            .iter()
                            .position(|(a, _, _)| a == &p.address)
                        {
                            self.state.multipoint_playback = i as u8;
                            out.push((
                                DataType::DataMdrNo2,
                                PeripheralNotifyExtendedParamSourceSwitch {
                                    result: SourceSwitchControlResult::Success,
                                    address: p.address,
                                }
                                .serialize(),
                            ));
                        }
                    }
                } else if let Ok(p) = PeripheralSetExtendedParamDeviceManagement::deserialize(data)
                {
                    if p.action == ConnectivityActionType::Connect {
                        if let Some(d) = self
                            .state
                            .multipoint_devices
                            .iter_mut()
                            .find(|(a, _, _)| a == &p.address)
                        {
                            d.2 = true;
                            out.push((
                                DataType::DataMdrNo2,
                                PeripheralNotifyExtendedParamDeviceManagement {
                                    type_: p.type_,
                                    action: p.action,
                                    result: 0x10, // CONNECTION_SUCCESS
                                    address: p.address,
                                }
                                .serialize(),
                            ));
                        }
                    }
                }
            }
            _ => {}
        }
        out
    }
}

impl MockDevice {
    /// The device-side transport (tests may inject bytes directly).
    pub fn tx(&mut self) -> &mut MockTransport {
        &mut self.tx
    }

    /// Bytes pending in the device's receive pipe (test helper).
    pub fn pending_bytes(&self) -> usize {
        self.tx.pending()
    }

    /// Non-blocking: processes one batch of incoming bytes. Returns whether
    /// anything was received.
    pub async fn run_once(&mut self) -> bool {
        use sony_buds_tray_control::transport::PollStatus;
        match tokio::time::timeout(
            std::time::Duration::from_millis(1),
            self.tx.poll_read(std::time::Duration::ZERO),
        )
        .await
        {
            Ok(Ok(PollStatus::Ready)) => {
                let mut buf = [0u8; 2048];
                if let Ok(n) = self.tx.recv(&mut buf).await {
                    if n == 0 {
                        return false;
                    }
                    for &b in &buf[..n] {
                        self.push_byte(b);
                        if !self.recv_buf.is_empty() && self.recv_buf.back() == Some(&END_MARKER) {
                            let frame: Vec<u8> = self.recv_buf.drain(..).collect();
                            self.handle_frame(&frame).await;
                        }
                    }
                    return true;
                }
                false
            }
            _ => false,
        }
    }
}

/// Pumps both the engine and the mock device until both are idle (or the
/// step budget is exhausted). Returns every event the engine produced.
///
/// The engine may flush outgoing bytes without producing an event, so
/// "idle" means two consecutive rounds in which neither side did anything.
pub async fn pump(
    engine: &mut sony_buds_tray_control::device::Engine<MockTransport>,
    device: &mut MockDevice,
) -> Vec<sony_buds_tray_control::device::DeviceEvent> {
    let mut events = Vec::new();
    let mut quiet = 0u32;
    for _round in 0..400 {
        let mut acted = false;
        while device.run_once().await {
            acted = true;
        }
        if let Some(ev) = engine.poll(std::time::Duration::ZERO).await {
            events.push(ev);
            acted = true;
        }

        if acted {
            quiet = 0;
        } else {
            quiet += 1;
            if engine.is_ready() && engine.recv_len_for_tests() == 0 && quiet >= 2 {
                break;
            }
        }
    }
    events
}

/// A transport factory whose devices are exposed for inspection.
pub struct PairFactory(pub std::sync::Arc<std::sync::Mutex<Vec<MockDevice>>>);

#[async_trait::async_trait]
impl sony_buds_tray_control::app::TransportFactory for PairFactory {
    async fn create(
        &self,
        _kind: sony_buds_tray_control::transport::TransportKind,
    ) -> Result<Box<dyn sony_buds_tray_control::transport::Transport>, String> {
        let (host, device_tx) = MockTransport::pair();
        let device = MockDevice::new(device_tx, DeviceProfile::xm5());
        self.0.lock().unwrap().push(device);
        Ok(Box::new(host))
    }
}
