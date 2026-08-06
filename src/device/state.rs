//! Headphone state: reported device info, support tables and settable
//! properties with `desired`/`current` semantics (mirrors `MDRHeadphones`
//! public fields and `MDRProperty` in the reference client).

use crate::protocol::*;

/// A settable value: `desired` is what the UI wants, `current` is what the
/// device last confirmed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Prop<T> {
    pub desired: T,
    pub current: T,
}

impl<T: Clone> Prop<T> {
    pub fn new(value: T) -> Self {
        Self {
            desired: value.clone(),
            current: value,
        }
    }

    /// Sets both desired and current (e.g. after a device report).
    pub fn overwrite(&mut self, value: T) {
        self.desired = value.clone();
        self.current = value;
    }
}

impl<T: PartialEq + Clone> Prop<T> {
    pub fn dirty(&self) -> bool {
        self.desired != self.current
    }

    /// Adopts `desired` as the new `current`.
    pub fn commit(&mut self) {
        self.current = self.desired.clone();
    }
}

/// The two 256-bit support function tables.
#[derive(Debug, Clone)]
pub struct SupportFunctions {
    pub table1: [bool; 256],
    pub table2: [bool; 256],
}

impl Default for SupportFunctions {
    fn default() -> Self {
        Self {
            table1: [false; 256],
            table2: [false; 256],
        }
    }
}

impl SupportFunctions {
    pub fn contains_t1(&self, f: FunctionTable1) -> bool {
        self.table1[f.to_u8() as usize]
    }

    pub fn contains_t2(&self, f: FunctionTable2) -> bool {
        self.table2[f.to_u8() as usize]
    }

    pub fn set_t1(&mut self, f: u8, supported: bool) {
        self.table1[f as usize] = supported;
    }

    pub fn set_t2(&mut self, f: u8, supported: bool) {
        self.table2[f as usize] = supported;
    }

    /// Number of advertised table-1 functions (informational).
    pub fn count_t1(&self) -> usize {
        self.table1.iter().filter(|&&b| b).count()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BatteryState {
    pub level: u8,
    pub threshold: u8,
    pub charging: BatteryChargingStatus,
}

impl Default for BatteryState {
    fn default() -> Self {
        Self {
            level: 0,
            threshold: 0,
            charging: BatteryChargingStatus::NotCharging,
        }
    }
}

impl BatteryState {
    pub fn is_reported(&self) -> bool {
        self.threshold != 0 || self.level != 0
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GsCapability {
    pub setting_type: GsSettingType,
    pub subject: String,
    pub summary: String,
}

/// One device in the headphone's multipoint (peripheral) device list.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MultipointDevice {
    pub address: String,
    pub name: String,
    pub connected: bool,
}

/// A one-shot device management request (multipoint).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MultipointRequest {
    /// Switch playback to this device.
    Switch { address: String },
    /// Connect (attach) a paired device.
    Connect { address: String },
}

#[derive(Debug, Clone)]
pub struct DeviceState {
    pub protocol_version: u32,
    pub has_table1: bool,
    pub has_table2: bool,
    pub support: SupportFunctions,

    pub unique_id: String,
    pub fw_version: String,
    pub model_name: String,
    pub model_series: ModelSeriesType,
    pub model_color: ModelColor,
    pub audio_codec: AudioCodec,

    pub upscaling_type: UpscalingType,
    pub upscaling_available: bool,

    pub battery_left: BatteryState,
    pub battery_right: BatteryState,
    pub battery_case: BatteryState,

    pub play_title: String,
    pub play_album: String,
    pub play_artist: String,
    pub play_status: PlaybackStatus,
    pub play_volume: u8,

    pub gs_capabilities: Vec<GsCapability>,

    /// Multipoint (peripheral device management) list.
    pub multipoint_devices: Vec<MultipointDevice>,
    /// Index into `multipoint_devices` of the current playback device.
    pub multipoint_playback: Option<usize>,
}

impl Default for DeviceState {
    fn default() -> Self {
        Self {
            protocol_version: 0,
            has_table1: false,
            has_table2: false,
            support: SupportFunctions::default(),
            unique_id: String::new(),
            fw_version: String::new(),
            model_name: String::new(),
            model_series: ModelSeriesType::NoSeries,
            model_color: ModelColor::Default,
            audio_codec: AudioCodec::Unsettled,
            upscaling_type: UpscalingType::DseeHx,
            upscaling_available: false,
            battery_left: BatteryState::default(),
            battery_right: BatteryState::default(),
            battery_case: BatteryState::default(),
            play_title: String::new(),
            play_album: String::new(),
            play_artist: String::new(),
            play_status: PlaybackStatus::Unsettled,
            play_volume: 0,
            gs_capabilities: Vec::new(),
            multipoint_devices: Vec::new(),
            multipoint_playback: None,
        }
    }
}

impl DeviceState {
    /// Effective ambient state for the UI.
    pub fn ambient_enabled(&self, props: &Properties) -> bool {
        props.nc_asm_enabled.current
    }
}

/// All settable properties (subset of the reference client's property list).
#[derive(Debug, Clone)]
pub struct Properties {
    pub shutdown: Prop<bool>,
    pub nc_asm_enabled: Prop<bool>,
    pub nc_asm_mode: Prop<NcAsmMode>,
    pub nc_asm_ambient_level: Prop<u8>, // [1, 20]
    pub nc_asm_focus_on_voice: Prop<bool>,
    pub nc_asm_auto_asm_enabled: Prop<bool>,
    pub nc_asm_noise_adaptive_sensitivity: Prop<NoiseAdaptiveSensitivity>,
    pub nc_asm_button_function: Prop<Function>,

    pub play_volume: Prop<u8>, // [0, 30]
    pub play_control: Prop<PlaybackControl>,

    pub speak_to_chat_enabled: Prop<bool>,
    pub speak_to_chat_detect_sensitivity: Prop<DetectSensitivity>,
    pub speak_to_mode_out_time: Prop<ModeOutTime>,

    pub bgm_mode_enabled: Prop<bool>,
    pub bgm_mode_room_size: Prop<RoomSize>,
    pub upmix_cinema_enabled: Prop<bool>,

    pub eq_available: Prop<bool>,
    pub eq_preset_id: Prop<EqPresetId>,
    pub eq_clear_bass: Prop<i8>,  // [-10, 10]
    pub eq_config: Prop<Vec<i8>>, // band gains; 5 or 10 entries

    pub upscaling_enabled: Prop<bool>,

    pub auto_power_off: Prop<AutoPowerOffElements>,
    pub auto_pause_enabled: Prop<bool>,
    pub voice_guidance_enabled: Prop<bool>,
    pub voice_guidance_volume: Prop<i8>, // [-2, 2]
    pub touch_function_left: Prop<Preset>,
    pub touch_function_right: Prop<Preset>,
    pub head_gesture_enabled: Prop<bool>,
    pub gs_param_bool: [Prop<bool>; 4],
    /// One-shot multipoint request (switch/connect); consumed on commit.
    pub multipoint_request: Prop<Option<MultipointRequest>>,
}

impl Default for Properties {
    fn default() -> Self {
        Self {
            shutdown: Prop::new(false),
            nc_asm_enabled: Prop::new(false),
            nc_asm_mode: Prop::new(NcAsmMode::Nc),
            nc_asm_ambient_level: Prop::new(12),
            nc_asm_focus_on_voice: Prop::new(false),
            nc_asm_auto_asm_enabled: Prop::new(false),
            nc_asm_noise_adaptive_sensitivity: Prop::new(NoiseAdaptiveSensitivity::Standard),
            nc_asm_button_function: Prop::new(Function::NcAsm),
            play_volume: Prop::new(12),
            play_control: Prop::new(PlaybackControl::KeyOff),
            speak_to_chat_enabled: Prop::new(false),
            speak_to_chat_detect_sensitivity: Prop::new(DetectSensitivity::Auto),
            speak_to_mode_out_time: Prop::new(ModeOutTime::Mid),
            bgm_mode_enabled: Prop::new(false),
            bgm_mode_room_size: Prop::new(RoomSize::Small),
            upmix_cinema_enabled: Prop::new(false),
            eq_available: Prop::new(false),
            eq_preset_id: Prop::new(EqPresetId::Off),
            eq_clear_bass: Prop::new(0),
            eq_config: Prop::new(Vec::new()),
            upscaling_enabled: Prop::new(false),
            auto_power_off: Prop::new(AutoPowerOffElements::PowerOffDisable),
            auto_pause_enabled: Prop::new(false),
            voice_guidance_enabled: Prop::new(true),
            voice_guidance_volume: Prop::new(0),
            touch_function_left: Prop::new(Preset::PlaybackControl),
            touch_function_right: Prop::new(Preset::PlaybackControl),
            head_gesture_enabled: Prop::new(false),
            gs_param_bool: [
                Prop::new(false),
                Prop::new(false),
                Prop::new(false),
                Prop::new(false),
            ],
            multipoint_request: Prop::new(None),
        }
    }
}

impl Properties {
    /// Mirrors `MDRHeadphones::IsDirty`.
    pub fn is_dirty(&self) -> bool {
        self.shutdown.dirty()
            || self.nc_asm_enabled.dirty()
            || self.nc_asm_mode.dirty()
            || self.nc_asm_ambient_level.dirty()
            || self.nc_asm_focus_on_voice.dirty()
            || self.nc_asm_auto_asm_enabled.dirty()
            || self.nc_asm_noise_adaptive_sensitivity.dirty()
            || self.nc_asm_button_function.dirty()
            || self.play_volume.dirty()
            || self.play_control.dirty()
            || self.speak_to_chat_enabled.dirty()
            || self.speak_to_chat_detect_sensitivity.dirty()
            || self.speak_to_mode_out_time.dirty()
            || self.bgm_mode_enabled.dirty()
            || self.bgm_mode_room_size.dirty()
            || self.upmix_cinema_enabled.dirty()
            || self.eq_available.dirty()
            || self.eq_preset_id.dirty()
            || self.eq_clear_bass.dirty()
            || self.eq_config.dirty()
            || self.upscaling_enabled.dirty()
            || self.auto_power_off.dirty()
            || self.auto_pause_enabled.dirty()
            || self.voice_guidance_enabled.dirty()
            || self.voice_guidance_volume.dirty()
            || self.touch_function_left.dirty()
            || self.touch_function_right.dirty()
            || self.head_gesture_enabled.dirty()
            || self.gs_param_bool.iter().any(|p| p.dirty())
            || self.multipoint_request.dirty()
    }

    /// Volume clamping to the device's [0, 30] range.
    pub fn set_volume(&mut self, v: i32) {
        self.play_volume.desired = v.clamp(0, 30) as u8;
    }

    pub fn nudge_volume(&mut self, delta: i32) {
        self.set_volume(self.play_volume.desired as i32 + delta);
    }

    pub fn nudge_ambient_level(&mut self, delta: i32) {
        self.nc_asm_ambient_level.desired =
            (self.nc_asm_ambient_level.desired as i32 + delta).clamp(1, 20) as u8;
    }

    /// Replaces the current EQ band gains (5- or 10-band).
    pub fn set_eq_config(&mut self, gains: Vec<i8>) {
        self.eq_config.desired = gains;
    }

    pub fn nudge_eq_band(&mut self, band: usize, delta: i32) {
        let (mn, mx) = if self.eq_config.desired.len() == 5 {
            (-10, 10)
        } else {
            (-6, 6)
        };
        let Some(g) = self.eq_config.desired.get_mut(band) else {
            return;
        };
        *g = (*g as i32 + delta).clamp(mn, mx) as i8;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prop_semantics() {
        let mut p = Prop::new(5);
        assert!(!p.dirty());
        p.desired = 7;
        assert!(p.dirty());
        p.commit();
        assert_eq!(p.current, 7);
        assert!(!p.dirty());
        p.overwrite(3);
        assert_eq!((p.desired, p.current), (3, 3));
    }

    #[test]
    fn dirty_tracking() {
        let mut props = Properties::default();
        assert!(!props.is_dirty());
        props.play_volume.desired = 20;
        assert!(props.is_dirty());
        props.play_volume.commit();
        assert!(!props.is_dirty());
        props.gs_param_bool[2].desired = true;
        assert!(props.is_dirty());
    }

    #[test]
    fn clamping() {
        let mut props = Properties::default();
        props.set_volume(99);
        assert_eq!(props.play_volume.desired, 30);
        props.set_volume(-4);
        assert_eq!(props.play_volume.desired, 0);
        props.set_volume(17);
        assert_eq!(props.play_volume.desired, 17);
        props.nudge_volume(2);
        assert_eq!(props.play_volume.desired, 19);
    }

    #[test]
    fn ambient_level_clamped_to_1_20() {
        let mut props = Properties::default();
        props.nc_asm_ambient_level.desired = 20;
        props.nudge_ambient_level(1);
        assert_eq!(props.nc_asm_ambient_level.desired, 20);
        props.nudge_ambient_level(-30);
        assert_eq!(props.nc_asm_ambient_level.desired, 1);
    }

    #[test]
    fn eq_band_clamping() {
        let mut props = Properties::default();
        props.set_eq_config(vec![0, 0, 0, 0, 0, 0, 0, 0, 0, 0]);
        props.nudge_eq_band(0, 99);
        assert_eq!(props.eq_config.desired[0], 6);
        props.nudge_eq_band(0, -99);
        assert_eq!(props.eq_config.desired[0], -6);
        props.nudge_eq_band(11, 1); // out of range is a no-op
        assert_eq!(props.eq_config.desired.len(), 10);
    }
}
