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

fn test_config() -> sony_buds_tray_control::config::Config {
    // Isolated config dir so tests never read or write the user's real
    // settings (which would leak the mock MAC into the live app).
    sony_buds_tray_control::config::Config::load_from(std::env::temp_dir().join(format!(
        "sony-buds-app-test-{}-{}",
        std::process::id(),
        std::thread::current().name().unwrap_or("")
    )))
}

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
    (
        AppCore::new_with_config(lister, factory, test_config()),
        devices,
    )
}

/// Pumps the mock devices and the app loop until the exchange settles.
///
/// The std Mutex guard is held across the mock pipes' awaits; the mock
/// transports never block on this lock, so this is safe.
#[allow(clippy::await_holding_lock)]
async fn pump_app(app: &mut AppCore, devices: &Arc<Mutex<Vec<MockDevice>>>) {
    let mut quiet = 0u32;
    for _it in 0..600 {
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
async fn auto_connect_connects_to_last_device_at_startup() {
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
    let mut config = test_config();
    config.auto_connect = true;
    config.last_device = Some("AA:BB:CC:DD:EE:FF".into());
    let mut app = AppCore::new_with_config(lister, factory, config);

    // A single pump: refresh + auto-connect + full handshake.
    pump_app(&mut app, &devices).await;
    assert_eq!(app.conn_state, ConnState::Connected);

    // Auto-connect must not fire again after a disconnect.
    app.apply_command(UiCommand::Disconnect);
    pump_app(&mut app, &devices).await;
    assert_eq!(app.conn_state, ConnState::NotConnected);
}

#[tokio::test]
async fn auto_reconnect_after_connection_loss() {
    use std::sync::atomic::{AtomicBool, Ordering};

    // Lister whose "connected" flag can be flipped to simulate the user
    // reconnecting the headphone through the system Bluetooth UI.
    struct ToggleLister {
        device: DeviceInfo,
        connected: Arc<AtomicBool>,
    }
    #[async_trait::async_trait]
    impl sony_buds_tray_control::transport::discovery::DeviceLister for ToggleLister {
        async fn list_devices(
            &self,
        ) -> Result<Vec<DeviceInfo>, sony_buds_tray_control::transport::TransportError> {
            let mut d = self.device.clone();
            d.connected = self.connected.load(Ordering::SeqCst);
            Ok(vec![d])
        }
    }

    let device_connected = Arc::new(AtomicBool::new(false));
    let lister: Arc<dyn sony_buds_tray_control::transport::discovery::DeviceLister> =
        Arc::new(ToggleLister {
            device: DeviceInfo {
                name: "WH-1000XM5".into(),
                mac: "AA:BB:CC:DD:EE:FF".into(),
                paired: true,
                connected: false,
            },
            connected: device_connected.clone(),
        });
    let devices: Arc<Mutex<Vec<MockDevice>>> = Arc::new(Mutex::new(Vec::new()));
    let factory: Arc<dyn sony_buds_tray_control::app::TransportFactory> =
        Arc::new(PairFactory(devices.clone()));
    let mut config = test_config();
    config.auto_connect = true;
    config.last_device = Some("AA:BB:CC:DD:EE:FF".into());
    let mut app = AppCore::new_with_config(lister, factory, config);
    app.reconnect_delay = std::time::Duration::from_millis(50);

    app.apply_command(UiCommand::Connect {
        mac: "AA:BB:CC:DD:EE:FF".into(),
    });
    pump_app(&mut app, &devices).await;
    assert_eq!(app.conn_state, ConnState::Connected);

    // Simulate the headphone dropping off Bluetooth: dropping the mock
    // device closes the transport pipe.
    devices.lock().unwrap().clear();
    pump_app(&mut app, &devices).await;
    assert!(matches!(
        app.conn_state,
        ConnState::Error(_) | ConnState::NotConnected
    ));
    assert!(
        app.snapshot()
            .menu
            .iter()
            .any(|m| m.label.contains("Waiting")),
        "menu should show the waiting state"
    );

    // Device still away: no connection attempts, stays in wait mode.
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    pump_app(&mut app, &devices).await;
    assert!(!app.is_connected());
    assert_eq!(
        devices.lock().unwrap().len(),
        0,
        "no connect attempts while away"
    );

    // User reconnects via the system UI: the app attaches on its own.
    device_connected.store(true, Ordering::SeqCst);
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    pump_app(&mut app, &devices).await;
    assert_eq!(app.conn_state, ConnState::Connected);

    // A manual disconnect cancels auto-reconnect.
    app.apply_command(UiCommand::Disconnect);
    pump_app(&mut app, &devices).await;
    assert_eq!(app.conn_state, ConnState::NotConnected);
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    pump_app(&mut app, &devices).await;
    assert_eq!(app.conn_state, ConnState::NotConnected);
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
async fn multipoint_menu_lists_devices_and_switches_playback() {
    let (mut app, devices) = harness();

    app.apply_command(UiCommand::Connect {
        mac: "AA:BB:CC:DD:EE:FF".into(),
    });
    pump_app(&mut app, &devices).await;
    pump_app(&mut app, &devices).await;

    // The multipoint submenu lists both devices, playback one checked.
    let snap = app.snapshot();
    let items = flatten(&snap.menu);
    let mp = items
        .iter()
        .find(|i| i.label == "🔁 Multipoint")
        .expect("multipoint submenu present");
    let children = match &mp.kind {
        ItemKind::Submenu(c) => c,
        _ => panic!("expected submenu"),
    };
    let phone = children
        .iter()
        .find(|i| i.label.contains("My Phone"))
        .expect("phone row");
    let laptop = children
        .iter()
        .find(|i| i.label.contains("Laptop"))
        .expect("laptop row");
    match &phone.kind {
        ItemKind::Radio { checked, .. } => assert!(*checked, "phone is the playback device"),
        _ => panic!("expected radio item"),
    }
    match &laptop.kind {
        ItemKind::Radio { checked, .. } => assert!(!*checked, "laptop is not playing"),
        _ => panic!("expected radio item"),
    }
    // The disconnected device is marked as paired.
    assert!(children.iter().any(|i| i.label.contains("· paired")));

    // The paired laptop is not connected: clicking it connects it.
    let cmd = match &laptop.kind {
        ItemKind::Radio { cmd, .. } => cmd.clone(),
        _ => unreachable!(),
    };
    app.apply_command(cmd);
    app.apply_command(UiCommand::RefreshSync);
    pump_app(&mut app, &devices).await;
    let laptop_connected = {
        let devs = devices.lock().unwrap();
        let device = devs.first().expect("device created");
        device.state.multipoint_devices[1].2
    };
    assert!(laptop_connected, "laptop connected");

    // With the laptop connected, the row now switches playback to it.
    let snap = app.snapshot();
    let items = flatten(&snap.menu);
    let mp = items
        .iter()
        .find(|i| i.label == "🔁 Multipoint")
        .expect("multipoint submenu present");
    let children = match &mp.kind {
        ItemKind::Submenu(c) => c,
        _ => panic!("expected submenu"),
    };
    let laptop = children
        .iter()
        .find(|i| i.label.contains("Laptop"))
        .expect("laptop row");
    let cmd = match &laptop.kind {
        ItemKind::Radio { cmd, .. } => cmd.clone(),
        _ => unreachable!(),
    };
    app.apply_command(cmd);
    pump_app(&mut app, &devices).await;
    // The device confirmed the switch and the radio moved.
    let devs = devices.lock().unwrap();
    let device = devs.first().expect("device created");
    assert_eq!(device.state.multipoint_playback, 1);
    drop(devs);

    let snap = app.snapshot();
    let items = flatten(&snap.menu);
    let mp = items
        .iter()
        .find(|i| i.label == "🔁 Multipoint")
        .expect("multipoint submenu present");
    let children = match &mp.kind {
        ItemKind::Submenu(c) => c,
        _ => panic!("expected submenu"),
    };
    let laptop = children
        .iter()
        .find(|i| i.label.contains("Laptop"))
        .expect("laptop row");
    match &laptop.kind {
        ItemKind::Radio { checked, .. } => assert!(*checked, "laptop is now the playback device"),
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
