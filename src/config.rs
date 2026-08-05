//! Persistent settings: auto-connect at startup and the last connected
//! device. Stored as a simple key=value file under the XDG config dir.

use std::path::PathBuf;

/// User-facing settings persisted between runs.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Config {
    /// Automatically connect to the last device when the app starts.
    pub auto_connect: bool,
    /// MAC of the last device we tried to connect to.
    pub last_device: Option<String>,
    /// Config directory override (tests). `None` = XDG default.
    dir: Option<PathBuf>,
}

impl Config {
    fn default_dir() -> PathBuf {
        std::env::var_os("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .or_else(|| {
                std::env::var_os("HOME")
                    .map(PathBuf::from)
                    .map(|h| h.join(".config"))
            })
            .unwrap_or_default()
            .join("sony-buds-tray-control")
    }

    fn path(&self) -> PathBuf {
        self.dir
            .clone()
            .unwrap_or_else(Self::default_dir)
            .join("config")
    }

    /// Loads the settings from the default (XDG) location.
    pub fn load() -> Self {
        Self::load_from(Self::default_dir())
    }

    /// Loads the settings from an explicit directory (used by tests so they
    /// never touch the user's real config).
    pub fn load_from(dir: PathBuf) -> Self {
        let mut cfg = Config {
            dir: Some(dir),
            ..Default::default()
        };
        let Ok(text) = std::fs::read_to_string(cfg.path()) else {
            return cfg;
        };
        for line in text.lines() {
            let line = line.trim();
            let Some((k, v)) = line.split_once('=') else {
                continue;
            };
            let (k, v) = (k.trim(), v.trim().trim_matches('"'));
            match k {
                "auto_connect" => cfg.auto_connect = v == "true",
                "last_device" if !v.is_empty() => cfg.last_device = Some(v.to_string()),
                _ => {}
            }
        }
        cfg
    }

    pub fn save(&self) {
        let path = self.path();
        let _ = std::fs::create_dir_all(path.parent().unwrap_or(std::path::Path::new(".")));
        let text = format!(
            "auto_connect = {}\nlast_device = {}\n",
            self.auto_connect,
            self.last_device
                .as_deref()
                .map(|m| format!("\"{m}\""))
                .unwrap_or_else(|| "\"\"".to_string())
        );
        let _ = std::fs::write(path, text);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn config_round_trip() {
        let dir = std::env::temp_dir().join(format!(
            "sony-buds-config-test-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        let cfg = Config {
            auto_connect: true,
            last_device: Some("AA:BB:CC:DD:EE:FF".into()),
            dir: None,
        };
        let dir2 = dir.clone();
        let scoped = Config {
            auto_connect: cfg.auto_connect,
            last_device: cfg.last_device.clone(),
            dir: Some(dir),
        };
        scoped.save();
        assert_eq!(Config::load_from(dir2.clone()), scoped);
        let _ = std::fs::remove_dir_all(dir2);
    }
}
