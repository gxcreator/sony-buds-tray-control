//! End-to-end tests at the application level: AppCore drives a simulated
//! headphone through the full connect → init → user command → commit flow,
//! and the tray menu model reflects the device state.

mod common;

use std::sync::{Arc, Mutex};

use common::{MockDevice, PairFactory};
use sony_buds_tray_control::app::{AmbientSel, AppCore, ConnState, ItemKind, MenuItem, UiCommand};
use sony_buds_tray_control::protocol::{DetectSensitivity, EqPresetId, ModeOutTime, NcAsmMode};
use sony_buds_tray_control::transport::discovery::{DeviceInfo, StaticDeviceLister};
use sony_buds_tray_control::transport::TransportKind;

fn harness() -> (AppCore, Arc<Mutex<Vec<MockDevice>>>) {
    let lister: Arc<dyn sony_buds_tray_control::transport::discovery::DeviceLister> =
        Arc::new(StaticDeviceLister(vec![DeviceInfo {
            name: "WH-1000XM5".into(),
            mac: "AA:BB:CC:DD:EE:FF".into(),
            paired: true,
            connected: false,
        }]));
    let devices: Arc<Mutex<Vec<MockDevice>>> = Arc::new(Mutex::new(Vec::new()));
    let factory: Arc<dyn sony_buds_tray_control::app::TransportFactory> =
        Arc::new(PairFactory(devices.clone()));
    (AppCore::new(lister, factory), devices)
}

/// Pumps the mock devices and the app loop until the exchange settles.
///
/// The std Mutex guard is held across the mock pipes' awaits; the mock
/// transports never block on this lock, so this is safe.
#[allow(clippy::await_holding_lock)]
async fn pump_app(app: &mut AppCore, devices: &Arc<Mutex<Vec<MockDevice>>>) {
    let mut quiet = 0u32;
    for _ in 0..600 {
        let mut acted = false;
        let before: usize = devices
            .lock()
            .unwrap()
            .iter()
            .map(|d| d.received.len())
            .sum();
        {
            let mut devs = devices.lock().unwrap();
            for d in devs.iter_mut() {
                while d.run_once().await {
                    acted = true;
                }
            }
        }
        let after: usize = devices
            .lock()
            .unwrap()
            .iter()
            .map(|d| d.received.len())
            .sum();
        if after != before {
            acted = true;
        }
        app.tick().await;
        if acted {
            quiet = 0;
        } else {
            quiet += 1;
            if quiet >= 3 {
                break;
            }
        }
    }
}

#[tokio::test]
async fn connect_init_and_menu_reflect_device() {
    let (mut app, devices) = harness();

    app.apply_command(UiCommand::RefreshDevices);
    pump_app(&mut app, &devices).await;
    assert_eq!(app.devices.len(), 1);

    app.apply_command(UiCommand::Connect {
        mac: "AA:BB:CC:DD:EE:FF".into(),
    });
    pump_app(&mut app, &devices).await;
    assert_eq!(app.conn_state, ConnState::Connected);

    // Give the engine time to finish the init handshake.
    pump_app(&mut app, &devices).await;
    assert_eq!(app.conn_state, ConnState::Connected);

    let snap = app.snapshot();
    let labels: Vec<String> = flatten(&snap.menu).into_iter().map(|m| m.label).collect();
    assert!(
        labels.iter().any(|l| l.contains("WH-1000XM5")),
        "labels: {labels:?}"
    );
    assert!(labels.iter().any(|l| l.contains("Battery")));
    assert!(labels.iter().any(|l| l.contains("Volume")));
    assert!(labels.iter().any(|l| l.contains("Ambient Sound")));
    assert!(labels.iter().any(|l| l.contains("Equalizer")));
    assert!(labels.iter().any(|l| l.contains("Quit")));
}

#[tokio::test]
async fn volume_command_reaches_the_device() {
    let (mut app, devices) = harness();

    app.apply_command(UiCommand::Connect {
        mac: "AA:BB:CC:DD:EE:FF".into(),
    });
    pump_app(&mut app, &devices).await;
    pump_app(&mut app, &devices).await;

    app.apply_command(UiCommand::VolumeUp);
    app.apply_command(UiCommand::VolumeUp);
    pump_app(&mut app, &devices).await;

    let devs = devices.lock().unwrap();
    let device = devs.first().expect("device created");
    assert_eq!(device.state.volume, 14, "two volume steps from 12");
}

#[tokio::test]
async fn ambient_mode_switch_via_menu_command() {
    let (mut app, devices) = harness();

    app.apply_command(UiCommand::Connect {
        mac: "AA:BB:CC:DD:EE:FF".into(),
    });
    pump_app(&mut app, &devices).await;
    pump_app(&mut app, &devices).await;

    app.apply_command(UiCommand::SetAmbientMode(AmbientSel::Asm));
    app.apply_command(UiCommand::AmbientUp);
    pump_app(&mut app, &devices).await;

    let devs = devices.lock().unwrap();
    let device = devs.first().expect("device created");
    assert!(device.state.nc_asm_enabled);
    assert_eq!(device.state.nc_asm_mode, NcAsmMode::Asm);
    assert_eq!(device.state.ambient_level, 13);
}

#[tokio::test]
async fn menu_radio_states_follow_device_reports() {
    let (mut app, devices) = harness();

    app.apply_command(UiCommand::Connect {
        mac: "AA:BB:CC:DD:EE:FF".into(),
    });
    pump_app(&mut app, &devices).await;
    pump_app(&mut app, &devices).await;

    // The device reports NC enabled, volume 12, EQ Off.
    let snap = app.snapshot();
    let items = flatten(&snap.menu);
    let nc = items
        .iter()
        .find(|i| i.label.contains("Noise Cancelling"))
        .expect("NC radio present");
    match &nc.kind {
        ItemKind::Radio { checked, .. } => assert!(*checked, "NC should be checked"),
        _ => panic!("expected radio item"),
    }
}

#[tokio::test]
async fn stc_and_eq_commands_commit() {
    let (mut app, devices) = harness();

    app.apply_command(UiCommand::Connect {
        mac: "AA:BB:CC:DD:EE:FF".into(),
    });
    pump_app(&mut app, &devices).await;
    pump_app(&mut app, &devices).await;

    app.apply_command(UiCommand::SetSpeakToChat(true));
    app.apply_command(UiCommand::SetStcSensitivity(DetectSensitivity::High));
    app.apply_command(UiCommand::SetStcModeOut(ModeOutTime::Slow));
    app.apply_command(UiCommand::SetEqPreset(EqPresetId::Rock));
    pump_app(&mut app, &devices).await;

    let devs = devices.lock().unwrap();
    let device = devs.first().expect("device created");
    assert!(device.state.stc_enabled);
    assert_eq!(device.state.stc_sensitivity, DetectSensitivity::High);
    assert_eq!(device.state.stc_mode_out, ModeOutTime::Slow);
    assert_eq!(device.state.eq_preset, EqPresetId::Rock);
}

#[tokio::test]
async fn disconnect_tears_down_cleanly() {
    let (mut app, devices) = harness();

    app.apply_command(UiCommand::Connect {
        mac: "AA:BB:CC:DD:EE:FF".into(),
    });
    pump_app(&mut app, &devices).await;
    pump_app(&mut app, &devices).await;
    assert_eq!(app.conn_state, ConnState::Connected);

    app.apply_command(UiCommand::Disconnect);
    pump_app(&mut app, &devices).await;
    assert_eq!(app.conn_state, ConnState::NotConnected);

    // Menu offers connect again.
    let snap = app.snapshot();
    let labels: Vec<String> = flatten(&snap.menu).into_iter().map(|m| m.label).collect();
    assert!(labels.iter().any(|l| l.contains("Quit")));
}

#[tokio::test]
async fn transport_kind_selection_is_remembered() {
    let (mut app, devices) = harness();
    app.apply_command(UiCommand::SetTransport(TransportKind::Ble));
    assert_eq!(app.transport_kind, TransportKind::Ble);
    pump_app(&mut app, &devices).await;
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

#[tokio::test]
async fn check_items_toggle_both_ways() {
    let (mut app, devices) = harness();

    app.apply_command(UiCommand::Connect {
        mac: "AA:BB:CC:DD:EE:FF".into(),
    });
    pump_app(&mut app, &devices).await;
    pump_app(&mut app, &devices).await;

    // Enable Speak to Chat via the menu's inverse-of-state command.
    app.apply_command(UiCommand::SetSpeakToChat(true));
    pump_app(&mut app, &devices).await;
    {
        let devs = devices.lock().unwrap();
        assert!(devs.first().unwrap().state.stc_enabled);
    }

    // The rebuilt menu must now offer turning it off.
    let snap = app.snapshot();
    let items = flatten(&snap.menu);
    let cmd = items
        .iter()
        .find(|i| i.label == "Enabled")
        .and_then(|i| match &i.kind {
            ItemKind::Check { checked, cmd } => {
                assert!(*checked);
                Some(cmd.clone())
            }
            _ => None,
        })
        .expect("STC check item");
    app.apply_command(cmd);
    pump_app(&mut app, &devices).await;
    {
        let devs = devices.lock().unwrap();
        assert!(
            !devs.first().unwrap().state.stc_enabled,
            "toggle must be able to turn it off"
        );
    }
}
