//! MDR V2 payload structs with exact wire layouts.
//!
//! Byte layouts mirror the payload structs in
//! `libmdr/include/mdr/ProtocolV2T1.hpp` and `ProtocolV2T2.hpp` from the
//! SonyHeadphonesClient project. Every payload starts with its command byte.

use super::codec::{read_pod_array, read_prefixed_string, Reader, SerError, SerResult, Writer};
use super::enums::*;

/// A payload that can be written to (for sending) and read from (for receiving).
pub trait Payload: Sized {
    /// Serializes the complete payload including the command byte.
    fn write(&self, w: &mut Writer) -> SerResult<()>;

    /// Deserializes a complete payload including the command byte.
    fn read(r: &mut Reader<'_>) -> SerResult<Self>;

    /// Convenience: serialize into a Vec.
    fn serialize(&self) -> Vec<u8> {
        let mut w = Writer::new(64);
        self.write(&mut w).expect("payload serialization failed");
        w.into_inner()
    }

    /// Convenience: deserialize from a Vec.
    fn deserialize(data: &[u8]) -> SerResult<Self> {
        let mut r = Reader::new(data);
        Self::read(&mut r)
    }
}

// ---------------------------------------------------------------------------
// Connect
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConnectGetProtocolInfo;

impl Payload for ConnectGetProtocolInfo {
    fn write(&self, w: &mut Writer) -> SerResult<()> {
        w.u8(CommandT1::ConnectGetProtocolInfo.to_u8())?;
        w.u8(ConnectInquiredType::FixedValue.to_u8())
    }
    fn read(r: &mut Reader<'_>) -> SerResult<Self> {
        r.u8()?;
        r.u8()?;
        Ok(Self)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConnectRetProtocolInfo {
    pub protocol_version: u32,
    /// `EnableDisable`: 0 = supported, 1 = not supported.
    pub support_table1: EnableDisable,
    pub support_table2: EnableDisable,
}

impl Payload for ConnectRetProtocolInfo {
    fn write(&self, w: &mut Writer) -> SerResult<()> {
        w.u8(CommandT1::ConnectRetProtocolInfo.to_u8())?;
        w.u8(ConnectInquiredType::FixedValue.to_u8())?;
        w.u32_be(self.protocol_version)?;
        w.u8(self.support_table1.to_u8())?;
        w.u8(self.support_table2.to_u8())
    }
    fn read(r: &mut Reader<'_>) -> SerResult<Self> {
        r.u8()?;
        r.u8()?;
        Ok(Self {
            protocol_version: r.u32_be()?,
            support_table1: EnableDisable::from_u8(r.u8()?),
            support_table2: EnableDisable::from_u8(r.u8()?),
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConnectGetCapabilityInfo;

impl Payload for ConnectGetCapabilityInfo {
    fn write(&self, w: &mut Writer) -> SerResult<()> {
        w.u8(CommandT1::ConnectGetCapabilityInfo.to_u8())?;
        w.u8(ConnectInquiredType::FixedValue.to_u8())
    }
    fn read(r: &mut Reader<'_>) -> SerResult<Self> {
        r.u8()?;
        r.u8()?;
        Ok(Self)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConnectGetDeviceInfo {
    pub device_info_type: DeviceInfoType,
}

impl Payload for ConnectGetDeviceInfo {
    fn write(&self, w: &mut Writer) -> SerResult<()> {
        w.u8(CommandT1::ConnectGetDeviceInfo.to_u8())?;
        w.u8(self.device_info_type.to_u8())
    }
    fn read(r: &mut Reader<'_>) -> SerResult<Self> {
        r.u8()?;
        Ok(Self {
            device_info_type: DeviceInfoType::from_u8(r.u8()?),
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DeviceInfoResponse {
    ModelName(String),
    FwVersion(String),
    SeriesAndColor {
        series: ModelSeriesType,
        color: ModelColor,
    },
}

impl Payload for DeviceInfoResponse {
    fn write(&self, w: &mut Writer) -> SerResult<()> {
        w.u8(CommandT1::ConnectRetDeviceInfo.to_u8())?;
        match self {
            Self::ModelName(v) => {
                w.u8(DeviceInfoType::ModelName.to_u8())?;
                w.prefixed_string(v)
            }
            Self::FwVersion(v) => {
                w.u8(DeviceInfoType::FwVersion.to_u8())?;
                w.prefixed_string(v)
            }
            Self::SeriesAndColor { series, color } => {
                w.u8(DeviceInfoType::SeriesAndColorInfo.to_u8())?;
                w.u8(series.to_u8())?;
                w.u8(color.to_u8())
            }
        }
    }
    fn read(r: &mut Reader<'_>) -> SerResult<Self> {
        r.u8()?;
        match DeviceInfoType::from_u8(r.u8()?) {
            DeviceInfoType::ModelName => Ok(Self::ModelName(read_prefixed_string(r)?)),
            DeviceInfoType::FwVersion => Ok(Self::FwVersion(read_prefixed_string(r)?)),
            DeviceInfoType::SeriesAndColorInfo => Ok(Self::SeriesAndColor {
                series: ModelSeriesType::from_u8(r.u8()?),
                color: ModelColor::from_u8(r.u8()?),
            }),
            _ => Err(SerError::Malformed),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConnectGetSupportFunction;

impl Payload for ConnectGetSupportFunction {
    fn write(&self, w: &mut Writer) -> SerResult<()> {
        w.u8(CommandT1::ConnectGetSupportFunction.to_u8())?;
        w.u8(ConnectInquiredType::FixedValue.to_u8())
    }
    fn read(r: &mut Reader<'_>) -> SerResult<Self> {
        r.u8()?;
        r.u8()?;
        Ok(Self)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SupportFunction {
    pub function: u8,
    pub priority: u8,
}

/// `ConnectRetSupportFunction` — a length-prefixed list of support functions.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConnectRetSupportFunction {
    pub functions: Vec<SupportFunction>,
}

impl Payload for ConnectRetSupportFunction {
    fn write(&self, w: &mut Writer) -> SerResult<()> {
        w.u8(CommandT1::ConnectRetSupportFunction.to_u8())?;
        w.u8(ConnectInquiredType::FixedValue.to_u8())?;
        if self.functions.len() >= 256 {
            return Err(SerError::InvalidArgument);
        }
        w.u8(self.functions.len() as u8)?;
        for f in &self.functions {
            w.u8(f.function)?;
            w.u8(f.priority)?;
        }
        Ok(())
    }
    fn read(r: &mut Reader<'_>) -> SerResult<Self> {
        r.u8()?;
        r.u8()?;
        let count = r.u8()? as usize;
        let mut functions = Vec::with_capacity(count);
        for _ in 0..count {
            functions.push(SupportFunction {
                function: r.u8()?,
                priority: r.u8()?,
            });
        }
        Ok(Self { functions })
    }
}

// ---------------------------------------------------------------------------
// Common
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommonGetStatus {
    pub type_: CommonInquiredType,
}

impl Payload for CommonGetStatus {
    fn write(&self, w: &mut Writer) -> SerResult<()> {
        w.u8(CommandT1::CommonGetStatus.to_u8())?;
        w.u8(self.type_.to_u8())
    }
    fn read(r: &mut Reader<'_>) -> SerResult<Self> {
        r.u8()?;
        Ok(Self {
            type_: CommonInquiredType::from_u8(r.u8()?),
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommonStatusAudioCodec {
    pub audio_codec: AudioCodec,
}

impl Payload for CommonStatusAudioCodec {
    fn write(&self, w: &mut Writer) -> SerResult<()> {
        w.u8(CommandT1::CommonRetStatus.to_u8())?;
        w.u8(CommonInquiredType::AudioCodec.to_u8())?;
        w.u8(self.audio_codec.to_u8())
    }
    fn read(r: &mut Reader<'_>) -> SerResult<Self> {
        r.u8()?;
        r.u8()?;
        Ok(Self {
            audio_codec: AudioCodec::from_u8(r.u8()?),
        })
    }
}

// ---------------------------------------------------------------------------
// Power
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PowerGetStatus {
    pub type_: PowerInquiredType,
}

impl Payload for PowerGetStatus {
    fn write(&self, w: &mut Writer) -> SerResult<()> {
        w.u8(CommandT1::PowerGetStatus.to_u8())?;
        w.u8(self.type_.to_u8())
    }
    fn read(r: &mut Reader<'_>) -> SerResult<Self> {
        r.u8()?;
        Ok(Self {
            type_: PowerInquiredType::from_u8(r.u8()?),
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct BatteryStatus {
    pub level: u8,
    pub charging: BatteryChargingStatus,
    pub threshold: u8,
}

/// Any of the battery report payloads, keyed by the inquired type.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PowerRetStatusBattery {
    pub type_: PowerInquiredType,
    pub left: BatteryStatus,
    pub right: BatteryStatus,
    pub case_: BatteryStatus,
}

impl Payload for PowerRetStatusBattery {
    fn write(&self, w: &mut Writer) -> SerResult<()> {
        w.u8(CommandT1::PowerRetStatus.to_u8())?;
        w.u8(self.type_.to_u8())?;
        match self.type_ {
            PowerInquiredType::CradleBattery | PowerInquiredType::CradleBatteryWithThreshold => {
                write_battery_status(w, &self.case_)?;
            }
            _ => write_battery_status(w, &self.left)?,
        }
        match self.type_ {
            PowerInquiredType::LeftRightBattery | PowerInquiredType::LrBatteryWithThreshold => {
                write_battery_status(w, &self.right)?;
            }
            _ => {}
        }
        match self.type_ {
            PowerInquiredType::BatteryWithThreshold | PowerInquiredType::LrBatteryWithThreshold => {
                w.u8(self.left.threshold)?;
            }
            PowerInquiredType::CradleBatteryWithThreshold => {
                w.u8(self.case_.threshold)?;
            }
            _ => {}
        }
        if self.type_ == PowerInquiredType::LrBatteryWithThreshold {
            w.u8(self.right.threshold)?;
        }
        Ok(())
    }
    fn read(r: &mut Reader<'_>) -> SerResult<Self> {
        r.u8()?;
        let type_ = PowerInquiredType::from_u8(r.u8()?);
        let mut left = BatteryStatus::default();
        let mut right = BatteryStatus::default();
        let mut case_ = BatteryStatus::default();
        left.level = r.u8()?;
        left.charging = BatteryChargingStatus::from_u8(r.u8()?);
        match type_ {
            PowerInquiredType::LeftRightBattery | PowerInquiredType::LrBatteryWithThreshold => {
                right.level = r.u8()?;
                right.charging = BatteryChargingStatus::from_u8(r.u8()?);
            }
            PowerInquiredType::CradleBattery | PowerInquiredType::CradleBatteryWithThreshold => {
                case_.level = left.level;
                case_.charging = left.charging;
                left = BatteryStatus::default();
            }
            _ => {}
        }
        match type_ {
            PowerInquiredType::BatteryWithThreshold
            | PowerInquiredType::LrBatteryWithThreshold
            | PowerInquiredType::CradleBatteryWithThreshold => {
                left.threshold = r.u8()?;
                if type_ == PowerInquiredType::LrBatteryWithThreshold {
                    right.threshold = r.u8()?;
                }
                if type_ == PowerInquiredType::CradleBatteryWithThreshold {
                    case_.threshold = left.threshold;
                    left = BatteryStatus::default();
                }
            }
            _ => {}
        }
        Ok(Self {
            type_,
            left,
            right,
            case_,
        })
    }
}

fn write_battery_status(w: &mut Writer, s: &BatteryStatus) -> SerResult<()> {
    w.u8(s.level)?;
    w.u8(s.charging.to_u8())
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PowerGetParam {
    pub type_: PowerInquiredType,
}

impl Payload for PowerGetParam {
    fn write(&self, w: &mut Writer) -> SerResult<()> {
        w.u8(CommandT1::PowerGetParam.to_u8())?;
        w.u8(self.type_.to_u8())
    }
    fn read(r: &mut Reader<'_>) -> SerResult<Self> {
        r.u8()?;
        Ok(Self {
            type_: PowerInquiredType::from_u8(r.u8()?),
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PowerParamAutoPowerOff {
    pub type_: PowerInquiredType,
    pub current: u8,
    pub last_select: u8,
}

impl Payload for PowerParamAutoPowerOff {
    fn write(&self, w: &mut Writer) -> SerResult<()> {
        w.u8(CommandT1::PowerSetParam.to_u8())?;
        w.u8(self.type_.to_u8())?;
        w.u8(self.current)?;
        w.u8(self.last_select)
    }
    fn read(r: &mut Reader<'_>) -> SerResult<Self> {
        r.u8()?;
        Ok(Self {
            type_: PowerInquiredType::from_u8(r.u8()?),
            current: r.u8()?,
            last_select: r.u8()?,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PowerSetStatusPowerOff;

impl Payload for PowerSetStatusPowerOff {
    fn write(&self, w: &mut Writer) -> SerResult<()> {
        w.u8(CommandT1::PowerSetStatus.to_u8())?;
        w.u8(PowerInquiredType::PowerOff.to_u8())?;
        w.u8(PowerOffSettingValue::UserPowerOff.to_u8())
    }
    fn read(r: &mut Reader<'_>) -> SerResult<Self> {
        r.u8()?;
        r.u8()?;
        r.u8()?;
        Ok(Self)
    }
}

// ---------------------------------------------------------------------------
// EQ
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EqEbbGetStatus;

impl Payload for EqEbbGetStatus {
    fn write(&self, w: &mut Writer) -> SerResult<()> {
        w.u8(CommandT1::EqEbbGetStatus.to_u8())?;
        w.u8(EqEbbInquiredType::PresetEq.to_u8())
    }
    fn read(r: &mut Reader<'_>) -> SerResult<Self> {
        r.u8()?;
        r.u8()?;
        Ok(Self)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EqEbbGetParam;

impl Payload for EqEbbGetParam {
    fn write(&self, w: &mut Writer) -> SerResult<()> {
        w.u8(CommandT1::EqEbbGetParam.to_u8())?;
        w.u8(EqEbbInquiredType::PresetEq.to_u8())
    }
    fn read(r: &mut Reader<'_>) -> SerResult<Self> {
        r.u8()?;
        r.u8()?;
        Ok(Self)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EqEbbParamEq {
    pub preset_id: EqPresetId,
    pub bands: Vec<u8>,
}

impl Payload for EqEbbParamEq {
    fn write(&self, w: &mut Writer) -> SerResult<()> {
        w.u8(CommandT1::EqEbbSetParam.to_u8())?;
        w.u8(EqEbbInquiredType::PresetEq.to_u8())?;
        w.u8(self.preset_id.to_u8())?;
        w.pod_array(&self.bands)
    }
    fn read(r: &mut Reader<'_>) -> SerResult<Self> {
        r.u8()?;
        r.u8()?;
        Ok(Self {
            preset_id: EqPresetId::from_u8(r.u8()?),
            bands: read_pod_array(r)?,
        })
    }
}

// ---------------------------------------------------------------------------
// NC/ASM
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NcAsmGetParam {
    pub type_: NcAsmInquiredType,
}

impl Payload for NcAsmGetParam {
    fn write(&self, w: &mut Writer) -> SerResult<()> {
        w.u8(CommandT1::NcAsmGetParam.to_u8())?;
        w.u8(self.type_.to_u8())
    }
    fn read(r: &mut Reader<'_>) -> SerResult<Self> {
        r.u8()?;
        Ok(Self {
            type_: NcAsmInquiredType::from_u8(r.u8()?),
        })
    }
}

/// Base for all NC/ASM ret/set/ntfy param payloads.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct NcAsmParamBase {
    pub type_: NcAsmInquiredType,
    pub value_change_status: ValueChangeStatus,
    pub nc_asm_total_effect: NcAsmOnOffValue,
}

/// `MODE_NC_ASM_DUAL_NC_MODE_SWITCH_AND_ASM_SEAMLESS` (0x17)
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NcAsmParamModeNcDualModeSwitchAsmSeamless {
    pub base: NcAsmParamBase,
    pub nc_asm_mode: NcAsmMode,
    pub ambient_sound_mode: AmbientSoundMode,
    pub ambient_sound_level: u8,
}

impl Payload for NcAsmParamModeNcDualModeSwitchAsmSeamless {
    fn write(&self, w: &mut Writer) -> SerResult<()> {
        w.u8(CommandT1::NcAsmSetParam.to_u8())?;
        write_nc_asm_base(w, &self.base)?;
        w.u8(self.nc_asm_mode.to_u8())?;
        w.u8(self.ambient_sound_mode.to_u8())?;
        w.u8(self.ambient_sound_level)
    }
    fn read(r: &mut Reader<'_>) -> SerResult<Self> {
        r.u8()?;
        Ok(Self {
            base: read_nc_asm_base(r)?,
            nc_asm_mode: NcAsmMode::from_u8(r.u8()?),
            ambient_sound_mode: AmbientSoundMode::from_u8(r.u8()?),
            ambient_sound_level: r.u8()?,
        })
    }
}

/// `MODE_NC_ASM_DUAL_NC_MODE_SWITCH_AND_ASM_SEAMLESS_NA` (0x19, WH-1000XM6+)
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NcAsmParamModeNcDualModeSwitchAsmSeamlessNa {
    pub base: NcAsmParamBase,
    pub nc_asm_mode: NcAsmMode,
    pub ambient_sound_mode: AmbientSoundMode,
    pub ambient_sound_level: u8,
    pub noise_adaptive_on_off: NcAsmOnOffValue,
    pub noise_adaptive_sensitivity: NoiseAdaptiveSensitivity,
}

impl Payload for NcAsmParamModeNcDualModeSwitchAsmSeamlessNa {
    fn write(&self, w: &mut Writer) -> SerResult<()> {
        w.u8(CommandT1::NcAsmSetParam.to_u8())?;
        write_nc_asm_base(w, &self.base)?;
        w.u8(self.nc_asm_mode.to_u8())?;
        w.u8(self.ambient_sound_mode.to_u8())?;
        w.u8(self.ambient_sound_level)?;
        w.u8(self.noise_adaptive_on_off.to_u8())?;
        w.u8(self.noise_adaptive_sensitivity.to_u8())
    }
    fn read(r: &mut Reader<'_>) -> SerResult<Self> {
        r.u8()?;
        Ok(Self {
            base: read_nc_asm_base(r)?,
            nc_asm_mode: NcAsmMode::from_u8(r.u8()?),
            ambient_sound_mode: AmbientSoundMode::from_u8(r.u8()?),
            ambient_sound_level: r.u8()?,
            noise_adaptive_on_off: NcAsmOnOffValue::from_u8(r.u8()?),
            noise_adaptive_sensitivity: NoiseAdaptiveSensitivity::from_u8(r.u8()?),
        })
    }
}

/// `ASM_SEAMLESS` (0x22)
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NcAsmParamAsmSeamless {
    pub base: NcAsmParamBase,
    pub ambient_sound_mode: AmbientSoundMode,
    pub ambient_sound_level: u8,
}

impl Payload for NcAsmParamAsmSeamless {
    fn write(&self, w: &mut Writer) -> SerResult<()> {
        w.u8(CommandT1::NcAsmSetParam.to_u8())?;
        write_nc_asm_base(w, &self.base)?;
        w.u8(self.ambient_sound_mode.to_u8())?;
        w.u8(self.ambient_sound_level)
    }
    fn read(r: &mut Reader<'_>) -> SerResult<Self> {
        r.u8()?;
        Ok(Self {
            base: read_nc_asm_base(r)?,
            ambient_sound_mode: AmbientSoundMode::from_u8(r.u8()?),
            ambient_sound_level: r.u8()?,
        })
    }
}

/// `NC_AMB_TOGGLE` (0x30)
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NcAsmParamNcAmbToggle {
    pub type_: NcAsmInquiredType,
    pub function: Function,
}

impl Payload for NcAsmParamNcAmbToggle {
    fn write(&self, w: &mut Writer) -> SerResult<()> {
        w.u8(CommandT1::NcAsmSetParam.to_u8())?;
        w.u8(self.type_.to_u8())?;
        w.u8(self.function.to_u8())
    }
    fn read(r: &mut Reader<'_>) -> SerResult<Self> {
        r.u8()?;
        Ok(Self {
            type_: NcAsmInquiredType::from_u8(r.u8()?),
            function: Function::from_u8(r.u8()?),
        })
    }
}

fn write_nc_asm_base(w: &mut Writer, b: &NcAsmParamBase) -> SerResult<()> {
    w.u8(b.type_.to_u8())?;
    w.u8(b.value_change_status.to_u8())?;
    w.u8(b.nc_asm_total_effect.to_u8())
}

fn read_nc_asm_base(r: &mut Reader<'_>) -> SerResult<NcAsmParamBase> {
    Ok(NcAsmParamBase {
        type_: NcAsmInquiredType::from_u8(r.u8()?),
        value_change_status: ValueChangeStatus::from_u8(r.u8()?),
        nc_asm_total_effect: NcAsmOnOffValue::from_u8(r.u8()?),
    })
}

// ---------------------------------------------------------------------------
// Alert
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AlertSetStatusFixedMessage {
    pub status: EnableDisable,
}

impl Payload for AlertSetStatusFixedMessage {
    fn write(&self, w: &mut Writer) -> SerResult<()> {
        w.u8(CommandT1::AlertSetStatus.to_u8())?;
        w.u8(AlertInquiredType::FixedMessage.to_u8())?;
        w.u8(self.status.to_u8())
    }
    fn read(r: &mut Reader<'_>) -> SerResult<Self> {
        r.u8()?;
        r.u8()?;
        Ok(Self {
            status: EnableDisable::from_u8(r.u8()?),
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AlertNotifyParamFixedMessage {
    pub message_type: AlertMessageType,
    pub action_type: AlertActionType,
}

impl Payload for AlertNotifyParamFixedMessage {
    fn write(&self, w: &mut Writer) -> SerResult<()> {
        w.u8(CommandT1::AlertNtfyParam.to_u8())?;
        w.u8(AlertInquiredType::FixedMessage.to_u8())?;
        w.u8(self.message_type.to_u8())?;
        w.u8(self.action_type.to_u8())
    }
    fn read(r: &mut Reader<'_>) -> SerResult<Self> {
        r.u8()?;
        r.u8()?;
        Ok(Self {
            message_type: AlertMessageType::from_u8(r.u8()?),
            action_type: AlertActionType::from(r.u8()?),
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AlertActionType {
    ConfirmationOnly = 0,
    PositiveNegative = 1,
    PositiveConfirmationWithReply = 2,
    Unknown,
}

impl From<u8> for AlertActionType {
    fn from(v: u8) -> Self {
        match v {
            0 => Self::ConfirmationOnly,
            1 => Self::PositiveNegative,
            2 => Self::PositiveConfirmationWithReply,
            _ => Self::Unknown,
        }
    }
}

impl AlertActionType {
    pub const fn to_u8(self) -> u8 {
        match self {
            Self::ConfirmationOnly => 0,
            Self::PositiveNegative => 1,
            Self::PositiveConfirmationWithReply => 2,
            Self::Unknown => 0xFF,
        }
    }
}

// ---------------------------------------------------------------------------
// Playback
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GetPlayParam {
    pub type_: PlayInquiredType,
}

impl Payload for GetPlayParam {
    fn write(&self, w: &mut Writer) -> SerResult<()> {
        w.u8(CommandT1::PlayGetParam.to_u8())?;
        w.u8(self.type_.to_u8())
    }
    fn read(r: &mut Reader<'_>) -> SerResult<Self> {
        r.u8()?;
        Ok(Self {
            type_: PlayInquiredType::from_u8(r.u8()?),
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GetPlayStatus {
    pub type_: PlayInquiredType,
}

impl Payload for GetPlayStatus {
    fn write(&self, w: &mut Writer) -> SerResult<()> {
        w.u8(CommandT1::PlayGetStatus.to_u8())?;
        w.u8(self.type_.to_u8())
    }
    fn read(r: &mut Reader<'_>) -> SerResult<Self> {
        r.u8()?;
        Ok(Self {
            type_: PlayInquiredType::from_u8(r.u8()?),
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlayStatusPlaybackController {
    pub type_: PlayInquiredType,
    pub status: EnableDisable,
    pub playback_status: PlaybackStatus,
    pub music_call_status: MusicCallStatus,
}

impl Payload for PlayStatusPlaybackController {
    fn write(&self, w: &mut Writer) -> SerResult<()> {
        w.u8(CommandT1::PlayRetStatus.to_u8())?;
        w.u8(self.type_.to_u8())?;
        w.u8(self.status.to_u8())?;
        w.u8(self.playback_status.to_u8())?;
        w.u8(self.music_call_status.to_u8())
    }
    fn read(r: &mut Reader<'_>) -> SerResult<Self> {
        r.u8()?;
        Ok(Self {
            type_: PlayInquiredType::from_u8(r.u8()?),
            status: EnableDisable::from_u8(r.u8()?),
            playback_status: PlaybackStatus::from_u8(r.u8()?),
            music_call_status: MusicCallStatus::from_u8(r.u8()?),
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlayStatusSetPlaybackController {
    pub status: EnableDisable,
    pub control: PlaybackControl,
}

impl Payload for PlayStatusSetPlaybackController {
    fn write(&self, w: &mut Writer) -> SerResult<()> {
        w.u8(CommandT1::PlaySetStatus.to_u8())?;
        w.u8(PlayInquiredType::PlaybackControlWithCallVolumeAdjustment.to_u8())?;
        w.u8(self.status.to_u8())?;
        w.u8(self.control.to_u8())
    }
    fn read(r: &mut Reader<'_>) -> SerResult<Self> {
        r.u8()?;
        r.u8()?;
        Ok(Self {
            status: EnableDisable::from_u8(r.u8()?),
            control: PlaybackControl::from_u8(r.u8()?),
        })
    }
}

/// `PlayParamPlaybackControllerName` — 4 fixed `PlaybackName` entries.
/// Index 0 = title, 1 = album, 2 = artist.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlayParamPlaybackControllerName {
    pub type_: PlayInquiredType,
    pub names: Vec<PlaybackName>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct PlaybackName {
    pub status: PlaybackNameStatus,
    pub name: String,
}

impl Payload for PlayParamPlaybackControllerName {
    fn write(&self, w: &mut Writer) -> SerResult<()> {
        w.u8(CommandT1::PlayRetParam.to_u8())?;
        w.u8(self.type_.to_u8())?;
        if self.names.len() != 4 {
            return Err(SerError::InvalidArgument);
        }
        for n in &self.names {
            w.u8(n.status.to_u8())?;
            w.prefixed_string(&n.name)?;
        }
        Ok(())
    }
    fn read(r: &mut Reader<'_>) -> SerResult<Self> {
        r.u8()?;
        let type_ = PlayInquiredType::from_u8(r.u8()?);
        let mut names = Vec::with_capacity(4);
        for _ in 0..4 {
            let status = PlaybackNameStatus::from_u8(r.u8()?);
            let name = read_prefixed_string(r)?;
            names.push(PlaybackName { status, name });
        }
        Ok(Self { type_, names })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlayParamPlaybackControllerVolume {
    pub type_: PlayInquiredType,
    pub volume: u8,
}

impl Payload for PlayParamPlaybackControllerVolume {
    fn write(&self, w: &mut Writer) -> SerResult<()> {
        w.u8(CommandT1::PlaySetParam.to_u8())?;
        w.u8(self.type_.to_u8())?;
        w.u8(self.volume)
    }
    fn read(r: &mut Reader<'_>) -> SerResult<Self> {
        r.u8()?;
        Ok(Self {
            type_: PlayInquiredType::from_u8(r.u8()?),
            volume: r.u8()?,
        })
    }
}

// ---------------------------------------------------------------------------
// General Setting
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GsGetCapability {
    pub type_: GsInquiredType,
    pub display_language: DisplayLanguage,
}

impl Payload for GsGetCapability {
    fn write(&self, w: &mut Writer) -> SerResult<()> {
        w.u8(CommandT1::GeneralSettingGetCapability.to_u8())?;
        w.u8(self.type_.to_u8())?;
        w.u8(self.display_language.to_u8())
    }
    fn read(r: &mut Reader<'_>) -> SerResult<Self> {
        r.u8()?;
        Ok(Self {
            type_: GsInquiredType::from_u8(r.u8()?),
            display_language: DisplayLanguage::from_u8(r.u8()?),
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GsGetParam {
    pub type_: GsInquiredType,
}

impl Payload for GsGetParam {
    fn write(&self, w: &mut Writer) -> SerResult<()> {
        w.u8(CommandT1::GeneralSettingGetParam.to_u8())?;
        w.u8(self.type_.to_u8())
    }
    fn read(r: &mut Reader<'_>) -> SerResult<Self> {
        r.u8()?;
        Ok(Self {
            type_: GsInquiredType::from_u8(r.u8()?),
        })
    }
}

/// `GsParamBoolean` — used both to report and to set boolean general settings.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GsParamBoolean {
    pub type_: GsInquiredType,
    pub setting_value: GsSettingValue,
}

impl Payload for GsParamBoolean {
    fn write(&self, w: &mut Writer) -> SerResult<()> {
        w.u8(CommandT1::GeneralSettingSetParam.to_u8())?;
        w.u8(self.type_.to_u8())?;
        w.u8(GsSettingType::BooleanType.to_u8())?;
        w.u8(self.setting_value.to_u8())
    }
    fn read(r: &mut Reader<'_>) -> SerResult<Self> {
        r.u8()?;
        let type_ = GsInquiredType::from_u8(r.u8()?);
        // Skip the setting type byte (validated as BOOLEAN_TYPE on the wire).
        r.u8()?;
        Ok(Self {
            type_,
            setting_value: GsSettingValue::from_u8(r.u8()?),
        })
    }
}

/// `GsRetCapability` — capability report for one general setting.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GsRetCapability {
    pub type_: GsInquiredType,
    pub setting_type: GsSettingType,
    pub string_format: GsStringFormat,
    pub subject: String,
    pub summary: String,
}

impl Payload for GsRetCapability {
    fn write(&self, w: &mut Writer) -> SerResult<()> {
        w.u8(CommandT1::GeneralSettingRetCapability.to_u8())?;
        w.u8(self.type_.to_u8())?;
        w.u8(self.setting_type.to_u8())?;
        w.u8(self.string_format.to_u8())?;
        w.prefixed_string(&self.subject)?;
        w.prefixed_string(&self.summary)
    }
    fn read(r: &mut Reader<'_>) -> SerResult<Self> {
        r.u8()?;
        Ok(Self {
            type_: GsInquiredType::from_u8(r.u8()?),
            setting_type: GsSettingType::from_u8(r.u8()?),
            string_format: GsStringFormat::from_u8(r.u8()?),
            subject: read_prefixed_string(r)?,
            summary: read_prefixed_string(r)?,
        })
    }
}

impl GsRetCapability {
    /// Converts into the engine's capability representation.
    pub fn into_capability(self) -> super::super::device::state::GsCapability {
        super::super::device::state::GsCapability {
            setting_type: self.setting_type,
            subject: self.subject,
            summary: self.summary,
        }
    }
}

// ---------------------------------------------------------------------------
// Audio
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AudioGetCapability {
    pub type_: AudioInquiredType,
}

impl Payload for AudioGetCapability {
    fn write(&self, w: &mut Writer) -> SerResult<()> {
        w.u8(CommandT1::AudioGetCapability.to_u8())?;
        w.u8(self.type_.to_u8())
    }
    fn read(r: &mut Reader<'_>) -> SerResult<Self> {
        r.u8()?;
        Ok(Self {
            type_: AudioInquiredType::from_u8(r.u8()?),
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AudioRetCapabilityUpscaling {
    pub upscaling_type: UpscalingType,
}

impl Payload for AudioRetCapabilityUpscaling {
    fn write(&self, w: &mut Writer) -> SerResult<()> {
        w.u8(CommandT1::AudioRetCapability.to_u8())?;
        w.u8(AudioInquiredType::Upscaling.to_u8())?;
        w.u8(self.upscaling_type.to_u8())
    }
    fn read(r: &mut Reader<'_>) -> SerResult<Self> {
        r.u8()?;
        r.u8()?;
        Ok(Self {
            upscaling_type: UpscalingType::from_u8(r.u8()?),
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AudioGetStatus {
    pub type_: AudioInquiredType,
}

impl Payload for AudioGetStatus {
    fn write(&self, w: &mut Writer) -> SerResult<()> {
        w.u8(CommandT1::AudioGetStatus.to_u8())?;
        w.u8(self.type_.to_u8())
    }
    fn read(r: &mut Reader<'_>) -> SerResult<Self> {
        r.u8()?;
        Ok(Self {
            type_: AudioInquiredType::from_u8(r.u8()?),
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AudioStatusCommon {
    pub type_: AudioInquiredType,
    pub status: EnableDisable,
}

impl Payload for AudioStatusCommon {
    fn write(&self, w: &mut Writer) -> SerResult<()> {
        w.u8(CommandT1::AudioRetStatus.to_u8())?;
        w.u8(self.type_.to_u8())?;
        w.u8(self.status.to_u8())
    }
    fn read(r: &mut Reader<'_>) -> SerResult<Self> {
        r.u8()?;
        Ok(Self {
            type_: AudioInquiredType::from_u8(r.u8()?),
            status: EnableDisable::from_u8(r.u8()?),
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AudioGetParam {
    pub type_: AudioInquiredType,
}

impl Payload for AudioGetParam {
    fn write(&self, w: &mut Writer) -> SerResult<()> {
        w.u8(CommandT1::AudioGetParam.to_u8())?;
        w.u8(self.type_.to_u8())
    }
    fn read(r: &mut Reader<'_>) -> SerResult<Self> {
        r.u8()?;
        Ok(Self {
            type_: AudioInquiredType::from_u8(r.u8()?),
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AudioParamUpscaling {
    pub type_: AudioInquiredType,
    pub setting_value: UpscalingTypeAutoOff,
}

impl Payload for AudioParamUpscaling {
    fn write(&self, w: &mut Writer) -> SerResult<()> {
        w.u8(CommandT1::AudioSetParam.to_u8())?;
        w.u8(self.type_.to_u8())?;
        w.u8(self.setting_value.to_u8())
    }
    fn read(r: &mut Reader<'_>) -> SerResult<Self> {
        r.u8()?;
        Ok(Self {
            type_: AudioInquiredType::from_u8(r.u8()?),
            setting_value: UpscalingTypeAutoOff::from_u8(r.u8()?),
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AudioParamConnection {
    pub type_: AudioInquiredType,
    pub setting_value: PriorMode,
}

impl Payload for AudioParamConnection {
    fn write(&self, w: &mut Writer) -> SerResult<()> {
        w.u8(CommandT1::AudioSetParam.to_u8())?;
        w.u8(self.type_.to_u8())?;
        w.u8(self.setting_value.to_u8())
    }
    fn read(r: &mut Reader<'_>) -> SerResult<Self> {
        r.u8()?;
        Ok(Self {
            type_: AudioInquiredType::from_u8(r.u8()?),
            setting_value: PriorMode::from_u8(r.u8()?),
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AudioParamBGMMode {
    pub type_: AudioInquiredType,
    pub on_off: EnableDisable,
    pub target_room_size: RoomSize,
}

impl Payload for AudioParamBGMMode {
    fn write(&self, w: &mut Writer) -> SerResult<()> {
        w.u8(CommandT1::AudioSetParam.to_u8())?;
        w.u8(self.type_.to_u8())?;
        w.u8(self.on_off.to_u8())?;
        w.u8(self.target_room_size.to_u8())
    }
    fn read(r: &mut Reader<'_>) -> SerResult<Self> {
        r.u8()?;
        Ok(Self {
            type_: AudioInquiredType::from_u8(r.u8()?),
            on_off: EnableDisable::from_u8(r.u8()?),
            target_room_size: RoomSize::from_u8(r.u8()?),
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AudioParamUpmixCinema {
    pub type_: AudioInquiredType,
    pub on_off: EnableDisable,
}

impl Payload for AudioParamUpmixCinema {
    fn write(&self, w: &mut Writer) -> SerResult<()> {
        w.u8(CommandT1::AudioSetParam.to_u8())?;
        w.u8(self.type_.to_u8())?;
        w.u8(self.on_off.to_u8())
    }
    fn read(r: &mut Reader<'_>) -> SerResult<Self> {
        r.u8()?;
        Ok(Self {
            type_: AudioInquiredType::from_u8(r.u8()?),
            on_off: EnableDisable::from_u8(r.u8()?),
        })
    }
}

// ---------------------------------------------------------------------------
// System
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SystemGetParam {
    pub type_: SystemInquiredType,
}

impl Payload for SystemGetParam {
    fn write(&self, w: &mut Writer) -> SerResult<()> {
        w.u8(CommandT1::SystemGetParam.to_u8())?;
        w.u8(self.type_.to_u8())
    }
    fn read(r: &mut Reader<'_>) -> SerResult<Self> {
        r.u8()?;
        Ok(Self {
            type_: SystemInquiredType::from_u8(r.u8()?),
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SystemParamCommon {
    pub type_: SystemInquiredType,
    pub setting_value: EnableDisable,
}

impl Payload for SystemParamCommon {
    fn write(&self, w: &mut Writer) -> SerResult<()> {
        w.u8(CommandT1::SystemSetParam.to_u8())?;
        w.u8(self.type_.to_u8())?;
        w.u8(self.setting_value.to_u8())
    }
    fn read(r: &mut Reader<'_>) -> SerResult<Self> {
        r.u8()?;
        Ok(Self {
            type_: SystemInquiredType::from_u8(r.u8()?),
            setting_value: EnableDisable::from_u8(r.u8()?),
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SystemParamSmartTalking {
    pub type_: SystemInquiredType,
    pub on_off: EnableDisable,
    pub preview_mode_on_off: EnableDisable,
}

impl Payload for SystemParamSmartTalking {
    fn write(&self, w: &mut Writer) -> SerResult<()> {
        w.u8(CommandT1::SystemSetParam.to_u8())?;
        w.u8(self.type_.to_u8())?;
        w.u8(self.on_off.to_u8())?;
        w.u8(self.preview_mode_on_off.to_u8())
    }
    fn read(r: &mut Reader<'_>) -> SerResult<Self> {
        r.u8()?;
        Ok(Self {
            type_: SystemInquiredType::from_u8(r.u8()?),
            on_off: EnableDisable::from_u8(r.u8()?),
            preview_mode_on_off: EnableDisable::from_u8(r.u8()?),
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SystemParamAssignableSettings {
    pub presets: Vec<Preset>,
}

impl Payload for SystemParamAssignableSettings {
    fn write(&self, w: &mut Writer) -> SerResult<()> {
        w.u8(CommandT1::SystemSetParam.to_u8())?;
        w.u8(SystemInquiredType::AssignableSettings.to_u8())?;
        w.pod_array(&self.presets)
    }
    fn read(r: &mut Reader<'_>) -> SerResult<Self> {
        r.u8()?;
        r.u8()?;
        Ok(Self {
            presets: read_pod_array(r)?,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SystemGetExtParam {
    pub type_: SystemInquiredType,
}

impl Payload for SystemGetExtParam {
    fn write(&self, w: &mut Writer) -> SerResult<()> {
        w.u8(CommandT1::SystemGetExtParam.to_u8())?;
        w.u8(self.type_.to_u8())
    }
    fn read(r: &mut Reader<'_>) -> SerResult<Self> {
        r.u8()?;
        Ok(Self {
            type_: SystemInquiredType::from_u8(r.u8()?),
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SystemExtParamSmartTalkingMode2 {
    pub type_: SystemInquiredType,
    pub detect_sensitivity: DetectSensitivity,
    pub mode_off_time: ModeOutTime,
}

impl Payload for SystemExtParamSmartTalkingMode2 {
    fn write(&self, w: &mut Writer) -> SerResult<()> {
        w.u8(CommandT1::SystemSetExtParam.to_u8())?;
        w.u8(self.type_.to_u8())?;
        w.u8(self.detect_sensitivity.to_u8())?;
        w.u8(self.mode_off_time.to_u8())
    }
    fn read(r: &mut Reader<'_>) -> SerResult<Self> {
        r.u8()?;
        Ok(Self {
            type_: SystemInquiredType::from_u8(r.u8()?),
            detect_sensitivity: DetectSensitivity::from_u8(r.u8()?),
            mode_off_time: ModeOutTime::from_u8(r.u8()?),
        })
    }
}

// ---------------------------------------------------------------------------
// Table 2 (DataMdrNo2)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct T2ConnectGetSupportFunction;

impl Payload for T2ConnectGetSupportFunction {
    fn write(&self, w: &mut Writer) -> SerResult<()> {
        w.u8(CommandT2::ConnectGetSupportFunction.to_u8())?;
        w.u8(ConnectInquiredType::FixedValue.to_u8())
    }
    fn read(r: &mut Reader<'_>) -> SerResult<Self> {
        r.u8()?;
        r.u8()?;
        Ok(Self)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VoiceGuidanceGetParam {
    pub type_: VoiceGuidanceInquiredType,
}

impl Payload for VoiceGuidanceGetParam {
    fn write(&self, w: &mut Writer) -> SerResult<()> {
        w.u8(CommandT2::VoiceGuidanceGetParam.to_u8())?;
        w.u8(self.type_.to_u8())
    }
    fn read(r: &mut Reader<'_>) -> SerResult<Self> {
        r.u8()?;
        Ok(Self {
            type_: VoiceGuidanceInquiredType::from_u8(r.u8()?),
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VoiceGuidanceParamSettingMtk {
    pub type_: VoiceGuidanceInquiredType,
    pub setting_value: OnOffSetting,
}

impl Payload for VoiceGuidanceParamSettingMtk {
    fn write(&self, w: &mut Writer) -> SerResult<()> {
        w.u8(CommandT2::VoiceGuidanceSetParam.to_u8())?;
        w.u8(self.type_.to_u8())?;
        w.u8(self.setting_value.to_u8())
    }
    fn read(r: &mut Reader<'_>) -> SerResult<Self> {
        r.u8()?;
        Ok(Self {
            type_: VoiceGuidanceInquiredType::from_u8(r.u8()?),
            setting_value: OnOffSetting::from_u8(r.u8()?),
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VoiceGuidanceSetParamVolume {
    pub volume: i8,
    pub feedback_sound: OnOffSetting,
}

impl Payload for VoiceGuidanceSetParamVolume {
    fn write(&self, w: &mut Writer) -> SerResult<()> {
        w.u8(CommandT2::VoiceGuidanceSetParam.to_u8())?;
        w.u8(VoiceGuidanceInquiredType::Volume.to_u8())?;
        w.u8(self.volume as u8)?;
        w.u8(self.feedback_sound.to_u8())
    }
    fn read(r: &mut Reader<'_>) -> SerResult<Self> {
        r.u8()?;
        r.u8()?;
        Ok(Self {
            volume: r.u8()? as i8,
            feedback_sound: OnOffSetting::from_u8(r.u8()?),
        })
    }
}

/// Raw `LOG_SET_STATUS` command emitted at the end of init:
/// `[0xC4, 0x01, 0x00]` (see `RequestInitV2` in the reference client).
pub fn log_set_status_payload() -> Vec<u8> {
    vec![CommandT1::LogSetStatus.to_u8(), 0x01, 0x00]
}

/// Fixed length of the ASCII MAC address in peripheral payloads:
/// `"XX:XX:XX:XX:XX:XX"`, no NUL terminator (`kMacAddressLength`).
const MAC_LEN: usize = 17;

/// Pads/truncates a MAC address to the 17-byte wire format.
fn mac_bytes(address: &str) -> [u8; MAC_LEN] {
    let mut out = [b' '; MAC_LEN];
    for (i, b) in address.as_bytes().iter().take(MAC_LEN).enumerate() {
        out[i] = *b;
    }
    out
}

/// Reads a 17-byte wire MAC, tolerating NUL/space padding.
fn mac_string(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes)
        .trim_end_matches('\0')
        .trim()
        .to_string()
}

/// One device in the peripheral device management list
/// (`PeripheralDeviceInfo` / `PeripheralDeviceInfoWithBluetoothClassOfDevice`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PeripheralDeviceInfo {
    pub address: String,
    pub connected: bool,
    pub name: String,
    /// Bluetooth class of device (only present in the type-0x02 variant).
    pub class_of_device: Option<u32>,
}

impl PeripheralDeviceInfo {
    fn write_to(&self, w: &mut Writer, with_class: bool) -> SerResult<()> {
        w.bytes(&mac_bytes(&self.address))?;
        w.u8(self.connected as u8)?;
        if with_class {
            let class = self.class_of_device.unwrap_or(0xFF_FFFF);
            w.u8((class >> 16) as u8)?;
            w.u8((class >> 8) as u8)?;
            w.u8(class as u8)?;
        }
        w.prefixed_string(&self.name)
    }

    fn read_from(r: &mut Reader<'_>, with_class: bool) -> SerResult<Self> {
        let address = mac_string(r.take(MAC_LEN)?);
        let connected = r.u8()? != 0;
        let class_of_device = if with_class {
            let b = r.take(3)?;
            Some(u32::from_be_bytes([0, b[0], b[1], b[2]]))
        } else {
            None
        };
        Ok(Self {
            address,
            connected,
            name: read_prefixed_string(r)?,
            class_of_device,
        })
    }
}

/// `PERI_GET_PARAM` — request the paired/connected device list
/// (multipoint). The type selects the classic-BT or class-of-device variant.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PeripheralGetParam {
    pub type_: PeripheralInquiredType,
}

impl Payload for PeripheralGetParam {
    fn write(&self, w: &mut Writer) -> SerResult<()> {
        w.u8(CommandT2::PeriGetParam.to_u8())?;
        w.u8(self.type_.to_u8())
    }
    fn read(r: &mut Reader<'_>) -> SerResult<Self> {
        r.u8()?;
        Ok(Self {
            type_: PeripheralInquiredType::from_u8(r.u8()?),
        })
    }
}

/// `PERI_RET/NTFY_PARAM` — the device management list
/// (`PeripheralParamPairingDeviceManagement*`). Type 0x00 entries carry no
/// class-of-device field; type 0x02 entries carry a 3-byte BE one.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PeripheralRetParamDeviceList {
    pub type_: PeripheralInquiredType,
    pub devices: Vec<PeripheralDeviceInfo>,
    /// Index of the current playback (multipoint) device.
    pub playback_device: u8,
}

impl PeripheralRetParamDeviceList {
    fn with_class(&self) -> bool {
        self.type_ == PeripheralInquiredType::PairingDeviceManagementWithBluetoothClassOfDevice
    }
}

impl Payload for PeripheralRetParamDeviceList {
    fn write(&self, w: &mut Writer) -> SerResult<()> {
        w.u8(CommandT2::PeriRetParam.to_u8())?;
        w.u8(self.type_.to_u8())?;
        if self.devices.len() >= 256 {
            return Err(SerError::InvalidArgument);
        }
        w.u8(self.devices.len() as u8)?;
        let with_class = self.with_class();
        for d in &self.devices {
            d.write_to(w, with_class)?;
        }
        w.u8(self.playback_device)
    }
    fn read(r: &mut Reader<'_>) -> SerResult<Self> {
        r.u8()?;
        let type_ = PeripheralInquiredType::from_u8(r.u8()?);
        let with_class =
            type_ == PeripheralInquiredType::PairingDeviceManagementWithBluetoothClassOfDevice;
        let count = r.u8()? as usize;
        let mut devices = Vec::with_capacity(count);
        for _ in 0..count {
            devices.push(PeripheralDeviceInfo::read_from(r, with_class)?);
        }
        let playback_device = r.u8()?;
        Ok(Self {
            type_,
            devices,
            playback_device,
        })
    }
}

/// `PERI_SET_EXTENDED_PARAM` / SOURCE_SWITCH_CONTROL — switch the playback
/// device to `address`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PeripheralSetExtendedParamSourceSwitch {
    pub address: String,
}

impl Payload for PeripheralSetExtendedParamSourceSwitch {
    fn write(&self, w: &mut Writer) -> SerResult<()> {
        w.u8(CommandT2::PeriSetExtendedParam.to_u8())?;
        w.u8(PeripheralInquiredType::SourceSwitchControl.to_u8())?;
        w.bytes(&mac_bytes(&self.address))
    }
    fn read(r: &mut Reader<'_>) -> SerResult<Self> {
        r.u8()?;
        r.u8()?;
        Ok(Self {
            address: mac_string(r.take(MAC_LEN)?),
        })
    }
}

/// `PERI_SET_EXTENDED_PARAM` / device management — connect/disconnect/unpair
/// a paired device (`PeripheralSetExtendedParamParingDeviceManagementCommon`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PeripheralSetExtendedParamDeviceManagement {
    pub type_: PeripheralInquiredType,
    pub action: ConnectivityActionType,
    pub address: String,
}

impl Payload for PeripheralSetExtendedParamDeviceManagement {
    fn write(&self, w: &mut Writer) -> SerResult<()> {
        w.u8(CommandT2::PeriSetExtendedParam.to_u8())?;
        w.u8(self.type_.to_u8())?;
        w.u8(self.action.to_u8())?;
        w.bytes(&mac_bytes(&self.address))
    }
    fn read(r: &mut Reader<'_>) -> SerResult<Self> {
        r.u8()?;
        let type_ = PeripheralInquiredType::from_u8(r.u8()?);
        let action = ConnectivityActionType::from_u8(r.u8()?);
        Ok(Self {
            type_,
            action,
            address: mac_string(r.take(MAC_LEN)?),
        })
    }
}

/// `PERI_NTFY_EXTENDED_PARAM` / SOURCE_SWITCH_CONTROL — result of a playback
/// switch (`PeripheralNotifyExtendedParamSourceSwitchControl`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PeripheralNotifyExtendedParamSourceSwitch {
    pub result: SourceSwitchControlResult,
    pub address: String,
}

impl Payload for PeripheralNotifyExtendedParamSourceSwitch {
    fn write(&self, w: &mut Writer) -> SerResult<()> {
        w.u8(CommandT2::PeriNtfyExtendedParam.to_u8())?;
        w.u8(PeripheralInquiredType::SourceSwitchControl.to_u8())?;
        w.u8(self.result.to_u8())?;
        w.bytes(&mac_bytes(&self.address))
    }
    fn read(r: &mut Reader<'_>) -> SerResult<Self> {
        r.u8()?;
        r.u8()?;
        Ok(Self {
            result: SourceSwitchControlResult::from_u8(r.u8()?),
            address: mac_string(r.take(MAC_LEN)?),
        })
    }
}

/// `PERI_NTFY_EXTENDED_PARAM` / device management — result of a
/// connect/disconnect/unpair (`PeripheralNotifyExtendedParamParingDeviceManagementCommon`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PeripheralNotifyExtendedParamDeviceManagement {
    pub type_: PeripheralInquiredType,
    pub action: ConnectivityActionType,
    pub result: u8,
    pub address: String,
}

impl Payload for PeripheralNotifyExtendedParamDeviceManagement {
    fn write(&self, w: &mut Writer) -> SerResult<()> {
        w.u8(CommandT2::PeriNtfyExtendedParam.to_u8())?;
        w.u8(self.type_.to_u8())?;
        w.u8(self.action.to_u8())?;
        w.u8(self.result)?;
        w.bytes(&mac_bytes(&self.address))
    }
    fn read(r: &mut Reader<'_>) -> SerResult<Self> {
        r.u8()?;
        let type_ = PeripheralInquiredType::from_u8(r.u8()?);
        let action = ConnectivityActionType::from_u8(r.u8()?);
        let result = r.u8()?;
        Ok(Self {
            type_,
            action,
            result,
            address: mac_string(r.take(MAC_LEN)?),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn roundtrip<P: Payload + PartialEq + std::fmt::Debug>(p: &P) {
        let bytes = p.serialize();
        let back = P::deserialize(&bytes).expect("deserialize");
        assert_eq!(back, *p);
    }

    #[test]
    fn connect_payloads() {
        roundtrip(&ConnectGetProtocolInfo);
        roundtrip(&ConnectRetProtocolInfo {
            protocol_version: 0x00000001,
            support_table1: EnableDisable::Enable,
            support_table2: EnableDisable::Disable,
        });
        roundtrip(&ConnectGetDeviceInfo {
            device_info_type: DeviceInfoType::ModelName,
        });
        roundtrip(&DeviceInfoResponse::ModelName("WH-1000XM5".into()));
        roundtrip(&DeviceInfoResponse::FwVersion("2.0.5".into()));
        roundtrip(&DeviceInfoResponse::SeriesAndColor {
            series: ModelSeriesType::Premium,
            color: ModelColor::Black,
        });
        roundtrip(&ConnectRetSupportFunction {
            functions: vec![
                SupportFunction {
                    function: 0x23,
                    priority: 0,
                },
                SupportFunction {
                    function: 0x20,
                    priority: 1,
                },
            ],
        });
    }

    #[test]
    fn common_and_power_payloads() {
        roundtrip(&CommonGetStatus {
            type_: CommonInquiredType::AudioCodec,
        });
        roundtrip(&CommonStatusAudioCodec {
            audio_codec: AudioCodec::Ldac,
        });
        roundtrip(&PowerGetStatus {
            type_: PowerInquiredType::Battery,
        });
        roundtrip(&PowerRetStatusBattery {
            type_: PowerInquiredType::Battery,
            left: BatteryStatus {
                level: 87,
                charging: BatteryChargingStatus::NotCharging,
                threshold: 0,
            },
            right: BatteryStatus::default(),
            case_: BatteryStatus::default(),
        });
        roundtrip(&PowerRetStatusBattery {
            type_: PowerInquiredType::LeftRightBattery,
            left: BatteryStatus {
                level: 80,
                charging: BatteryChargingStatus::Charging,
                threshold: 0,
            },
            right: BatteryStatus {
                level: 79,
                charging: BatteryChargingStatus::Charging,
                threshold: 0,
            },
            case_: BatteryStatus::default(),
        });
        roundtrip(&PowerRetStatusBattery {
            type_: PowerInquiredType::LrBatteryWithThreshold,
            left: BatteryStatus {
                level: 80,
                charging: BatteryChargingStatus::Charging,
                threshold: 25,
            },
            right: BatteryStatus {
                level: 79,
                charging: BatteryChargingStatus::Charging,
                threshold: 25,
            },
            case_: BatteryStatus::default(),
        });
        roundtrip(&PowerRetStatusBattery {
            type_: PowerInquiredType::CradleBatteryWithThreshold,
            left: BatteryStatus::default(),
            right: BatteryStatus::default(),
            case_: BatteryStatus {
                level: 55,
                charging: BatteryChargingStatus::NotCharging,
                threshold: 10,
            },
        });
        roundtrip(&PowerGetParam {
            type_: PowerInquiredType::AutoPowerOff,
        });
        roundtrip(&PowerParamAutoPowerOff {
            type_: PowerInquiredType::AutoPowerOff,
            current: 0x11,
            last_select: 0x00,
        });
        roundtrip(&PowerSetStatusPowerOff);
    }

    #[test]
    fn eq_payloads() {
        roundtrip(&EqEbbGetStatus);
        roundtrip(&EqEbbGetParam);
        roundtrip(&EqEbbParamEq {
            preset_id: EqPresetId::Custom,
            bands: vec![4, 6, 10, 6, 4, 2, 0, 0, 0, 0],
        });
        roundtrip(&EqEbbParamEq {
            preset_id: EqPresetId::Rock,
            bands: vec![],
        });
    }

    #[test]
    fn ncasm_payloads() {
        roundtrip(&NcAsmGetParam {
            type_: NcAsmInquiredType::ModeNcAsmDualNcModeSwitchAndAsmSeamless,
        });
        roundtrip(&NcAsmParamModeNcDualModeSwitchAsmSeamless {
            base: NcAsmParamBase {
                type_: NcAsmInquiredType::ModeNcAsmDualNcModeSwitchAndAsmSeamless,
                value_change_status: ValueChangeStatus::Changed,
                nc_asm_total_effect: NcAsmOnOffValue::On,
            },
            nc_asm_mode: NcAsmMode::Nc,
            ambient_sound_mode: AmbientSoundMode::Normal,
            ambient_sound_level: 0,
        });
        roundtrip(&NcAsmParamModeNcDualModeSwitchAsmSeamlessNa {
            base: NcAsmParamBase {
                type_: NcAsmInquiredType::ModeNcAsmDualNcModeSwitchAndAsmSeamlessNa,
                value_change_status: ValueChangeStatus::Changed,
                nc_asm_total_effect: NcAsmOnOffValue::Off,
            },
            nc_asm_mode: NcAsmMode::Asm,
            ambient_sound_mode: AmbientSoundMode::Voice,
            ambient_sound_level: 12,
            noise_adaptive_on_off: NcAsmOnOffValue::On,
            noise_adaptive_sensitivity: NoiseAdaptiveSensitivity::Low,
        });
        roundtrip(&NcAsmParamAsmSeamless {
            base: NcAsmParamBase {
                type_: NcAsmInquiredType::AsmSeamless,
                value_change_status: ValueChangeStatus::Changed,
                nc_asm_total_effect: NcAsmOnOffValue::On,
            },
            ambient_sound_mode: AmbientSoundMode::Normal,
            ambient_sound_level: 20,
        });
        roundtrip(&NcAsmParamNcAmbToggle {
            type_: NcAsmInquiredType::NcAmbToggle,
            function: Function::NcAsm,
        });
    }

    #[test]
    fn alert_payloads() {
        roundtrip(&AlertSetStatusFixedMessage {
            status: EnableDisable::Enable,
        });
        roundtrip(&AlertNotifyParamFixedMessage {
            message_type: AlertMessageType::CautionForDisableTouchSensorPanel,
            action_type: AlertActionType::PositiveNegative,
        });
    }

    #[test]
    fn playback_payloads() {
        roundtrip(&GetPlayParam {
            type_: PlayInquiredType::MusicVolume,
        });
        roundtrip(&GetPlayStatus {
            type_: PlayInquiredType::PlaybackControlWithCallVolumeAdjustment,
        });
        roundtrip(&PlayStatusPlaybackController {
            type_: PlayInquiredType::PlaybackControlWithCallVolumeAdjustment,
            status: EnableDisable::Enable,
            playback_status: PlaybackStatus::Play,
            music_call_status: MusicCallStatus::Music,
        });
        roundtrip(&PlayStatusSetPlaybackController {
            status: EnableDisable::Enable,
            control: PlaybackControl::Play,
        });
        roundtrip(&PlayParamPlaybackControllerName {
            type_: PlayInquiredType::PlaybackControlWithCallVolumeAdjustment,
            names: vec![
                PlaybackName {
                    status: PlaybackNameStatus::Settled,
                    name: "Song".into(),
                },
                PlaybackName {
                    status: PlaybackNameStatus::Settled,
                    name: "Album".into(),
                },
                PlaybackName {
                    status: PlaybackNameStatus::Settled,
                    name: "Artist".into(),
                },
                PlaybackName {
                    status: PlaybackNameStatus::Nothing,
                    name: String::new(),
                },
            ],
        });
        roundtrip(&PlayParamPlaybackControllerVolume {
            type_: PlayInquiredType::MusicVolume,
            volume: 15,
        });
    }

    #[test]
    fn general_setting_payloads() {
        roundtrip(&GsGetCapability {
            type_: GsInquiredType::GeneralSetting1,
            display_language: DisplayLanguage::English,
        });
        roundtrip(&GsGetParam {
            type_: GsInquiredType::GeneralSetting2,
        });
        roundtrip(&GsParamBoolean {
            type_: GsInquiredType::GeneralSetting2,
            setting_value: GsSettingValue::On,
        });
    }

    #[test]
    fn audio_payloads() {
        roundtrip(&AudioGetCapability {
            type_: AudioInquiredType::Upscaling,
        });
        roundtrip(&AudioRetCapabilityUpscaling {
            upscaling_type: UpscalingType::DseeHx,
        });
        roundtrip(&AudioGetStatus {
            type_: AudioInquiredType::Upscaling,
        });
        roundtrip(&AudioStatusCommon {
            type_: AudioInquiredType::Upscaling,
            status: EnableDisable::Enable,
        });
        roundtrip(&AudioGetParam {
            type_: AudioInquiredType::BgmModeAndErrorCode,
        });
        roundtrip(&AudioParamUpscaling {
            type_: AudioInquiredType::Upscaling,
            setting_value: UpscalingTypeAutoOff::Auto,
        });
        roundtrip(&AudioParamConnection {
            type_: AudioInquiredType::ConnectionMode,
            setting_value: PriorMode::SoundQualityPrior,
        });
        roundtrip(&AudioParamBGMMode {
            type_: AudioInquiredType::BgmMode,
            on_off: EnableDisable::Enable,
            target_room_size: RoomSize::Middle,
        });
        roundtrip(&AudioParamUpmixCinema {
            type_: AudioInquiredType::UpmixCinema,
            on_off: EnableDisable::Disable,
        });
    }

    #[test]
    fn system_payloads() {
        roundtrip(&SystemGetParam {
            type_: SystemInquiredType::SmartTalkingModeType2,
        });
        roundtrip(&SystemParamCommon {
            type_: SystemInquiredType::PlaybackControlByWearing,
            setting_value: EnableDisable::Enable,
        });
        roundtrip(&SystemParamSmartTalking {
            type_: SystemInquiredType::SmartTalkingModeType2,
            on_off: EnableDisable::Disable,
            preview_mode_on_off: EnableDisable::Disable,
        });
        roundtrip(&SystemParamAssignableSettings {
            presets: vec![Preset::PlaybackControl, Preset::NoFunction],
        });
        roundtrip(&SystemGetExtParam {
            type_: SystemInquiredType::SmartTalkingModeType2,
        });
        roundtrip(&SystemExtParamSmartTalkingMode2 {
            type_: SystemInquiredType::SmartTalkingModeType2,
            detect_sensitivity: DetectSensitivity::Auto,
            mode_off_time: ModeOutTime::Mid,
        });
    }

    #[test]
    fn table2_payloads() {
        roundtrip(&T2ConnectGetSupportFunction);
        roundtrip(&VoiceGuidanceGetParam {
            type_: VoiceGuidanceInquiredType::Volume,
        });
        roundtrip(&VoiceGuidanceParamSettingMtk {
            type_: VoiceGuidanceInquiredType::MtkTransferWoDisconnectionSupportLanguageSwitch,
            setting_value: OnOffSetting::On,
        });
        roundtrip(&VoiceGuidanceSetParamVolume {
            volume: 1,
            feedback_sound: OnOffSetting::On,
        });
        roundtrip(&PeripheralGetParam {
            type_: PeripheralInquiredType::PairingDeviceManagementClassicBt,
        });
        roundtrip(&PeripheralRetParamDeviceList {
            type_: PeripheralInquiredType::PairingDeviceManagementClassicBt,
            devices: vec![
                PeripheralDeviceInfo {
                    address: "AA:BB:CC:DD:EE:FF".into(),
                    connected: true,
                    name: "My Phone".into(),
                    class_of_device: None,
                },
                PeripheralDeviceInfo {
                    address: "11:22:33:44:55:66".into(),
                    connected: false,
                    name: String::new(),
                    class_of_device: None,
                },
            ],
            playback_device: 0,
        });
        roundtrip(&PeripheralRetParamDeviceList {
            type_: PeripheralInquiredType::PairingDeviceManagementWithBluetoothClassOfDevice,
            devices: vec![PeripheralDeviceInfo {
                address: "AA:BB:CC:DD:EE:FF".into(),
                connected: true,
                name: "My Phone".into(),
                class_of_device: Some(0x5A020C),
            }],
            playback_device: 0,
        });
        roundtrip(&PeripheralSetExtendedParamSourceSwitch {
            address: "11:22:33:44:55:66".into(),
        });
        roundtrip(&PeripheralSetExtendedParamDeviceManagement {
            type_: PeripheralInquiredType::PairingDeviceManagementClassicBt,
            action: ConnectivityActionType::Connect,
            address: "11:22:33:44:55:66".into(),
        });
        roundtrip(&PeripheralNotifyExtendedParamSourceSwitch {
            result: SourceSwitchControlResult::Success,
            address: "11:22:33:44:55:66".into(),
        });
        roundtrip(&PeripheralNotifyExtendedParamDeviceManagement {
            type_: PeripheralInquiredType::PairingDeviceManagementClassicBt,
            action: ConnectivityActionType::Connect,
            result: 0x10,
            address: "11:22:33:44:55:66".into(),
        });
        assert_eq!(log_set_status_payload(), vec![0xC4, 0x01, 0x00]);
    }

    #[test]
    fn parses_real_device_list_frame() {
        // First ~2 devices of the actual WH-1000XM5 response captured on
        // hardware: [0x37, 0x02, 0x08, ...] with class-of-device entries.
        let mut payload = vec![
            0x37, 0x02, 0x02, // PERI_RET_PARAM, type 0x02, 2 devices
        ];
        payload.extend_from_slice(b"FC:70:2E:B6:5A:92");
        payload.extend_from_slice(&[0x01, 0x6C, 0x01, 0x04, 0x0D]);
        payload.extend_from_slice(b"cachyos-x8664");
        payload.extend_from_slice(b"A4:A4:90:72:30:7D");
        payload.extend_from_slice(&[0x02, 0x5A, 0x02, 0x0C, 0x05]);
        payload.extend_from_slice(b"Phone");
        payload.push(0x01); // playback device = index 1
        let list = PeripheralRetParamDeviceList::deserialize(&payload).expect("parses");
        assert_eq!(list.type_, PeripheralInquiredType::PairingDeviceManagementWithBluetoothClassOfDevice);
        assert_eq!(list.devices.len(), 2);
        assert_eq!(list.devices[0].address, "FC:70:2E:B6:5A:92");
        assert!(list.devices[0].connected);
        assert_eq!(list.devices[0].name, "cachyos-x8664");
        assert_eq!(list.devices[0].class_of_device, Some(0x6C0104));
        assert_eq!(list.devices[1].address, "A4:A4:90:72:30:7D");
        assert_eq!(list.devices[1].name, "Phone");
        assert_eq!(list.playback_device, 1);
    }

    #[test]
    fn peripheral_wire_layout_spot_checks() {
        // PERI_GET_PARAM: [0x36, type].
        assert_eq!(
            PeripheralGetParam {
                type_: PeripheralInquiredType::PairingDeviceManagementClassicBt,
            }
            .serialize(),
            vec![0x36, 0x00]
        );
        // Source switch: [0x3C, 0x01, 17-byte address].
        let mut expected = vec![0x3C, 0x01];
        expected.extend_from_slice(b"11:22:33:44:55:66");
        assert_eq!(
            PeripheralSetExtendedParamSourceSwitch {
                address: "11:22:33:44:55:66".into(),
            }
            .serialize(),
            expected
        );
        // Device management: [0x3C, type, action, 17-byte address].
        let mut expected = vec![0x3C, 0x00, 0x01];
        expected.extend_from_slice(b"11:22:33:44:55:66");
        assert_eq!(
            PeripheralSetExtendedParamDeviceManagement {
                type_: PeripheralInquiredType::PairingDeviceManagementClassicBt,
                action: ConnectivityActionType::Connect,
                address: "11:22:33:44:55:66".into(),
            }
            .serialize(),
            expected
        );
        // Source switch notify: [0x3D, 0x01, result, 17-byte address].
        let mut expected = vec![0x3D, 0x01, 0x00];
        expected.extend_from_slice(b"11:22:33:44:55:66");
        assert_eq!(
            PeripheralNotifyExtendedParamSourceSwitch {
                result: SourceSwitchControlResult::Success,
                address: "11:22:33:44:55:66".into(),
            }
            .serialize(),
            expected
        );
        // Class-of-device list entry: [mac(17), connected, class(3 BE), name].
        // Verified against the real device's `37 02 08 ...` response.
        let mut expected = vec![0x37, 0x02, 0x01];
        expected.extend_from_slice(b"11:22:33:44:55:66");
        expected.extend_from_slice(&[0x01, 0x5A, 0x02, 0x0C, 0x08]);
        expected.extend_from_slice(b"My Phone");
        expected.push(0x00);
        assert_eq!(
            PeripheralRetParamDeviceList {
                type_: PeripheralInquiredType::PairingDeviceManagementWithBluetoothClassOfDevice,
                devices: vec![PeripheralDeviceInfo {
                    address: "11:22:33:44:55:66".into(),
                    connected: true,
                    name: "My Phone".into(),
                    class_of_device: Some(0x5A020C),
                }],
                playback_device: 0,
            }
            .serialize(),
            expected
        );
    }

    #[test]
    fn wire_layout_spot_checks() {
        // Byte-exact checks against the C++ layouts.
        assert_eq!(PowerSetStatusPowerOff.serialize(), vec![0x24, 0x03, 0x01]);
        assert_eq!(
            NcAsmGetParam {
                type_: NcAsmInquiredType::AsmSeamless
            }
            .serialize(),
            vec![0x66, 0x22]
        );
        assert_eq!(
            ConnectGetDeviceInfo {
                device_info_type: DeviceInfoType::FwVersion
            }
            .serialize(),
            vec![0x04, 0x02]
        );
        assert_eq!(
            AlertSetStatusFixedMessage {
                status: EnableDisable::Enable
            }
            .serialize(),
            vec![0x94, 0x00, 0x00]
        );
        // Version 0x00000001, both tables supported.
        assert_eq!(
            ConnectRetProtocolInfo {
                protocol_version: 1,
                support_table1: EnableDisable::Enable,
                support_table2: EnableDisable::Enable,
            }
            .serialize(),
            vec![0x01, 0x00, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00]
        );
    }
}
