//! Sony MDR V2 protocol enums.
//!
//! Values and byte layouts mirror `libmdr/include/mdr/ProtocolV2.hpp` and
//! `libmdr/include/mdr/ProtocolV2T1.hpp` from the SonyHeadphonesClient project.

use core::fmt;

/// MDR packet data type (first byte of a packed command payload).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum DataType {
    Data = 0,
    Ack = 1,
    DataMcNo1 = 2,
    DataIcd = 9,
    DataEv = 10,
    DataMdr = 12,
    DataCommon = 13,
    DataMdrNo2 = 14,
    Shot = 16,
    ShotMcNo1 = 18,
    ShotIcd = 25,
    ShotEv = 26,
    ShotMdr = 28,
    ShotCommon = 29,
    ShotMdrNo2 = 30,
    LargeDataCommon = 45,
    Unknown = 0xff,
}

impl DataType {
    pub const fn from_u8(v: u8) -> Self {
        match v {
            0 => Self::Data,
            1 => Self::Ack,
            2 => Self::DataMcNo1,
            9 => Self::DataIcd,
            10 => Self::DataEv,
            12 => Self::DataMdr,
            13 => Self::DataCommon,
            14 => Self::DataMdrNo2,
            16 => Self::Shot,
            18 => Self::ShotMcNo1,
            25 => Self::ShotIcd,
            26 => Self::ShotEv,
            28 => Self::ShotMdr,
            29 => Self::ShotCommon,
            30 => Self::ShotMdrNo2,
            45 => Self::LargeDataCommon,
            _ => Self::Unknown,
        }
    }
}

macro_rules! u8_enum {
    ($(#[$doc:meta])* $name:ident { $($variant:ident = $value:expr,)* }) => {
        $(#[$doc])*
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
        #[repr(u8)]
        pub enum $name {
            $($variant = $value,)*
        }

        impl $name {
            pub const fn from_u8(v: u8) -> Self {
                match v {
                    $($value => Self::$variant,)*
                    _ => Self::Unknown,
                }
            }

            pub const fn to_u8(self) -> u8 {
                self as u8
            }
        }

        impl From<u8> for $name {
            fn from(v: u8) -> Self {
                Self::from_u8(v)
            }
        }

        impl Default for $name {
            fn default() -> Self {
                Self::Unknown
            }
        }
    };
}

macro_rules! u8_enum_no_unknown {
    ($(#[$doc:meta])* $name:ident { $($variant:ident = $value:expr,)* }) => {
        $(#[$doc])*
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
        #[repr(u8)]
        pub enum $name {
            $($variant = $value,)*
        }

        impl $name {
            pub const fn from_u8(v: u8) -> Self {
                match v {
                    $($value => Self::$variant,)*
                    _ => Self::Unknown,
                }
            }

            pub const fn to_u8(self) -> u8 {
                self as u8
            }
        }

        impl From<u8> for $name {
            fn from(v: u8) -> Self {
                Self::from_u8(v)
            }
        }

        impl Default for $name {
            fn default() -> Self {
                Self::Unknown
            }
        }
    };
}

u8_enum_no_unknown! {
    /// MDR V2 Table 1 command opcodes.
    CommandT1 {
        ConnectGetProtocolInfo = 0x00,
        ConnectRetProtocolInfo = 0x01,
        ConnectGetCapabilityInfo = 0x02,
        ConnectRetCapabilityInfo = 0x03,
        ConnectGetDeviceInfo = 0x04,
        ConnectRetDeviceInfo = 0x05,
        ConnectGetSupportFunction = 0x06,
        ConnectRetSupportFunction = 0x07,
        GetTest = 0x0F,
        CommonGetCapability = 0x10,
        CommonRetCapability = 0x11,
        CommonGetStatus = 0x12,
        CommonRetStatus = 0x13,
        CommonNtfyStatus = 0x15,
        CommonSetParam = 0x18,
        CommonNtfyParam = 0x19,
        PowerGetCapability = 0x20,
        PowerRetCapability = 0x21,
        PowerGetStatus = 0x22,
        PowerRetStatus = 0x23,
        PowerSetStatus = 0x24,
        PowerNtfyStatus = 0x25,
        PowerGetParam = 0x26,
        PowerRetParam = 0x27,
        PowerSetParam = 0x28,
        PowerNtfyParam = 0x29,
        UpdateGetCapability = 0x30,
        UpdateRetCapability = 0x31,
        UpdateGetStatus = 0x32,
        UpdateRetStatus = 0x33,
        UpdateSetStatus = 0x34,
        UpdateNtfyStatus = 0x35,
        UpdateGetParam = 0x36,
        UpdateRetParam = 0x37,
        UpdateSetParam = 0x38,
        UpdateNtfyParam = 0x39,
        LeaGetCapability = 0x40,
        LeaRetCapability = 0x41,
        LeaGetStatus = 0x42,
        LeaRetStatus = 0x43,
        LeaNtfyStatus = 0x45,
        LeaGetParam = 0x46,
        LeaRetParam = 0x47,
        LeaSetParam = 0x48,
        LeaNtfyParam = 0x49,
        EqEbbGetStatus = 0x52,
        EqEbbRetStatus = 0x53,
        EqEbbNtfyStatus = 0x55,
        EqEbbGetParam = 0x56,
        EqEbbRetParam = 0x57,
        EqEbbSetParam = 0x58,
        EqEbbNtfyParam = 0x59,
        EqEbbGetExtendedInfo = 0x5A,
        EqEbbRetExtendedInfo = 0x5B,
        NcAsmGetCapability = 0x60,
        NcAsmRetCapability = 0x61,
        NcAsmGetStatus = 0x62,
        NcAsmRetStatus = 0x63,
        NcAsmSetStatus = 0x64,
        NcAsmNtfyStatus = 0x65,
        NcAsmGetParam = 0x66,
        NcAsmRetParam = 0x67,
        NcAsmSetParam = 0x68,
        NcAsmNtfyParam = 0x69,
        SenseGetCapability = 0x70,
        SenseRetCapability = 0x71,
        SenseSetStatus = 0x74,
        SenseNtfyStatus = 0x75,
        SenseSetParam = 0x78,
        SenseNtfyParam = 0x79,
        SenseGetExtInfo = 0x7A,
        SenseRetExtInfo = 0x7B,
        OptGetCapability = 0x80,
        OptRetCapability = 0x81,
        OptGetStatus = 0x82,
        OptRetStatus = 0x83,
        OptSetStatus = 0x84,
        OptNtfyStatus = 0x85,
        OptGetParam = 0x86,
        OptRetParam = 0x87,
        OptSetParam = 0x88,
        OptNtfyParam = 0x89,
        AlertGetCapability = 0x90,
        AlertRetCapability = 0x91,
        AlertGetStatus = 0x92,
        AlertRetStatus = 0x93,
        AlertSetStatus = 0x94,
        AlertNtfyStatus = 0x95,
        AlertSetParam = 0x98,
        AlertNtfyParam = 0x99,
        PlayGetCapability = 0xA0,
        PlayRetCapability = 0xA1,
        PlayGetStatus = 0xA2,
        PlayRetStatus = 0xA3,
        PlaySetStatus = 0xA4,
        PlayNtfyStatus = 0xA5,
        PlayGetParam = 0xA6,
        PlayRetParam = 0xA7,
        PlaySetParam = 0xA8,
        PlayNtfyParam = 0xA9,
        SarAutoPlayGetCapability = 0xB0,
        SarAutoPlayRetCapability = 0xB1,
        SarAutoPlayGetStatus = 0xB2,
        SarAutoPlayRetStatus = 0xB3,
        SarAutoPlayNtfyStatus = 0xB5,
        SarAutoPlayGetParam = 0xB6,
        SarAutoPlayRetParam = 0xB7,
        SarAutoPlaySetParam = 0xB8,
        SarAutoPlayNtfyParam = 0xB9,
        LogSetStatus = 0xC4,
        LogNtfyParam = 0xC9,
        GeneralSettingGetCapability = 0xD0,
        GeneralSettingRetCapability = 0xD1,
        GeneralSettingGetStatus = 0xD2,
        GeneralSettingRetStatus = 0xD3,
        GeneralSettingNtfyStatus = 0xD5,
        GeneralSettingGetParam = 0xD6,
        GeneralSettingRetParam = 0xD7,
        GeneralSettingSetParam = 0xD8,
        GeneralSettingNtfyParam = 0xD9,
        AudioGetCapability = 0xE0,
        AudioRetCapability = 0xE1,
        AudioGetStatus = 0xE2,
        AudioRetStatus = 0xE3,
        AudioNtfyStatus = 0xE5,
        AudioGetParam = 0xE6,
        AudioRetParam = 0xE7,
        AudioSetParam = 0xE8,
        AudioNtfyParam = 0xE9,
        SystemGetCapability = 0xF0,
        SystemRetCapability = 0xF1,
        SystemGetStatus = 0xF2,
        SystemRetStatus = 0xF3,
        SystemSetStatus = 0xF4,
        SystemNtfyStatus = 0xF5,
        SystemGetParam = 0xF6,
        SystemRetParam = 0xF7,
        SystemSetParam = 0xF8,
        SystemNtfyParam = 0xF9,
        SystemGetExtParam = 0xFA,
        SystemRetExtParam = 0xFB,
        SystemSetExtParam = 0xFC,
        SystemNtfyExtParam = 0xFD,
        Unknown = 0xFF,
    }
}

u8_enum_no_unknown! {
    /// MDR V2 Table 2 command opcodes (subset used by this app).
    CommandT2 {
        ConnectRetProtocolInfo = 0x01,
        ConnectGetSupportFunction = 0x06,
        ConnectRetSupportFunction = 0x07,
        PeriSetExtendedParam = 0x68,
        PeriGetStatus = 0x62,
        PeriSetStatus = 0x64,
        PeriNtfyStatus = 0x65,
        PeriGetParam = 0x66,
        PeriRetParam = 0x67,
        PeriNtfyParam = 0x69,
        VoiceGuidanceGetParam = 0x36,
        VoiceGuidanceRetParam = 0x37,
        VoiceGuidanceSetParam = 0x38,
        VoiceGuidanceNtfyParam = 0x39,
        Unknown = 0xFF,
    }
}

u8_enum_no_unknown! {
    /// `MessageMdrV2EnableDisable` (shared by many payloads).
    EnableDisable {
        Enable = 0,
        Disable = 1,
        Unknown = 0xFF,
    }
}

u8_enum_no_unknown! {
    /// `MessageMdrV2OnOffSettingValue`.
    OnOffSetting {
        On = 0,
        Off = 1,
        Unknown = 0xFF,
    }
}

u8_enum! {
    /// `ConnectInquiredType`.
    ConnectInquiredType { FixedValue = 0, Unknown = 0xFF, }
}

u8_enum! {
    /// `DeviceInfoType`.
    DeviceInfoType {
        ModelName = 1,
        FwVersion = 2,
        SeriesAndColorInfo = 3,
        InstructionGuide = 4,
        Unknown = 0xFF,
    }
}

u8_enum! {
    /// `ModelColor`.
    ModelColor {
        Default = 0,
        Black = 1,
        White = 2,
        Silver = 3,
        Red = 4,
        Blue = 5,
        Pink = 6,
        Yellow = 7,
        Green = 8,
        Gray = 9,
        Gold = 10,
        Cream = 11,
        Orange = 12,
        Brown = 13,
        Violet = 14,
        Unknown = 0xFF,
    }
}

u8_enum! {
    /// `ModelSeriesType`.
    ModelSeriesType {
        NoSeries = 0,
        ExtraBass = 0x10,
        UltPowerSound = 0x11,
        Hear = 0x20,
        Premium = 0x30,
        Sports = 0x40,
        Casual = 0x50,
        LinkBuds = 0x60,
        Neckband = 0x70,
        Linkpod = 0x80,
        Gaming = 0x90,
        Unknown = 0xFF,
    }
}

u8_enum! {
    /// `CommonInquiredType`.
    CommonInquiredType {
        Concierge = 0x00,
        ConnectionStatus = 0x01,
        AudioCodec = 0x02,
        UpscalingEffect = 0x03,
        BleSetup = 0x04,
        ConnectionEstablishedTime = 0x05,
        DeviceSpecialMode = 0x06,
        SmartPhoneAndConnectedDeviceInformationForClassic = 0x07,
        TandemReconnectionRequest = 0x08,
        DisplayFwVersion = 0x09,
        Unknown = 0xFF,
    }
}

u8_enum! {
    /// `AudioCodec`.
    AudioCodec {
        Unsettled = 0x00,
        Sbc = 0x01,
        Aac = 0x02,
        Ldac = 0x10,
        AptX = 0x20,
        AptXHd = 0x21,
        Lc3 = 0x30,
        Other = 0xFF,
        Unknown = 0xFE,
    }
}

impl fmt::Display for AudioCodec {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        use AudioCodec::*;
        let s = match self {
            Unsettled => "<unsettled>",
            Sbc => "SBC",
            Aac => "AAC",
            Ldac => "LDAC",
            AptX => "aptX",
            AptXHd => "aptX HD",
            Lc3 => "LC3",
            Self::Unknown => "Unknown",
            Other => "Unknown",
        };
        f.write_str(s)
    }
}

u8_enum! {
    /// `PowerInquiredType`.
    PowerInquiredType {
        Battery = 0x00,
        LeftRightBattery = 0x01,
        CradleBattery = 0x02,
        PowerOff = 0x03,
        AutoPowerOff = 0x04,
        AutoPowerOffWearingDetection = 0x05,
        PowerSaveMode = 0x06,
        LinkControl = 0x07,
        BatteryWithThreshold = 0x08,
        LrBatteryWithThreshold = 0x09,
        CradleBatteryWithThreshold = 0x0A,
        BatterySafeMode = 0x0B,
        CaringCharge = 0x0C,
        BtStandby = 0x0D,
        Stamina = 0x0E,
        AutomaticTouchPanelBacklightTurnOff = 0x0F,
        Unknown = 0xFF,
    }
}

u8_enum! {
    /// `BatteryChargingStatus`.
    BatteryChargingStatus {
        NotCharging = 0,
        Charging = 1,
        UnknownStatus = 2,
        Charged = 3,
        Unknown = 0xFF,
    }
}

impl fmt::Display for BatteryChargingStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        use BatteryChargingStatus::*;
        let s = match self {
            Charging => "Charging",
            Charged => "Charged",
            NotCharging => "",
            _ => "Unknown",
        };
        f.write_str(s)
    }
}

u8_enum! {
    /// `PowerOffSettingValue`.
    PowerOffSettingValue {
        UserPowerOff = 0x01,
        FactoryPowerOff = 0xFF,
        Unknown = 0x00,
    }
}

u8_enum! {
    /// `AutoPowerOffElements`.
    AutoPowerOffElements {
        PowerOffIn5Min = 0x00,
        PowerOffIn30Min = 0x01,
        PowerOffIn60Min = 0x02,
        PowerOffIn180Min = 0x03,
        PowerOffIn15Min = 0x04,
        PowerOffDisable = 0x11,
        Unknown = 0xFF,
    }
}

impl AutoPowerOffElements {
    pub const ALL: [AutoPowerOffElements; 6] = [
        AutoPowerOffElements::PowerOffDisable,
        AutoPowerOffElements::PowerOffIn5Min,
        AutoPowerOffElements::PowerOffIn15Min,
        AutoPowerOffElements::PowerOffIn30Min,
        AutoPowerOffElements::PowerOffIn60Min,
        AutoPowerOffElements::PowerOffIn180Min,
    ];
}

impl fmt::Display for AutoPowerOffElements {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        use AutoPowerOffElements::*;
        let s = match self {
            PowerOffIn5Min => "5 minutes",
            PowerOffIn15Min => "15 minutes",
            PowerOffIn30Min => "30 minutes",
            PowerOffIn60Min => "1 hour",
            PowerOffIn180Min => "3 hours",
            PowerOffDisable => "Do not turn off",
            Self::Unknown => "Unknown",
        };
        f.write_str(s)
    }
}

u8_enum! {
    /// `EqEbbInquiredType`.
    EqEbbInquiredType {
        PresetEq = 0x00,
        Ebb = 0x01,
        PresetEqNonCustomizable = 0x02,
        PresetEqAndUltMode = 0x03,
        PresetEqAndErrorCode = 0x04,
        SoundEffect = 0x30,
        CustomEq = 0x31,
        TurnKeyEq = 0x32,
        Unknown = 0xFF,
    }
}

u8_enum! {
    /// `EqPresetId`.
    EqPresetId {
        Off = 0x00,
        Rock = 0x01,
        Pop = 0x02,
        Jazz = 0x03,
        Dance = 0x04,
        Edm = 0x05,
        RAndBHipHop = 0x06,
        Acoustic = 0x07,
        Bright = 0x10,
        Excited = 0x11,
        Mellow = 0x12,
        Relaxed = 0x13,
        Vocal = 0x14,
        Treble = 0x15,
        Bass = 0x16,
        Speech = 0x17,
        GamingEq = 0x20,
        Fps1 = 0x21,
        Fps2 = 0x22,
        Fps3 = 0x23,
        Heavy = 0x30,
        Clear = 0x31,
        Hard = 0x32,
        Soft = 0x33,
        Custom = 0xA0,
        UserSetting1 = 0xA1,
        UserSetting2 = 0xA2,
        UserSetting3 = 0xA3,
        UserSetting4 = 0xA4,
        UserSetting5 = 0xA5,
        Unspecified = 0xFF,
        Unknown = 0xFE,
    }
}

impl EqPresetId {
    /// Presets offered in the tray menu, in display order.
    pub const ALL: [EqPresetId; 22] = [
        EqPresetId::Off,
        EqPresetId::Rock,
        EqPresetId::Pop,
        EqPresetId::Jazz,
        EqPresetId::Dance,
        EqPresetId::Edm,
        EqPresetId::RAndBHipHop,
        EqPresetId::Acoustic,
        EqPresetId::Bright,
        EqPresetId::Excited,
        EqPresetId::Mellow,
        EqPresetId::Relaxed,
        EqPresetId::Vocal,
        EqPresetId::Treble,
        EqPresetId::Bass,
        EqPresetId::Speech,
        EqPresetId::Heavy,
        EqPresetId::Clear,
        EqPresetId::Hard,
        EqPresetId::Soft,
        EqPresetId::GamingEq,
        EqPresetId::Custom,
    ];
}

impl fmt::Display for EqPresetId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        use EqPresetId::*;
        let s = match self {
            Off => "Off",
            Rock => "Rock",
            Pop => "Pop",
            Jazz => "Jazz",
            Dance => "Dance",
            Edm => "EDM",
            RAndBHipHop => "R&B/Hip-Hop",
            Acoustic => "Acoustic",
            Bright => "Bright",
            Excited => "Excited",
            Mellow => "Mellow",
            Relaxed => "Relaxed",
            Vocal => "Vocal",
            Treble => "Treble",
            Bass => "Bass",
            Speech => "Speech",
            Heavy => "Heavy",
            Clear => "Clear",
            Hard => "Hard",
            Soft => "Soft",
            GamingEq => "Gaming",
            Fps1 => "FPS 1",
            Fps2 => "FPS 2",
            Fps3 => "FPS 3",
            Custom => "Custom",
            UserSetting1 => "User Setting 1",
            UserSetting2 => "User Setting 2",
            UserSetting3 => "User Setting 3",
            UserSetting4 => "User Setting 4",
            UserSetting5 => "User Setting 5",
            Self::Unknown => "Unknown",
            _ => "Unknown",
        };
        f.write_str(s)
    }
}

u8_enum! {
    /// `NcAsmInquiredType`.
    NcAsmInquiredType {
        NcOnOff = 0x1,
        NcOnOffAndAsmOnOff = 0x11,
        NcModeSwitchAndAsmOnOff = 0x12,
        NcOnOffAndAsmSeamless = 0x13,
        NcModeSwitchAndAsmSeamless = 0x14,
        ModeNcAsmAutoNcModeSwitchAndAsmSeamless = 0x15,
        ModeNcAsmDualSingleNcModeSwitchAndAsmSeamless = 0x16,
        ModeNcAsmDualNcModeSwitchAndAsmSeamless = 0x17,
        ModeNcNcssAsmDualNcModeSwitchAndAsmSeamless = 0x18,
        ModeNcAsmDualNcModeSwitchAndAsmSeamlessNa = 0x19,
        AsmOnOff = 0x21,
        AsmSeamless = 0x22,
        NcAmbToggle = 0x30,
        NcTestMode = 0x40,
        Unknown = 0xFF,
    }
}

u8_enum! {
    /// `ValueChangeStatus`.
    ValueChangeStatus {
        UnderChanging = 0,
        Changed = 1,
        Unknown = 0xFF,
    }
}

u8_enum! {
    /// `NcAsmOnOffValue`.
    NcAsmOnOffValue {
        Off = 0,
        On = 1,
        Unknown = 0xFF,
    }
}

u8_enum! {
    /// `NcAsmMode`.
    NcAsmMode {
        Nc = 0,
        Asm = 1,
        Unknown = 0xFF,
    }
}

impl fmt::Display for NcAsmMode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            NcAsmMode::Nc => "Noise Cancelling",
            NcAsmMode::Asm => "Ambient Sound",
            Self::Unknown => "Unknown",
        })
    }
}

u8_enum! {
    /// `AmbientSoundMode`.
    AmbientSoundMode {
        Normal = 0,
        Voice = 1,
        Unknown = 0xFF,
    }
}

u8_enum! {
    /// `NoiseAdaptiveSensitivity`.
    NoiseAdaptiveSensitivity {
        Standard = 0,
        High = 1,
        Low = 2,
        Unknown = 0xFF,
    }
}

impl fmt::Display for NoiseAdaptiveSensitivity {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            NoiseAdaptiveSensitivity::Standard => "Standard",
            NoiseAdaptiveSensitivity::High => "High",
            NoiseAdaptiveSensitivity::Low => "Low",
            Self::Unknown => "Unknown",
        })
    }
}

u8_enum! {
    /// `Function` (NC/ASM button function).
    Function {
        NoFunction = 0x00,
        NcAsmOff = 0x01,
        NcAsm = 0x02,
        NcOff = 0x03,
        AsmOff = 0x04,
        QuickAttention = 0x10,
        NcOptimizer = 0x11,
        PlayPause = 0x20,
        NextTrack = 0x21,
        PrevTrack = 0x22,
        VolumeUp = 0x23,
        VolumeDown = 0x24,
        VoiceRecognition = 0x30,
        GetYourNotification = 0x31,
        TalkToGoogleAssistant = 0x32,
        StopGoogleAssistant = 0x33,
        VoiceInputCancel = 0x34,
        TalkToTencentXiaowei = 0x35,
        CancelVoiceRecognition = 0x36,
        VoiceInputAmazonAlexa = 0x37,
        CancelAmazonAlexa = 0x38,
        CancelTencentXiaowei = 0x39,
        LaunchMlp = 0x40,
        TalkToYourMlp = 0x41,
        SptfOneTouch = 0x42,
        QuickAccess1 = 0x43,
        QuickAccess2 = 0x44,
        TalkToTencentXiaoweiCancel = 0x45,
        QMscOneTouch = 0x46,
        Teams = 0x47,
        TeamsVoiceSkills = 0x48,
        NcNcssAsmOff = 0x50,
        NcNcssAsm = 0x51,
        NcNcssOff = 0x52,
        NcssAsmOff = 0x53,
        NcNcss = 0x54,
        NcssAsm = 0x55,
        NcssOff = 0x56,
        AmbSetting = 0x57,
        StandardVoiceSound = 0x58,
        MicMute = 0x70,
        GameUp = 0x71,
        ChatUp = 0x72,
        Unknown = 0xFF,
    }
}

impl fmt::Display for Function {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        use Function::*;
        let s = match self {
            NoFunction => "No Function",
            NcAsmOff => "NC-ASM-OFF",
            NcAsm => "NC-ASM",
            NcOff => "NC-OFF",
            AsmOff => "ASM-OFF",
            Self::Unknown => "Unknown",
            _ => "Unknown",
        };
        f.write_str(s)
    }
}

u8_enum! {
    /// `AlertInquiredType` (subset).
    AlertInquiredType {
        FixedMessage = 0x00,
        VibratorAlertNotification = 0x01,
        FixedMessageWithLeftRightSelection = 0x02,
        VoiceAssistantAlertNotification = 0x03,
        AppBecomesForeground = 0x04,
        LeAudioAlertNotification = 0x05,
        FlexibleMessage = 0x06,
        Unknown = 0xFF,
    }
}

u8_enum! {
    /// `AlertMessageType` (subset surfaced to the UI).
    AlertMessageType {
        DisconnectCausedByConnectionModeChange = 0x00,
        DisconnectCausedByChangingKeyAssign = 0x01,
        NeedDisconnectionForUpdatingFirmware = 0x02,
        GoogleAssistantIsNowAvailable = 0x03,
        DisconnectCausedByChangingMultipoint = 0x07,
        BatteryConsumptionIncreaseDueToEqAndUpscaling = 0x08,
        CautionForDisableTouchSensorPanel = 0x0A,
        Unknown = 0xFF,
    }
}

u8_enum! {
    /// `AlertAction`.
    AlertAction {
        Negative = 0x00,
        Positive = 0x01,
        Unknown = 0xFF,
    }
}

u8_enum! {
    /// `PlayInquiredType`.
    PlayInquiredType {
        PlaybackControlWithCallVolumeAdjustment = 0x1,
        PlaybackControlWithCallVolumeAdjustmentAndFunctionChange = 0x2,
        PlaybackControlWithFunctionChange = 0x3,
        MusicVolume = 0x20,
        CallVolume = 0x21,
        MusicVolumeWithMute = 0x30,
        CallVolumeWithMute = 0x31,
        PlayMode = 0x40,
        Unknown = 0xFF,
    }
}

u8_enum! {
    /// `PlaybackStatus`.
    PlaybackStatus {
        Unsettled = 0x00,
        Play = 0x01,
        Pause = 0x02,
        Stop = 0x03,
        Unknown = 0xFF,
    }
}

u8_enum! {
    /// `MusicCallStatus`.
    MusicCallStatus {
        Music = 0x0,
        Call = 0x1,
        Unknown = 0xFF,
    }
}

u8_enum! {
    /// `PlaybackControl`.
    PlaybackControl {
        KeyOff = 0x00,
        Pause = 0x01,
        TrackUp = 0x02,
        TrackDown = 0x03,
        GroupUp = 0x04,
        GroupDown = 0x05,
        Stop = 0x06,
        Play = 0x07,
        FastForward = 0x08,
        FastRewind = 0x09,
        Unknown = 0xFF,
    }
}

u8_enum! {
    /// `PlaybackNameStatus`.
    PlaybackNameStatus {
        Unsettled = 0,
        Nothing = 1,
        Settled = 2,
        Unknown = 0xFF,
    }
}

u8_enum! {
    /// `GsInquiredType`.
    GsInquiredType {
        GeneralSetting1 = 0xD1,
        GeneralSetting2 = 0xD2,
        GeneralSetting3 = 0xD3,
        GeneralSetting4 = 0xD4,
        Unknown = 0xFF,
    }
}

u8_enum! {
    /// `GsSettingType`.
    GsSettingType {
        BooleanType = 0x00,
        ListType = 0x01,
        Unknown = 0xFF,
    }
}

u8_enum! {
    /// `GsSettingValue`.
    GsSettingValue {
        On = 0x00,
        Off = 0x01,
        Unknown = 0xFF,
    }
}

u8_enum! {
    /// `DisplayLanguage`.
    DisplayLanguage {
        UndefinedLanguage = 0x00,
        English = 0x01,
        French = 0x02,
        German = 0x03,
        Spanish = 0x04,
        Italian = 0x05,
        Portuguese = 0x06,
        Dutch = 0x07,
        Swedish = 0x08,
        Finnish = 0x09,
        Russian = 0x0A,
        Japanese = 0x0B,
        SimplifiedChinese = 0x0C,
        BrazilianPortuguese = 0x0D,
        TraditionalChinese = 0x0E,
        Korean = 0x0F,
        Turkish = 0x10,
        Unknown = 0xFF,
    }
}

u8_enum! {
    /// `GsStringFormat`.
    GsStringFormat {
        RawName = 0x00,
        EnumName = 0x01,
        Unknown = 0xFF,
    }
}

u8_enum! {
    /// `AudioInquiredType`.
    AudioInquiredType {
        ConnectionMode = 0x00,
        Upscaling = 0x01,
        ConnectionModeWithLdacStatus = 0x02,
        BgmMode = 0x03,
        UpmixCinema = 0x04,
        ConnectionModeClassicAudioLeAudio = 0x05,
        VoiceContents = 0x06,
        SoundLeakageReduction = 0x07,
        ListeningOptionAssignCustomizable = 0x08,
        BgmModeAndErrorCode = 0x09,
        UpmixSeries = 0x0A,
        Unknown = 0xFF,
    }
}

u8_enum! {
    /// `PriorMode`.
    PriorMode {
        SoundQualityPrior = 0x00,
        ConnectionQualityPrior = 0x01,
        LowLatencyPriorBeta = 0x02,
        Unknown = 0xFF,
    }
}

impl fmt::Display for PriorMode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        use PriorMode::*;
        let s = match self {
            SoundQualityPrior => "Sound Quality",
            ConnectionQualityPrior => "Connection Quality",
            LowLatencyPriorBeta => "Low Latency",
            Self::Unknown => "Unknown",
        };
        f.write_str(s)
    }
}

u8_enum! {
    /// `UpscalingTypeAutoOff`.
    UpscalingTypeAutoOff {
        Off = 0x00,
        Auto = 0x01,
        Unknown = 0xFF,
    }
}

u8_enum! {
    /// `UpscalingType`.
    UpscalingType {
        DseeHx = 0x00,
        Dsee = 0x01,
        DseeHxAi = 0x02,
        DseeUltimate = 0x03,
        Unknown = 0xFF,
    }
}

impl fmt::Display for UpscalingType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        use UpscalingType::*;
        let s = match self {
            DseeHx => "DSEE HX",
            Dsee => "DSEE",
            DseeHxAi => "DSEE HX AI",
            DseeUltimate => "DSEE ULTIMATE",
            Self::Unknown => "Unknown",
        };
        f.write_str(s)
    }
}

u8_enum! {
    /// `RoomSize`.
    RoomSize {
        Small = 0x00,
        Middle = 0x01,
        Large = 0x02,
        Unknown = 0xFF,
    }
}

impl fmt::Display for RoomSize {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        use RoomSize::*;
        let s = match self {
            Small => "My Room",
            Middle => "Living Room",
            Large => "Cafe",
            Self::Unknown => "Unknown",
        };
        f.write_str(s)
    }
}

u8_enum! {
    /// `SystemInquiredType` (subset).
    SystemInquiredType {
        Vibrator = 0x00,
        PlaybackControlByWearing = 0x01,
        SmartTalkingModeType1 = 0x02,
        AssignableSettings = 0x03,
        VoiceAssistantSettings = 0x04,
        VoiceAssistantWakeWord = 0x05,
        WearingStatusDetector = 0x06,
        EarpieceSelection = 0x07,
        CallSettings = 0x08,
        ResetSettings = 0x09,
        AutoVolume = 0x0A,
        FaceTapTestMode = 0x0B,
        SmartTalkingModeType2 = 0x0C,
        QuickAccess = 0x0D,
        AssignableSettingsWithLimitation = 0x0E,
        HeadGestureOnOff = 0x0F,
        HeadGestureTraining = 0x10,
        Unknown = 0xFF,
    }
}

u8_enum! {
    /// `Preset` (touch functions).
    Preset {
        AmbientSoundControl = 0x00,
        VolumeControl = 0x10,
        PlaybackControl = 0x20,
        TrackControl = 0x21,
        PlaybackControlVoiceAssistantLimitation = 0x22,
        VoiceRecognition = 0x30,
        GoogleAssist = 0x31,
        AmazonAlexa = 0x32,
        TencentXiaowei = 0x33,
        Ms = 0x34,
        AmbientSoundControlQuickAccess = 0x35,
        QuickAccess = 0x36,
        TencentXiaoweiQMsc = 0x37,
        Teams = 0x38,
        AmbientSoundControlMic = 0x45,
        ListeningModeQuickAccess = 0x46,
        ChatMix = 0x70,
        Custom1 = 0x71,
        Custom2 = 0x72,
        NoFunction = 0xFF,
        Unknown = 0xFE,
    }
}

impl fmt::Display for Preset {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        use Preset::*;
        let s = match self {
            AmbientSoundControl => "Ambient Sound Control",
            VolumeControl => "Volume Control",
            PlaybackControl | PlaybackControlVoiceAssistantLimitation => "Playback Control",
            TrackControl => "Track Control",
            VoiceRecognition => "Voice Recognition",
            GoogleAssist => "Google Assistant",
            AmazonAlexa => "Amazon Alexa",
            TencentXiaowei => "Tencent Xiaowei",
            AmbientSoundControlQuickAccess => "Ambient Sound Control",
            QuickAccess => "Quick Access",
            NoFunction => "No Function",
            Self::Unknown => "Unknown",
            _ => "Unknown",
        };
        f.write_str(s)
    }
}

u8_enum! {
    /// `DetectSensitivity` (Speak to Chat).
    DetectSensitivity {
        Auto = 0x00,
        High = 0x01,
        Low = 0x02,
        Unknown = 0xFF,
    }
}

impl fmt::Display for DetectSensitivity {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            DetectSensitivity::Auto => "Auto",
            DetectSensitivity::High => "High",
            DetectSensitivity::Low => "Low",
            Self::Unknown => "Unknown",
        })
    }
}

u8_enum! {
    /// `ModeOutTime`.
    ModeOutTime {
        Fast = 0x00,
        Mid = 0x01,
        Slow = 0x02,
        None = 0x03,
        Unknown = 0xFF,
    }
}

impl fmt::Display for ModeOutTime {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        use ModeOutTime::*;
        let s = match self {
            Fast => "Short (~5s)",
            Mid => "Standard (~15s)",
            Slow => "Long (~30s)",
            None => "Don't end automatically",
            Self::Unknown => "Unknown",
        };
        f.write_str(s)
    }
}

u8_enum! {
    /// `MessageMdrV2FunctionType_Table1` (support function table 1).
    FunctionTable1 {
        ConciergeData = 0x10,
        ConnectionStatus = 0x11,
        CodecIndicator = 0x12,
        UpscalingIndicator = 0x13,
        BleSetup = 0x14,
        TutorialContentsSelectOnConcierge = 0x15,
        ConnectionEstablishedTime = 0x16,
        UnnecessaryAutoReconnection = 0x17,
        DeviceSpecialMode = 0x18,
        PhoneAndConnectedDeviceInfomationForClassic = 0x19,
        TandemReconnectionRequest = 0x1A,
        DisplayFwVersion = 0x1B,
        BatteryLevelIndicator = 0x20,
        LeftRightBatteryLevelIndicator = 0x21,
        CradleBatteryLevelIndicator = 0x22,
        PowerOff = 0x23,
        AutoPowerOff = 0x24,
        AutoPowerOffWithWearingDetection = 0x25,
        PowerSavingModeOnOff = 0x26,
        TandemKeepAlive = 0x27,
        BatteryLevelWithThreshold = 0x28,
        LrBatteryLevelWithThreshold = 0x29,
        CradleBatteryLevelWithThreshold = 0x2A,
        BatterySafeMode = 0x2B,
        CaringCharge = 0x2C,
        BtStandby = 0x2D,
        Stamina = 0x2E,
        AutomaticTouchPanelBacklightTurnOff = 0x2F,
        FwUpdateMtkTransferWithoutDisconnection = 0x32,
        FwUpdateMtkTransferWithoutDisconnectionAutoUpdate = 0x34,
        FwUpdateMtkTransferWithRepairMode = 0x35,
        FwUpdateMtkTransferWithAcConnectionCheck = 0x36,
        FwUpdateTandemTransferUsingCommonTable = 0x37,
        FwUpdateUsingMcApp = 0x38,
        TwsSupportsA2dpLeaUniLeaBroadWithCtkd = 0x40,
        HbsSupportsA2dpLeaUniLeaBroadWithCtkd = 0x41,
        ClassicOnlyLeClassicSetting = 0x42,
        TwsSupportsLeaUniLeaBroad = 0x43,
        ChangeTandemConnectionProfileForAndroid = 0x44,
        BgmModeCantBeUsedWithLeaConnection = 0x45,
        HeadTrackerCantBeUsedWithLeaConnection = 0x46,
        PairingDeviceManagementCantBeUsedWithLeaConnection = 0x47,
        SoundArCantBeUsedWithLeaConnection = 0x48,
        AutoPlayCantBeUsedWithLeaConnection = 0x49,
        GattConnectableCantBeUsedWithLeaConnection = 0x4A,
        SoundArOptimizationCantBeUsedWithLeaConnection = 0x4B,
        QuickAccessCantBeUsedWithLeaConnection = 0x4C,
        ConnectionModeCantBeUsedWithLeaConnection = 0x4D,
        VoiceAssistantSettingsCantBeUsedWithLeaConnection = 0x4E,
        VoiceAssistantWakeWordCantBeUsedWithLeaConnection = 0x4F,
        PresetEq = 0x50,
        Ebb = 0x51,
        PresetEqNonCustomizable = 0x52,
        PresetEqAndUltMode = 0x53,
        SoundEffect = 0x54,
        CustomEq = 0x55,
        TurnKeyEq = 0x56,
        PresetEqAndErrorCode = 0x57,
        NoiseCancellingOnOff = 0x61,
        NoiseCancellingOnOffAndAmbientSoundModeOnOff = 0x62,
        NoiseCancellingDualSingleOffAndAmbientSoundModeOnOff = 0x63,
        NoiseCancellingOnOffAndAmbientSoundModeLevelAdjustment = 0x64,
        NoiseCancellingDualSingleOffAmbientSoundModeLevelAdjustment = 0x65,
        AmbientSoundModeOnOff = 0x66,
        AmbientSoundModeLevelAdjustment = 0x67,
        ModeNcAsmNoiseCancellingDualAutoAmbientSoundModeLevelAdjustment = 0x68,
        AmbientSoundControlModeSelect = 0x69,
        ModeNcAsmNoiseCancellingDualSingleAmbientSoundModeLevelAdjustment = 0x6A,
        ModeNcAsmNoiseCancellingDualAmbientSoundModeLevelAdjustment = 0x6B,
        ModeNcNcssAsmNoiseCancellingDualAmbientSoundModeLevelAdjustmentWithTestMode = 0x6C,
        ModeNcAsmNoiseCancellingDualAmbientSoundModeLevelAdjustmentNoiseAdaptation = 0x6D,
        AutoNcasm = 0x70,
        AdaptiveControlWithParameterNotification = 0x71,
        NcOptimizerPersonalBarometric = 0x80,
        NcOptimizerPersonal = 0x81,
        NcOptimizerBarometric = 0x82,
        SoundFieldOptimization = 0x83,
        TvSoundBooster = 0x84,
        FixedMessage = 0x90,
        VibratorAlertNotification = 0x91,
        FixedMessageWithLrSelection = 0x92,
        VoiceAssistantAlertNotification = 0x93,
        LeAudioAlertNotification = 0x94,
        PlaybackControllerWithCallVolumeAdjustment = 0xA1,
        PlaybackControllerWithCallVolumeAdjustmentAndMute = 0xA2,
        PlaybackControllerWithCallVolumeAdjustmentAndFunctionChange = 0xA3,
        PlaybackControllerWithFunctionChange = 0xA4,
        Sar = 0xB0,
        AutoPlay = 0xB1,
        GattConnectable = 0xB2,
        SarOptimizationCompassAccelType = 0xB3,
        HeadTrackerCompassAccelType = 0xB5,
        SarOptimizationAccelType = 0xB6,
        HeadTrackerAccelType = 0xB7,
        IntegratedAutoPlay = 0xB8,
        ActionLogNotifier = 0xC1,
        TimeSeriesOperationlogNotifier = 0xC2,
        SoundDropoutNotifier = 0xC3,
        GeneralSetting1 = 0xD1,
        GeneralSetting2 = 0xD2,
        GeneralSetting3 = 0xD3,
        GeneralSetting4 = 0xD4,
        ConnectionModeSoundQualityConnectionQuality = 0xE1,
        UpscalingAutoOff = 0xE2,
        ConnectionModeSoundQualitySoundWithLdacStatusQualityConnectionQuality = 0xE3,
        BgmModeSmallMiddleLarge = 0xE4,
        UpmixCinema = 0xE5,
        ListeningOption = 0xE6,
        ConnectionModeClassicAudioLeAudio = 0xE7,
        VoiceContents = 0xE8,
        SoundLeakageReduction = 0xE9,
        ListeningOptionAssignCustomizable = 0xEA,
        BgmModeSmallMiddleLargeAndErrorCode = 0xEB,
        UpmixSeries = 0xEC,
        VibratorOnOff = 0xF0,
        PlaybackControlByWearingRemovingHeadphoneOnOff = 0xF1,
        SmartTalkingModeType1 = 0xF2,
        AssignableSetting = 0xF3,
        VoiceAssistantSettings = 0xF4,
        VoiceAssistantWakeWordOnOff = 0xF5,
        WearingStatusDetector = 0xF6,
        EarpieceSelection = 0xF7,
        CallSettings = 0xF8,
        ResetSettings = 0xF9,
        AutoVolume = 0xFA,
        FaceTapTestMode = 0xFB,
        SmartTalkingModeType2 = 0xFC,
        QuickAccess = 0xFD,
        AssignableSettingWithLimitation = 0xFE,
        HeadGestureOnOffTraining = 0xFF,
        Unknown = 0x00,
    }
}

u8_enum! {
    /// `MessageMdrV2FunctionType_Table2` (support function table 2).
    FunctionTable2 {
        AutoStandby = 0x20,
        ChargeInUse = 0x21,
        CaringChargeWithThreshold = 0x22,
        UsbSubmersion = 0x23,
        PairingDeviceManagementClassicBt = 0x30,
        SourceSwitchControl = 0x31,
        PairingDeviceManagementWithBluetoothClassOfDeviceClassicBt = 0x32,
        PairingDeviceManagementWithBluetoothClassOfDeviceClassicLe = 0x33,
        MusicHandOverSetting = 0x34,
        VoiceGuidanceSettingMtkTransferWithoutDisconnectionNotSupportLanguageSwitch = 0x40,
        VoiceGuidanceSettingMtkTransferWithoutDisconnectionSupportLanguageSwitch = 0x41,
        VoiceGuidanceSettingMtkTransferWithoutDisconnectionSupportLanguageSwitchAndVolumeAdjustment = 0x42,
        VoiceGuidanceVolumeSettingMtkFixedTo5Steps = 0x43,
        VoiceGuidanceSettingSupportLanguageSwitch = 0x44,
        VoiceGuidanceSettingOnlyOnOffSwitch = 0x45,
        VoiceGuidanceBatteryLevelVoice = 0x46,
        VoiceGuidancePowerOnOffSound = 0x47,
        VoiceGuidanceSoundEffectUltBeepOnOff = 0x48,
        SafeListeningHbs1 = 0x50,
        SafeListeningTws1 = 0x51,
        SafeListeningHbs2 = 0x52,
        SafeListeningTws2 = 0x53,
        SafeVolumeControl = 0x54,
        LeAudioConnectionStateNotification = 0x60,
        LeAudioSwitchSupportedCompatibility = 0x61,
        LeAudioConnectionMode = 0x62,
        GetIdentityResolvingKey = 0x63,
        LinkAutoSwitchCantBeUsedWithLeaConnection = 0x6F,
        DjControl = 0x70,
        Illumination = 0x71,
        Karaoke = 0x72,
        WearingStatusChecker = 0xF0,
        RepeatTapTrainingMode = 0xF1,
        QuickAccessEasySetting = 0xF2,
        AutoVolumeOptimizer = 0xF3,
        AutoVolumeWithLimitation = 0xF4,
        SonyVoiceAssistant = 0xF5,
        WearingPosition = 0xF6,
        LinkAutoSwitchForSpeaker = 0xF7,
        LinkAutoSwitchForHeadsets = 0xF8,
        MicOnOffByHeadphoneOperation = 0xF9,
        FunctionChange = 0xFA,
        UsbBrowser = 0xFB,
        LightingMode = 0xFC,
        Unknown = 0x00,
    }
}

u8_enum! {
    /// `VoiceGuidanceInquiredType` (Table 2).
    VoiceGuidanceInquiredType {
        MtkTransferWoDisconnectionNotSupportLanguageSwitch = 0x00,
        MtkTransferWoDisconnectionSupportLanguageSwitch = 0x01,
        Volume = 0x02,
        VolumeSettingFixedTo5Steps = 0x03,
        SupportLanguageSwitch = 0x04,
        OnlyOnOffSetting = 0x05,
        BatteryLevelVoice = 0x06,
        PowerOnOffSound = 0x07,
        SoundEffectUltBeepOnOff = 0x08,
        Unknown = 0xFF,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn enum_roundtrips() {
        // from_u8/to_u8 must be idempotent, and exact for defined values.
        for v in 0..=255u8 {
            let t1 = FunctionTable1::from_u8(v);
            assert_eq!(FunctionTable1::from_u8(t1.to_u8()), t1, "t1 v={v}");
            let t2 = FunctionTable2::from_u8(v);
            assert_eq!(FunctionTable2::from_u8(t2.to_u8()), t2, "t2 v={v}");
            let c = CommandT1::from_u8(v);
            assert_eq!(CommandT1::from_u8(c.to_u8()), c, "cmd v={v}");
        }
        // Spot-check exact mappings.
        assert_eq!(FunctionTable1::from_u8(0x23), FunctionTable1::PowerOff);
        assert_eq!(FunctionTable1::PowerOff.to_u8(), 0x23);
        assert_eq!(CommandT1::from_u8(0x68), CommandT1::NcAsmSetParam);
        assert_eq!(NcAsmInquiredType::from_u8(0x17).to_u8(), 0x17);
        assert_eq!(NcAsmInquiredType::from_u8(0x7F), NcAsmInquiredType::Unknown);
    }

    #[test]
    fn display_helpers() {
        assert_eq!(AudioCodec::Ldac.to_string(), "LDAC");
        assert_eq!(
            AutoPowerOffElements::PowerOffDisable.to_string(),
            "Do not turn off"
        );
        assert_eq!(EqPresetId::RAndBHipHop.to_string(), "R&B/Hip-Hop");
        assert_eq!(NcAsmMode::Nc.to_string(), "Noise Cancelling");
        assert_eq!(DetectSensitivity::Auto.to_string(), "Auto");
    }

    #[test]
    fn fallback_unknown() {
        assert_eq!(NcAsmMode::from_u8(0x7F), NcAsmMode::Unknown);
        assert_eq!(CommandT1::from_u8(0x7F), CommandT1::Unknown);
        assert_eq!(EqPresetId::from_u8(0x7F), EqPresetId::Unknown);
    }
}
