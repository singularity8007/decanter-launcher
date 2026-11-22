use crate::settings::{self, Field, Toggle};
use std::env::{self, VarError};
use std::path::PathBuf;

/// The game's exe, relative to the install folder.
const GAME_EXE: &str = "ZenlessZoneZero.exe";

/// Everything the launcher needs to know. Hardcoded defaults (D-006), but the
/// player can change any path except `settings_path` on the settings page.
#[derive(Debug)]
#[allow(dead_code)]
pub struct Config {
    pub runner_dir: PathBuf,    // Path to Proton folder
    pub steamrt_dir: PathBuf,   // Path to SteamLinuxRuntime_sniper (SLR 3)
    pub reaper_path: PathBuf,   // Path to reaper binary
    pub prefix_dir: PathBuf,    // Path to the folder that holds the pfx/ prefix
    pub game_dir: PathBuf,      // Path to ZZZ install
    pub exe: PathBuf,           // Path to the exe to launch (Wine accepts a Linux path here)
    pub stats_path: PathBuf,    // Path to play time and last-played date (JSON)
    pub settings_path: PathBuf, // Path to the settings the player can change (JSON)
    pub wayland: bool,          // Native Wayland instead of XWayland
    pub hud: bool,              // DXVK HUD on
    pub dx12: bool,             // Launch with -use-d3d12
    pub performance: bool,      // Hold the performance power profile while playing
    pub fps_cap: u32,           // 0 = no cap
}

impl Config {
    pub fn hardcoded() -> Result<Self, VarError> {
        let root = PathBuf::from(env::var("HOME")?).join("wine_launcher");
        let settings_path = root.join("settings.json");

        // A path the player changed wins. A missing or empty one means the default.
        let saved = settings::load(&settings_path);
        let pick = |field: Field, default: PathBuf| match saved.get(field).map(str::trim) {
            Some(path) if !path.is_empty() => PathBuf::from(path),
            _ => default,
        };

        let game_dir = pick(Field::Game, root.join("game"));
        Ok(Self {
            runner_dir: pick(Field::Runner, root.join("proton").join("ge-proton")),
            steamrt_dir: pick(Field::SteamRt, root.join("steamrt").join("steamrt3")),
            reaper_path: pick(Field::Reaper, root.join("bin").join("reaper")),
            prefix_dir: pick(Field::Prefix, root.join("prefix")),
            exe: pick(Field::Exe, game_dir.join(GAME_EXE)),
            stats_path: pick(Field::Stats, root.join("playtime.json")),
            settings_path,
            game_dir,
            wayland: saved.flag(Toggle::Wayland),
            hud: saved.flag(Toggle::Hud),
            dx12: saved.flag(Toggle::Dx12),
            performance: saved.flag(Toggle::Performance),
            fps_cap: saved.fps_cap(),
        })
    }

    /// The path behind one box on the settings page.
    pub fn path(&self, field: Field) -> &PathBuf {
        match field {
            Field::Runner => &self.runner_dir,
            Field::SteamRt => &self.steamrt_dir,
            Field::Reaper => &self.reaper_path,
            Field::Prefix => &self.prefix_dir,
            Field::Game => &self.game_dir,
            Field::Exe => &self.exe,
            Field::Stats => &self.stats_path,
            Field::SettingsFile => &self.settings_path,
        }
    }

    /// A launch switch as it is in use right now.
    pub fn flag(&self, toggle: Toggle) -> bool {
        match toggle {
            Toggle::Wayland => self.wayland,
            Toggle::Hud => self.hud,
            Toggle::Dx12 => self.dx12,
            Toggle::Performance => self.performance,
        }
    }

    /// The dxvk.conf the launcher writes and owns.
    pub fn dxvk_conf_path(&self) -> PathBuf {
        self.prefix_dir.join("dxvk.conf")
    }

    /// The Wine binary bundled inside GE-Proton.
    pub fn wine_bin(&self) -> PathBuf {
        self.runner_dir.join("files/bin/wine")
    }

    /// SLR's container entry point script.
    pub fn entry_point(&self) -> PathBuf {
        self.steamrt_dir.join("_v2-entry-point")
    }

    /// The value for the WINEPREFIX environment variable.
    pub fn wine_prefix(&self) -> PathBuf {
        self.prefix_dir.join("pfx")
    }
}