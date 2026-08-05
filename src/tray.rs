//! KDE / freedesktop system tray integration via `ksni` (StatusNotifierItem).
//!
//! The tray is a thin presentation layer: it renders the [`UiSnapshot`]
//! produced by the app core and forwards user actions back through a channel.

use std::path::PathBuf;
use std::sync::{Arc, RwLock};

use ksni::menu::{CheckmarkItem, MenuItem as KsniItem, StandardItem, SubMenu};
use ksni::{Tray as TrayTrait, TrayMethods};
use tokio::sync::mpsc::UnboundedSender;

use crate::app::{ItemKind, MenuItem, UiCommand, UiSnapshot};

pub struct Tray {
    shared: Arc<RwLock<UiSnapshot>>,
    tx: UnboundedSender<UiCommand>,
    icon_theme_dir: PathBuf,
}

impl Tray {
    pub fn new(shared: Arc<RwLock<UiSnapshot>>, tx: UnboundedSender<UiCommand>) -> Self {
        Self {
            shared,
            tx,
            icon_theme_dir: ensure_icon_theme(),
        }
    }
}

impl TrayTrait for Tray {
    fn id(&self) -> String {
        "sony-buds-tray-control".to_string()
    }

    fn title(&self) -> String {
        "Sony Buds Control".to_string()
    }

    fn activate(&mut self, _x: i32, _y: i32) {
        // Single click cycles the noise cancelling / ambient sound mode.
        let _ = self.tx.send(UiCommand::CycleAmbientMode);
    }

    fn icon_theme_path(&self) -> String {
        self.icon_theme_dir.to_string_lossy().into_owned()
    }

    fn icon_name(&self) -> String {
        // When connected, pick the SVG variant with the NC status dot baked
        // in (green = NC, blue = ambient sound, grey = NC/ASM off); when
        // idle, use our red-cross variant. Hosts resolve them from our
        // IconThemePath.
        let snap = self.shared.read().ok();
        let connected = snap
            .as_ref()
            .map(|s| s.conn_state == crate::app::ConnState::Connected)
            .unwrap_or(false);
        if !connected {
            return "sony-buds-disconnected".to_string();
        }
        match snap.map(|s| s.nc_dot).unwrap_or(crate::app::NcDot::Hidden) {
            crate::app::NcDot::NoiseCancelling => "sony-buds-nc",
            crate::app::NcDot::Ambient => "sony-buds-asm",
            _ => "sony-buds-off",
        }
        .to_string()
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

/// Ensures the custom icon theme directory exists with the headphone SVG
/// variants (one per NC state, plus the disconnected red-cross variant) and
/// returns its path. Hosts resolve icons that are missing from the system
/// theme via `IconThemePath/<name>.svg` (KIconLoader's "User" group
/// fallback), so no index.theme is needed.
fn ensure_icon_theme() -> PathBuf {
    let dir = std::env::temp_dir()
        .join("sony-buds-tray-control")
        .join("icons");
    let _ = std::fs::create_dir_all(&dir);
    let variants = [
        ("sony-buds-nc.svg", Some("#2EC853"), false),
        ("sony-buds-asm.svg", Some("#2979FF"), false),
        ("sony-buds-off.svg", Some("#9E9E9E"), false),
        ("sony-buds-disconnected.svg", None, true),
    ];
    for (name, color, cross) in variants {
        let _ = std::fs::write(dir.join(name), headphone_svg(color, cross));
    }
    dir
}

/// The headphone SVG (Breeze `audio-headphones` glyph) with an optional
/// status dot in the bottom-right corner and/or a red "disconnected" cross.
fn headphone_svg(dot: Option<&str>, cross: bool) -> String {
    let dot = match dot {
        Some(color) => format!(r#"<circle cx="19.2" cy="19.2" r="2.8" fill="{color}"/>"#),
        None => String::new(),
    };
    let cross = if cross {
        r##"<path d="M 16.7 16.7 L 21.7 21.7 M 21.7 16.7 L 16.7 21.7" stroke="#E53935" stroke-width="2.2" stroke-linecap="round"/>"##.to_string()
    } else {
        String::new()
    };
    format!(
        r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 22 22"><path fill="#ffffff" d="M 11 3 A 8 8 0 0 0 3 11 L 3 19 L 4 19 L 4 17 L 6 19 L 7 19 L 7 13 L 6 13 L 4 15 L 4 11 A 7 7 0 0 1 11 4 A 7 7 0 0 1 18 11 L 18 15 L 16 13 L 15 13 L 15 19 L 16 19 L 18 17 L 18 19 L 19 19 L 19 11 A 8 8 0 0 0 11 3 z"/>{cross}{dot}</svg>"##
    )
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn svg_variants_are_valid() {
        let with_dot = headphone_svg(Some("#2EC853"), false);
        let without = headphone_svg(None, false);
        assert!(with_dot.starts_with("<svg"));
        assert!(with_dot.ends_with("</svg>"));
        assert!(with_dot.contains("viewBox"));
        assert!(with_dot.contains("<circle"));
        assert!(!without.contains("<circle"));
    }

    #[test]
    fn disconnected_svg_has_red_cross() {
        let s = headphone_svg(None, true);
        assert!(s.contains("stroke=\"#E53935\""));
        assert!(s.contains("M 16.7 16.7 L 21.7 21.7"));
        assert!(!s.contains("<circle"));
    }

    #[test]
    fn icon_theme_dir_contains_variants() {
        let dir = ensure_icon_theme();
        assert!(dir.join("sony-buds-nc.svg").exists());
        assert!(dir.join("sony-buds-asm.svg").exists());
        assert!(dir.join("sony-buds-off.svg").exists());
        assert!(dir.join("sony-buds-disconnected.svg").exists());
    }
}
