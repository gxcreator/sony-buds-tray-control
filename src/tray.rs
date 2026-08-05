//! KDE / freedesktop system tray integration via `ksni` (StatusNotifierItem).
//!
//! The tray is a thin presentation layer: it renders the [`UiSnapshot`]
//! produced by the app core and forwards user actions back through a channel.

use std::sync::{Arc, RwLock};

use ksni::menu::{CheckmarkItem, MenuItem as KsniItem, StandardItem, SubMenu};
use ksni::{Tray as TrayTrait, TrayMethods};
use tokio::sync::mpsc::UnboundedSender;

use crate::app::{ItemKind, MenuItem, UiCommand, UiSnapshot};

pub struct Tray {
    shared: Arc<RwLock<UiSnapshot>>,
    tx: UnboundedSender<UiCommand>,
}

impl Tray {
    pub fn new(shared: Arc<RwLock<UiSnapshot>>, tx: UnboundedSender<UiCommand>) -> Self {
        Self { shared, tx }
    }
}

impl TrayTrait for Tray {
    fn id(&self) -> String {
        "sony-buds-tray-control".to_string()
    }

    fn title(&self) -> String {
        "Sony Buds Control".to_string()
    }

    fn icon_name(&self) -> String {
        self.shared
            .read()
            .map(|s| s.icon_name.clone())
            .unwrap_or_else(|_| "audio-headphones-symbolic".to_string())
    }

    fn tool_tip(&self) -> ksni::ToolTip {
        let tip = self
            .shared
            .read()
            .map(|s| s.tooltip.clone())
            .unwrap_or_default();
        ksni::ToolTip {
            title: "Sony Buds Control".into(),
            description: tip,
            ..Default::default()
        }
    }

    fn status(&self) -> ksni::Status {
        let connected = self
            .shared
            .read()
            .map(|s| s.conn_state == crate::app::ConnState::Connected)
            .unwrap_or(false);
        if connected {
            ksni::Status::Active
        } else {
            ksni::Status::Passive
        }
    }

    fn menu(&self) -> Vec<KsniItem<Self>> {
        let snapshot = self.shared.read();
        let Ok(snapshot) = snapshot else {
            return vec![];
        };
        snapshot
            .menu
            .iter()
            .map(|item| to_ksni(item, &self.tx))
            .collect()
    }
}

fn to_ksni(item: &MenuItem, tx: &UnboundedSender<UiCommand>) -> KsniItem<Tray> {
    match &item.kind {
        ItemKind::Action(None) => StandardItem {
            label: item.label.clone(),
            enabled: false,
            ..Default::default()
        }
        .into(),
        ItemKind::Action(Some(cmd)) => {
            let tx = tx.clone();
            let cmd = cmd.clone();
            StandardItem {
                label: item.label.clone(),
                activate: Box::new(move |_: &mut Tray| {
                    let _ = tx.send(cmd.clone());
                }),
                ..Default::default()
            }
            .into()
        }
        ItemKind::Check { checked, cmd } => {
            let tx = tx.clone();
            let cmd = cmd.clone();
            CheckmarkItem {
                label: item.label.clone(),
                checked: *checked,
                activate: Box::new(move |_: &mut Tray| {
                    let _ = tx.send(cmd.clone());
                }),
                ..Default::default()
            }
            .into()
        }
        ItemKind::Radio { checked, cmd } => {
            let tx = tx.clone();
            let cmd = cmd.clone();
            StandardItem {
                label: item.label.clone(),
                activate: Box::new(move |_: &mut Tray| {
                    let _ = tx.send(cmd.clone());
                }),
                icon_name: if *checked {
                    "emblem-checked".into()
                } else {
                    String::new()
                },
                ..Default::default()
            }
            .into()
        }
        ItemKind::Submenu(children) => SubMenu {
            label: item.label.clone(),
            submenu: children.iter().map(|c| to_ksni(c, tx)).collect(),
            ..Default::default()
        }
        .into(),
        ItemKind::Separator => KsniItem::Separator,
    }
}

/// Spawns the tray service and keeps it alive forever (or until the host
/// closes the item).
pub async fn run(
    shared: Arc<RwLock<UiSnapshot>>,
    tx: UnboundedSender<UiCommand>,
) -> Result<(), ksni::Error> {
    let handle = Tray::new(shared.clone(), tx).spawn().await?;
    // KDE caches the SNI properties and the menu layout, so re-reading the
    // shared snapshot is not enough: we must notify the host through
    // `Handle::update` (emits NewStatus/NewToolTip/LayoutUpdated) whenever
    // the published snapshot changes, or the tray shows the initial state
    // forever.
    let mut last: Option<UiSnapshot> = None;
    loop {
        tokio::time::sleep(std::time::Duration::from_millis(200)).await;
        if handle.is_closed() {
            break;
        }
        let current = match shared.read() {
            Ok(guard) => Some(guard.clone()),
            Err(_) => None,
        };
        if current.is_some() && current != last {
            last = current;
            handle.update(|_| {}).await;
        }
    }
    Ok(())
}
