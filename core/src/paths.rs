//! Where Decanter keeps everything on disk.
//!
//! One data root (default `~/.local/share/decanter`) holding:
//!
//! ```text
//! games/<slug>/        default install folder per game
//! prefixes/<slug>/     Proton data path per game (holds pfx/ and dxvk.conf)
//! runners/<name>/      one folder per installed Proton build
//! steamrt/steamrt3/    Steam Linux Runtime 3 "sniper"
//! steamrt/steamrt4/    Steam Linux Runtime 4
//! playtime/<slug>.json play time per game
//! tools/               pinned helpers: winetricks, the process reaper
//! cache/               release lists and other refetchable data
//! installs.json        which games are installed where, with which Proton
//! settings.json        launcher-wide settings
//! ```

use crate::games::GameId;
use crate::steamrt::Runtime;
use std::env;
use std::path::PathBuf;

#[derive(Debug, Clone)]
pub struct Paths {
    pub root: PathBuf,
}

impl Paths {
    pub fn new(root: PathBuf) -> Self {
        Self { root }
    }

    /// `$DECANTER_HOME`, else `$XDG_DATA_HOME/decanter`, else `~/.local/share/decanter`.
    pub fn default_root() -> PathBuf {
        if let Some(dir) = env::var_os("DECANTER_HOME").filter(|v| !v.is_empty()) {
            return PathBuf::from(dir);
        }
        if let Some(dir) = env::var_os("XDG_DATA_HOME").filter(|v| !v.is_empty()) {
            return PathBuf::from(dir).join("decanter");
        }
        home().join(".local/share/decanter")
    }

    pub fn game_default_dir(&self, game: GameId) -> PathBuf {
        self.root.join("games").join(game.info().slug)
    }

    pub fn prefix_default_dir(&self, game: GameId) -> PathBuf {
        self.root.join("prefixes").join(game.info().slug)
    }

    pub fn runners_dir(&self) -> PathBuf {
        self.root.join("runners")
    }

    pub fn runner_dir(&self, name: &str) -> PathBuf {
        self.runners_dir().join(name)
    }

    pub fn steamrt_dir(&self, runtime: Runtime) -> PathBuf {
        self.root.join("steamrt").join(runtime.folder())
    }

    pub fn playtime_file(&self, game: GameId) -> PathBuf {
        self.root.join("playtime").join(format!("{}.json", game.info().slug))
    }

    pub fn cache_dir(&self) -> PathBuf {
        self.root.join("cache")
    }

    pub fn installs_file(&self) -> PathBuf {
        self.root.join("installs.json")
    }

    pub fn settings_file(&self) -> PathBuf {
        self.root.join("settings.json")
    }
}

pub fn home() -> PathBuf {
    env::var_os("HOME").map(PathBuf::from).unwrap_or_else(|| PathBuf::from("/"))
}

/// Write `bytes` to `path` through a temp file + rename, so readers never see
/// a half-written file. Creates the parent folder.
pub fn write_atomic(path: &std::path::Path, bytes: &[u8]) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let mut temp = path.as_os_str().to_owned();
    temp.push(".tmp");
    let temp = PathBuf::from(temp);
    std::fs::write(&temp, bytes)?;
    std::fs::rename(&temp, path)
}
