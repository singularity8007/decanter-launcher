//! Settings the player can change, kept in a small JSON file.
//!
//! Read once when the launcher starts (see `Config::hardcoded`) and written
//! every time the player edits a box in the window, so closing the launcher
//! never loses a change. A change only takes effect the next time the
//! launcher starts. A path that was never changed isn't in the file, so it
//! keeps following its default.

use serde::{Deserialize, Serialize};
use std::fs;
use std::io;
use std::path::Path;

/// One path on the settings page, in the order the page shows them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Field {
    Runner,
    SteamRt,
    Reaper,
    Prefix,
    Game,
    Exe,
    Stats,
    SettingsFile,
}

impl Field {
    pub const ALL: [Field; 8] = [
        Field::Runner,
        Field::SteamRt,
        Field::Reaper,
        Field::Prefix,
        Field::Game,
        Field::Exe,
        Field::Stats,
        Field::SettingsFile,
    ];

    /// The small text above the box. Same wording as the comments in `Config`.
    pub fn label(self) -> &'static str {
        match self {
            Field::Runner => "Path to Proton folder",
            Field::SteamRt => "Path to SteamLinuxRuntime_sniper (SLR 3)",
            Field::Reaper => "Path to reaper binary",
            Field::Prefix => "Path to the folder that holds the pfx/ prefix",
            Field::Game => "Path to ZZZ install",
            Field::Exe => "Path to the exe to launch (Wine accepts a Linux path here)",
            Field::Stats => "Path to play time and last-played date (JSON)",
            Field::SettingsFile => "Path to the settings the player can change (JSON)",
        }
    }

    /// The settings file can't be moved by a value stored inside it: the
    /// launcher has to know where it is before it can read anything. So that
    /// box is shown but can't be typed in.
    pub fn editable(self) -> bool {
        self != Field::SettingsFile
    }
}

/// An on/off switch on the settings page that changes how the game is launched.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Toggle {
    Wayland,
    Hud,
    Dx12,
    Performance,
}

impl Toggle {
    pub const ALL: [Toggle; 4] = [Toggle::Wayland, Toggle::Hud, Toggle::Dx12, Toggle::Performance];

    pub fn label(self) -> &'static str {
        match self {
            Toggle::Wayland => "Native Wayland (PROTON_ENABLE_WAYLAND). Off = XWayland",
            Toggle::Hud => "Show DXVK HUD (fps + frametime graph)",
            Toggle::Dx12 => "Use DirectX 12 (-use-d3d12, needed for DLSS / frame gen)",
            Toggle::Performance => "Hold the 'performance' power profile while playing",
        }
    }

    /// What a switch is when the player never touched it.
    pub fn default_on(self) -> bool {
        match self {
            // Off, like TwintailLauncher: XWayland paces ZZZ smoothly on
            // KDE + NVIDIA, the native Wayland driver doesn't.
            Toggle::Wayland => false,
            Toggle::Hud => false,
            Toggle::Dx12 => false,
            Toggle::Performance => true,
        }
    }
}

/// What the player changed. `None` means "never changed": use the default.
#[derive(Default, Serialize, Deserialize)]
pub struct Settings {
    // `proton_folder` is what the first version of this file called it.
    #[serde(alias = "proton_folder")]
    runner_dir: Option<String>,
    steamrt_dir: Option<String>,
    reaper_path: Option<String>,
    prefix_dir: Option<String>,
    game_dir: Option<String>,
    exe: Option<String>,
    stats_path: Option<String>,
    wayland: Option<bool>,
    hud: Option<bool>,
    dx12: Option<bool>,
    performance_mode: Option<bool>,
    /// 0 means no cap.
    fps_cap: Option<u32>,
}

impl Settings {
    /// What the player saved for `field`, if anything.
    pub fn get(&self, field: Field) -> Option<&str> {
        match field {
            Field::Runner => self.runner_dir.as_deref(),
            Field::SteamRt => self.steamrt_dir.as_deref(),
            Field::Reaper => self.reaper_path.as_deref(),
            Field::Prefix => self.prefix_dir.as_deref(),
            Field::Game => self.game_dir.as_deref(),
            Field::Exe => self.exe.as_deref(),
            Field::Stats => self.stats_path.as_deref(),
            Field::SettingsFile => None,
        }
    }

    /// Remember what the player typed for `field`.
    pub fn set(&mut self, field: Field, path: &str) {
        let slot = match field {
            Field::Runner => &mut self.runner_dir,
            Field::SteamRt => &mut self.steamrt_dir,
            Field::Reaper => &mut self.reaper_path,
            Field::Prefix => &mut self.prefix_dir,
            Field::Game => &mut self.game_dir,
            Field::Exe => &mut self.exe,
            Field::Stats => &mut self.stats_path,
            Field::SettingsFile => return,
        };
        *slot = Some(path.to_string());
    }

    fn flag_slot(&mut self, toggle: Toggle) -> &mut Option<bool> {
        match toggle {
            Toggle::Wayland => &mut self.wayland,
            Toggle::Hud => &mut self.hud,
            Toggle::Dx12 => &mut self.dx12,
            Toggle::Performance => &mut self.performance_mode,
        }
    }

    /// The switch as saved, or its default.
    pub fn flag(&self, toggle: Toggle) -> bool {
        let saved = match toggle {
            Toggle::Wayland => self.wayland,
            Toggle::Hud => self.hud,
            Toggle::Dx12 => self.dx12,
            Toggle::Performance => self.performance_mode,
        };
        saved.unwrap_or(toggle.default_on())
    }

    pub fn set_flag(&mut self, toggle: Toggle, on: bool) {
        *self.flag_slot(toggle) = Some(on);
    }

    /// FPS cap, 0 = none.
    pub fn fps_cap(&self) -> u32 {
        self.fps_cap.unwrap_or(0)
    }

    pub fn set_fps_cap(&mut self, cap: u32) {
        self.fps_cap = Some(cap);
    }
}

/// Read the saved settings. Starts from the defaults if there's no file yet.
pub fn load(path: &Path) -> Settings {
    fs::read_to_string(path)
        .ok()
        .and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or_default()
}

/// Write the settings through a temp file so the file is never half-written.
pub fn save(path: &Path, settings: &Settings) -> io::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let text = serde_json::to_string_pretty(settings).map_err(io::Error::other)?;
    let temp = path.with_extension("json.tmp");
    fs::write(&temp, text)?;
    fs::rename(&temp, path)
}