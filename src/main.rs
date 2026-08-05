//! Sony Buds Tray Control — KDE system tray control for Sony headphones.
//!
//! Usage: `sony-buds-tray-control` (optionally with `RUST_LOG=debug` for
//! verbose logging).

use std::sync::{Arc, RwLock};

use sony_buds_tray_control::app::{AppCore, RealTransportFactory, UiCommand, UiSnapshot};
use sony_buds_tray_control::transport::discovery::{BlueZDeviceLister, DeviceLister};
use sony_buds_tray_control::transport::TransportError;

fn main() -> anyhow::Result<()> {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();
    log::info!("starting Sony Buds Tray Control");

    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?;
    runtime.block_on(run_app())
}

async fn run_app() -> anyhow::Result<()> {
    // One BlueZ session shared by discovery and the BLE transport. If BlueZ
    // is unavailable (e.g. no D-Bus system bus yet), the tray still starts
    // and surfaces the error in the menu.
    let session = bluer::Session::new().await;
    match &session {
        Ok(_) => log::info!("connected to BlueZ"),
        Err(e) => log::error!("BlueZ unavailable: {e}"),
    }

    let lister: Arc<dyn DeviceLister> = match &session {
        Ok(s) => Arc::new(BlueZDeviceLister::new(s.clone())),
        Err(e) => Arc::new(NoBlueZLister(TransportError::Internal(Box::leak(
            format!("BlueZ unavailable: {e}").into_boxed_str(),
        )))),
    };
    let factory: Arc<dyn sony_buds_tray_control::app::TransportFactory> = match &session {
        Ok(s) => Arc::new(RealTransportFactory::new(s.clone())),
        Err(e) => Arc::new(NoBlueZFactory(TransportError::Internal(Box::leak(
            format!("BlueZ unavailable: {e}").into_boxed_str(),
        )))),
    };

    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<UiCommand>();

    let shared: Arc<RwLock<UiSnapshot>> = Arc::new(RwLock::new(UiSnapshot {
        conn_state: sony_buds_tray_control::app::ConnState::NotConnected,
        menu: Vec::new(),
        tooltip: "Sony Buds Control".to_string(),
        icon_name: "audio-headphones-symbolic".to_string(),
        title: "Sony Buds Control".to_string(),
    }));

    // App core task: handles commands and drives the device engine.
    let app_shared = shared.clone();
    let app_task = tokio::spawn(async move {
        let mut app = AppCore::new(lister, factory);
        let mut ticker = tokio::time::interval(std::time::Duration::from_millis(100));
        ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        loop {
            tokio::select! {
                cmd = rx.recv() => {
                    let Some(cmd) = cmd else { break };
                    if cmd == UiCommand::Quit {
                        break;
                    }
                    app.apply_command(cmd);
                }
                _ = ticker.tick() => {
                    app.tick().await;
                }
            }
            // Publish the latest snapshot when anything changed.
            if app.menu_dirty {
                if let Ok(mut snap) = app_shared.write() {
                    *snap = app.snapshot();
                }
                app.menu_dirty = false;
            }
        }
        // Mark the snapshot disconnected on exit.
        if let Ok(mut snap) = app_shared.write() {
            snap.conn_state = sony_buds_tray_control::app::ConnState::NotConnected;
        }
    });

    // Tray task: renders the snapshot, forwards actions.
    let tx2 = tx.clone();
    let tray_task = tokio::spawn(async move {
        if let Err(e) = sony_buds_tray_control::tray::run(shared, tx2).await {
            log::error!("tray error: {e}");
        }
    });

    // Run until the user quits from the menu or the process is signalled.
    let mut app_task = std::pin::pin!(app_task);
    tokio::select! {
        _ = app_task.as_mut() => {}
        _ = tokio::signal::ctrl_c() => {
            let _ = tx.send(UiCommand::Quit);
        }
    }
    tray_task.abort();
    Ok(())
}

/// Device lister used when BlueZ is unavailable: reports the reason.
struct NoBlueZLister(TransportError);

#[async_trait::async_trait]
impl DeviceLister for NoBlueZLister {
    async fn list_devices(
        &self,
    ) -> Result<Vec<sony_buds_tray_control::transport::discovery::DeviceInfo>, TransportError> {
        Err(self.0)
    }
}

/// Transport factory used when BlueZ is unavailable: reports the reason.
struct NoBlueZFactory(TransportError);

#[async_trait::async_trait]
impl sony_buds_tray_control::app::TransportFactory for NoBlueZFactory {
    async fn create(
        &self,
        _kind: sony_buds_tray_control::transport::TransportKind,
    ) -> Result<Box<dyn sony_buds_tray_control::transport::Transport>, String> {
        Err(self.0.to_string())
    }
}
