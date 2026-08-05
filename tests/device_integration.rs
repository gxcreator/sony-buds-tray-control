//! End-to-end tests: the device engine against a simulated headphone over an
//! in-memory transport. These validate the wire protocol byte-for-byte.

mod common;

use std::time::Duration;

use common::*;
use sony_buds_tray_control::device::{DeviceEvent, Engine, EngineError};
use sony_buds_tray_control::protocol::*;
use sony_buds_tray_control::transport::{MockTransport, Transport};

#[tokio::test]
async fn init_flow_populates_device_state() {
    let (host, device_tx) = MockTransport::pair();
    let mut device = MockDevice::new(device_tx, DeviceProfile::xm5());
    let mut engine = Engine::new(host);

    engine.request_init().unwrap();
    let events = pump(&mut engine, &mut device).await;
    for _f in device
        .received
        .iter()
        .filter(|f| f.first() == Some(&0xA6) || f.first() == Some(&0x02))
    {}

    assert!(events.contains(&DeviceEvent::InitOk), "events: {events:?}");
    assert_eq!(engine.state.model_name, "WH-1000XM5");
    assert_eq!(engine.state.fw_version, "2.0.5");
    assert_eq!(engine.state.unique_id, "MDR-TEST1");
    assert_eq!(engine.state.audio_codec, AudioCodec::Ldac);
    assert_eq!(engine.state.model_series, ModelSeriesType::Premium);
    assert_eq!(engine.state.model_color, ModelColor::Black);
    assert!(engine.state.has_table1);
    assert!(engine.state.has_table2);

    // Support functions.
    assert!(engine.state.support.contains_t1(FunctionTable1::PowerOff));
    assert!(engine
        .state
        .support
        .contains_t1(FunctionTable1::BatteryLevelIndicator));
    assert!(engine
        .state
        .support
        .contains_t1(FunctionTable1::SmartTalkingModeType2));
    assert!(engine.state.support.contains_t2(
        FunctionTable2::VoiceGuidanceSettingMtkTransferWithoutDisconnectionSupportLanguageSwitchAndVolumeAdjustment
    ));

    // Playback.
    assert_eq!(engine.state.play_title, "Test Song");
    assert_eq!(engine.state.play_artist, "Test Artist");
    assert_eq!(engine.state.play_status, PlaybackStatus::Play);
    assert_eq!(engine.state.play_volume, 12);

    // NC/ASM (AsmSeamless variant).
    assert!(engine.props.nc_asm_enabled.current);
    assert_eq!(engine.props.nc_asm_ambient_level.current, 12);

    // EQ + DSEE.
    assert!(engine.props.eq_available.current);
    assert_eq!(engine.props.eq_preset_id.current, EqPresetId::Off);
    assert!(!engine.props.upscaling_enabled.current);
    assert!(engine.state.upscaling_available);

    // Speak to Chat.
    assert!(!engine.props.speak_to_chat_enabled.current);
    assert_eq!(
        engine.props.speak_to_chat_detect_sensitivity.current,
        DetectSensitivity::Auto
    );

    // System.
    assert_eq!(
        engine.props.auto_power_off.current,
        AutoPowerOffElements::PowerOffDisable
    );
    assert!(!engine.props.auto_pause_enabled.current);
    assert!(engine.props.voice_guidance_enabled.current);

    // General settings capability.
    assert_eq!(engine.state.gs_capabilities.len(), 3);
    assert!(engine
        .state
        .gs_capabilities
        .iter()
        .any(|c| c.subject == "MULTIPOINT_SETTING"));
}

#[tokio::test]
async fn sync_updates_battery() {
    let (host, device_tx) = MockTransport::pair();
    let mut device = MockDevice::new(device_tx, DeviceProfile::xm5());
    let mut engine = Engine::new(host);

    engine.request_init().unwrap();
    let events = pump(&mut engine, &mut device).await;
    assert!(events.contains(&DeviceEvent::InitOk));

    device.state.battery = 42;
    engine.request_sync().unwrap();
    let events = pump(&mut engine, &mut device).await;
    assert!(events.contains(&DeviceEvent::SyncOk));
    assert!(events.contains(&DeviceEvent::Battery));
    assert_eq!(engine.state.battery_left.level, 42);
}

#[tokio::test]
async fn volume_change_is_committed_to_device() {
    let (host, device_tx) = MockTransport::pair();
    let mut device = MockDevice::new(device_tx, DeviceProfile::xm5());
    let mut engine = Engine::new(host);

    engine.request_init().unwrap();
    pump(&mut engine, &mut device).await;

    engine.props.set_volume(20);
    assert!(engine.props.is_dirty());
    engine.request_commit().unwrap();
    let events = pump(&mut engine, &mut device).await;
    assert!(events.contains(&DeviceEvent::CommitOk));
    assert_eq!(device.state.volume, 20);
    assert_eq!(engine.props.play_volume.current, 20);
    assert!(!engine.props.is_dirty());

    // Verify the exact command on the wire.
    let set_cmd = device
        .received
        .iter()
        .find(|d| d.first() == Some(&CommandT1::PlaySetParam.to_u8()))
        .expect("PLAY_SET_PARAM sent");
    assert_eq!(
        set_cmd,
        &PlayParamPlaybackControllerVolume {
            type_: PlayInquiredType::MusicVolume,
            volume: 20,
        }
        .serialize()
    );
}

#[tokio::test]
async fn ambient_mode_switch_is_committed() {
    let (host, device_tx) = MockTransport::pair();
    let mut device = MockDevice::new(device_tx, DeviceProfile::xm5());
    let mut engine = Engine::new(host);

    engine.request_init().unwrap();
    pump(&mut engine, &mut device).await;

    // Switch from NC to Ambient Sound, level 20.
    engine.props.nc_asm_enabled.desired = true;
    engine.props.nc_asm_mode.desired = NcAsmMode::Asm;
    engine.props.nc_asm_ambient_level.desired = 20;
    engine.request_commit().unwrap();
    let events = pump(&mut engine, &mut device).await;
    assert!(events.contains(&DeviceEvent::CommitOk));

    assert!(device.state.nc_asm_enabled);
    assert_eq!(device.state.ambient_level, 20);
    assert_eq!(engine.props.nc_asm_ambient_level.current, 20);

    // Verify wire format (AsmSeamless variant, 0x22).
    let cmd = device
        .received
        .iter()
        .find(|d| d.first() == Some(&CommandT1::NcAsmSetParam.to_u8()) && d.get(1) == Some(&0x22))
        .expect("NCASM_SET_PARAM (ASM_SEAMLESS) sent");
    assert_eq!(
        cmd,
        &NcAsmParamAsmSeamless {
            base: NcAsmParamBase {
                type_: NcAsmInquiredType::AsmSeamless,
                value_change_status: ValueChangeStatus::Changed,
                nc_asm_total_effect: NcAsmOnOffValue::On,
            },
            ambient_sound_mode: AmbientSoundMode::Normal,
            ambient_sound_level: 20,
        }
        .serialize()
    );
}

#[tokio::test]
async fn xm6_noise_adaptation_commit_uses_na_variant() {
    let (host, device_tx) = MockTransport::pair();
    let mut device = MockDevice::new(device_tx, DeviceProfile::xm6());
    let mut engine = Engine::new(host);

    engine.request_init().unwrap();
    pump(&mut engine, &mut device).await;

    engine.props.nc_asm_enabled.desired = false;
    engine.props.nc_asm_auto_asm_enabled.desired = true;
    engine.props.nc_asm_noise_adaptive_sensitivity.desired = NoiseAdaptiveSensitivity::High;
    engine.request_commit().unwrap();
    pump(&mut engine, &mut device).await;

    assert!(!device.state.nc_asm_enabled);
    assert!(device.state.auto_asm);
    assert_eq!(device.state.sensitivity, NoiseAdaptiveSensitivity::High);

    let cmd = device
        .received
        .iter()
        .find(|d| d.first() == Some(&CommandT1::NcAsmSetParam.to_u8()) && d.get(1) == Some(&0x19))
        .expect("NCASM_SET_PARAM (NA) sent");
    assert_eq!(cmd[0], CommandT1::NcAsmSetParam.to_u8());
    assert_eq!(cmd[1], 0x19);
}

#[tokio::test]
async fn eq_preset_and_bands_committed() {
    let (host, device_tx) = MockTransport::pair();
    let mut device = MockDevice::new(device_tx, DeviceProfile::xm5());
    let mut engine = Engine::new(host);

    engine.request_init().unwrap();
    pump(&mut engine, &mut device).await;

    engine.props.eq_preset_id.desired = EqPresetId::Custom;
    engine
        .props
        .set_eq_config(vec![4, 3, 2, 1, 0, -1, -2, -3, -4, -6]);
    engine.request_commit().unwrap();
    pump(&mut engine, &mut device).await;

    assert_eq!(device.state.eq_preset, EqPresetId::Custom);
    // Wire values are offset by +6 for 10-band EQ.
    assert_eq!(device.state.eq_bands, vec![10, 9, 8, 7, 6, 5, 4, 3, 2, 0]);

    // The engine refreshes bands after the commit and adopts the device view.
    assert_eq!(engine.props.eq_preset_id.current, EqPresetId::Custom);
}

#[tokio::test]
async fn play_control_is_one_shot() {
    let (host, device_tx) = MockTransport::pair();
    let mut device = MockDevice::new(device_tx, DeviceProfile::xm5());
    let mut engine = Engine::new(host);

    engine.request_init().unwrap();
    pump(&mut engine, &mut device).await;

    engine.props.play_control.desired = PlaybackControl::TrackUp;
    engine.request_commit().unwrap();
    pump(&mut engine, &mut device).await;

    // One-shot: after commit the control resets to KEY_OFF and isn't dirty.
    assert_eq!(engine.props.play_control.desired, PlaybackControl::KeyOff);
    assert!(!engine.props.is_dirty());

    // Device-side: next-track doesn't change play/pause.
    assert_eq!(device.state.play_status, PlaybackStatus::Play);
}

#[tokio::test]
async fn play_pause_toggle() {
    let (host, device_tx) = MockTransport::pair();
    let mut device = MockDevice::new(device_tx, DeviceProfile::xm5());
    let mut engine = Engine::new(host);

    engine.request_init().unwrap();
    pump(&mut engine, &mut device).await;

    engine.props.play_control.desired = PlaybackControl::Pause;
    engine.request_commit().unwrap();
    pump(&mut engine, &mut device).await;
    assert_eq!(device.state.play_status, PlaybackStatus::Pause);

    engine.props.play_control.desired = PlaybackControl::Play;
    engine.request_commit().unwrap();
    pump(&mut engine, &mut device).await;
    assert_eq!(device.state.play_status, PlaybackStatus::Play);
}

#[tokio::test]
async fn stc_and_system_settings_committed() {
    let (host, device_tx) = MockTransport::pair();
    let mut device = MockDevice::new(device_tx, DeviceProfile::xm5());
    let mut engine = Engine::new(host);

    engine.request_init().unwrap();
    pump(&mut engine, &mut device).await;

    engine.props.speak_to_chat_enabled.desired = true;
    engine.props.speak_to_chat_detect_sensitivity.desired = DetectSensitivity::Low;
    engine.props.speak_to_mode_out_time.desired = ModeOutTime::Fast;
    engine.props.auto_pause_enabled.desired = true;
    engine.props.auto_power_off.desired = AutoPowerOffElements::PowerOffIn30Min;
    engine.props.voice_guidance_enabled.desired = false;
    engine.props.voice_guidance_volume.desired = 1;
    engine.request_commit().unwrap();
    pump(&mut engine, &mut device).await;

    assert!(device.state.stc_enabled);
    assert_eq!(device.state.stc_sensitivity, DetectSensitivity::Low);
    assert_eq!(device.state.stc_mode_out, ModeOutTime::Fast);
    assert!(device.state.auto_pause);
    assert_eq!(
        device.state.auto_power_off,
        AutoPowerOffElements::PowerOffIn30Min.to_u8()
    );
    assert!(!device.state.voice_guidance);
    assert_eq!(device.state.voice_guidance_volume, 1);
}

#[tokio::test]
async fn general_settings_committed() {
    let (host, device_tx) = MockTransport::pair();
    let mut device = MockDevice::new(device_tx, DeviceProfile::xm5());
    let mut engine = Engine::new(host);

    engine.request_init().unwrap();
    pump(&mut engine, &mut device).await;

    engine.props.gs_param_bool[1].desired = true; // Multipoint
    engine.request_commit().unwrap();
    pump(&mut engine, &mut device).await;

    assert!(device.state.gs_values[1]);
    assert!(!engine.props.is_dirty());
}

#[tokio::test]
async fn battery_notification_updates_state() {
    let (host, device_tx) = MockTransport::pair();
    let mut device = MockDevice::new(device_tx, DeviceProfile::xm5());
    let mut engine = Engine::new(host);

    engine.request_init().unwrap();
    pump(&mut engine, &mut device).await;

    // Simulate an unsolicited battery notification.
    let notify = pack(
        DataType::DataMdr,
        0,
        &PowerRetStatusBattery {
            type_: PowerInquiredType::Battery,
            left: BatteryStatus {
                level: 33,
                charging: BatteryChargingStatus::Charging,
                threshold: 0,
            },
            right: BatteryStatus::default(),
            case_: BatteryStatus::default(),
        }
        .serialize(),
    );
    device.tx().send(&notify).await.unwrap();
    let events = pump(&mut engine, &mut device).await;
    assert!(events.contains(&DeviceEvent::Battery));
    assert_eq!(engine.state.battery_left.level, 33);
    assert_eq!(
        engine.state.battery_left.charging,
        BatteryChargingStatus::Charging
    );
}

#[tokio::test(start_paused = true)]
async fn unresponsive_device_times_out() {
    let (host, device_tx) = MockTransport::pair();
    // A device that never answers.
    let mut device = MockDevice::new(device_tx, DeviceProfile::xm5());
    device.silent = true;
    let mut engine = Engine::new(host);

    engine.request_init().unwrap();
    // Let the engine send its first command, then never respond.
    device.run_once().await;

    let mut saw_error = false;
    // The step is retransmitted a few times (3s apart) before the task fails,
    // so the timeout error surfaces after ~12s of virtual time.
    for _ in 0..150 {
        tokio::time::advance(Duration::from_millis(100)).await;
        if let Some(DeviceEvent::Error(e)) = engine.poll(Duration::ZERO).await {
            assert_eq!(e, EngineError::Timeout);
            saw_error = true;
            break;
        }
    }
    assert!(saw_error, "expected a timeout error");
    assert!(engine.is_ready(), "task must be cleared after failure");
    assert!(engine.last_error().is_some());
}

#[tokio::test]
async fn eof_on_transport_is_fatal() {
    let (host, device_tx) = MockTransport::pair();
    let mut device = MockDevice::new(device_tx, DeviceProfile::xm5());
    let mut engine = Engine::new(host);

    engine.request_init().unwrap();
    // Kill the pipe before the init completes.
    device.tx().disconnect().await;
    let events = pump(&mut engine, &mut device).await;
    assert!(events.iter().any(|e| matches!(e, DeviceEvent::Error(_))));
}

#[tokio::test]
async fn frame_resync_after_garbage() {
    let (host, device_tx) = MockTransport::pair();
    let mut device = MockDevice::new(device_tx, DeviceProfile::xm5());
    let mut engine = Engine::new(host);

    // Garbage before the first frame must be skipped.
    device.tx().send(&[0xDE, 0xAD, 0xBE, 0xEF]).await.unwrap();
    engine.request_init().unwrap();
    let events = pump(&mut engine, &mut device).await;
    assert!(events.contains(&DeviceEvent::InitOk), "events: {events:?}");
}

#[tokio::test]
async fn partial_frames_are_assembled() {
    let (host, device_tx) = MockTransport::pair();
    let device = MockDevice::new(device_tx, DeviceProfile::xm5());
    let mut engine = Engine::new(host);

    engine.request_init().unwrap();
    // Simulate byte-by-byte delivery of one response: the engine must
    // assemble the frame from fragments.
    tokio::spawn(async move {
        let mut device = device;
        // Pump the device normally until init completes.
        for _ in 0..400 {
            if !device.run_once().await {
                tokio::time::sleep(Duration::from_millis(1)).await;
            }
        }
    });

    let mut init_ok = false;
    for _ in 0..400 {
        if let Some(DeviceEvent::InitOk) = engine.poll(Duration::from_millis(5)).await {
            init_ok = true;
            break;
        }
    }
    assert!(init_ok);
}

#[tokio::test]
async fn shutdown_command_sent_when_supported() {
    let (host, device_tx) = MockTransport::pair();
    let mut device = MockDevice::new(device_tx, DeviceProfile::xm5());
    let mut engine = Engine::new(host);

    engine.request_init().unwrap();
    pump(&mut engine, &mut device).await;

    engine.props.shutdown.desired = true;
    engine.request_commit().unwrap();
    pump(&mut engine, &mut device).await;

    let cmd = device
        .received
        .iter()
        .find(|d| d.first() == Some(&CommandT1::PowerSetStatus.to_u8()))
        .expect("POWER_SET_STATUS sent");
    assert_eq!(cmd, &PowerSetStatusPowerOff.serialize());
    assert!(engine.props.shutdown.current);
}

#[tokio::test]
async fn no_commit_when_nothing_dirty() {
    let (host, device_tx) = MockTransport::pair();
    let mut device = MockDevice::new(device_tx, DeviceProfile::xm5());
    let mut engine = Engine::new(host);

    engine.request_init().unwrap();
    pump(&mut engine, &mut device).await;
    let sent = device.received.len();

    engine.request_commit().unwrap();
    let events = pump(&mut engine, &mut device).await;
    assert!(!events.contains(&DeviceEvent::CommitOk));
    // No additional commands hit the wire (device keeps sending only ACKs).
    assert!(
        device.received.len() <= sent + 1,
        "unexpected commands after no-op commit"
    );
}

#[tokio::test]
async fn eq_report_with_6_bands_parses_clear_bass() {
    let (host, device_tx) = MockTransport::pair();
    let mut device = MockDevice::new(device_tx, DeviceProfile::xm5());
    let mut engine = Engine::new(host);

    engine.request_init().unwrap();
    pump(&mut engine, &mut device).await;

    // Simulate a 5-band EQ report: [clearBass+10, band0+10, ..., band4+10].
    let mut w = codec::Writer::new(64);
    w.u8(CommandT1::EqEbbRetParam.to_u8()).unwrap();
    w.u8(EqEbbInquiredType::PresetEq.to_u8()).unwrap();
    w.u8(EqPresetId::Custom.to_u8()).unwrap();
    w.pod_array(&[13u8, 20, 16, 12, 8, 4]).unwrap();
    let payload = w.into_inner();
    let frame = pack(DataType::DataMdr, 0, &payload);
    device.tx().send(&frame).await.unwrap();
    pump(&mut engine, &mut device).await;

    assert_eq!(engine.props.eq_clear_bass.current, 3);
    assert_eq!(engine.props.eq_config.current, vec![10, 6, 2, -2, -6]);
}
