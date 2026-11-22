//! Launcher-wide settings, kept in `settings.json` in the data root.
//! Missing keys (older files) fall back to their defaults.

use serde::{Deserialize, Serialize};
use std::fs;
use std::io;
use std::path::Path;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct Settings {
    /// Native Wayland (PROTON_ENABLE_WAYLAND) instead of XWayland. Off by
    /// default: XWayland paces ZZZ smoothly on KDE + NVIDIA.
    pub wayland: bool,
    /// DXVK HUD with fps + frametimes.
    pub hud: bool,
    /// Hold the 'performance' power profile while a game runs.
    pub performance_mode: bool,
    /// 0 = no cap.
    pub fps_cap: u32,
    /// Update a game automatically when Play is pressed.
    pub auto_update_games: bool,
    /// Minimize the window while a game runs.
    pub minimize_on_launch: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            wayland: false,
            hud: false,
            performance_mode: true,
            fps_cap: 0,
            auto_update_games: true,
            minimize_on_launch: true,
        }
    }
}

pub fn load(path: &Path) -> Settings {
    fs::read_to_string(path).ok().and_then(|text| serde_json::from_str(&text).ok()).unwrap_or_default()
}

pub fn save(path: &Path, settings: &Settings) -> io::Result<()> {
    let text = serde_json::to_vec_pretty(settings).map_err(io::Error::other)?;
    crate::paths::write_atomic(path, &text)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_keys_use_defaults() {
        let s: Settings = serde_json::from_str(r#"{"hud": true}"#).unwrap();
        assert!(s.hud);
        assert!(s.performance_mode);
        assert!(s.auto_update_games);
    }

    #[test]
    fn round_trip() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("settings.json");
        let s = Settings { fps_cap: 138, wayland: true, ..Settings::default() };
        save(&path, &s).unwrap();
        assert_eq!(load(&path), s);
    }
}
