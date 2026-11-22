//! Which games are installed where, and how each one is launched.
//! Kept in `installs.json` in the data root.

use crate::games::GameId;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Install {
    pub game: GameId,
    /// Game folder.
    pub dir: PathBuf,
    /// Proton data path (STEAM_COMPAT_DATA_PATH); the Wine prefix is `<prefix>/pfx`.
    pub prefix: PathBuf,
    /// Installed runner folder name, e.g. "GE-Proton10-34".
    pub runner: String,
    /// Graphics API arguments; None = the game's default.
    #[serde(default)]
    pub graphics: Option<String>,
    /// HoYo voice pack; None = the default.
    #[serde(default)]
    pub voice: Option<String>,
    /// Extra arguments appended after the exe.
    #[serde(default)]
    pub extra_args: String,
    /// Extra environment variables for this game only.
    #[serde(default)]
    pub env: BTreeMap<String, String>,
}

impl Install {
    pub fn graphics_args(&self) -> &str {
        self.graphics.as_deref().unwrap_or(self.game.info().default_graphics)
    }

    pub fn exe(&self) -> PathBuf {
        self.dir.join(self.game.info().exe)
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct State {
    #[serde(default)]
    pub installs: Vec<Install>,
    /// The game the home screen shows.
    #[serde(default)]
    pub selected: Option<GameId>,
}

impl State {
    pub fn get(&self, game: GameId) -> Option<&Install> {
        self.installs.iter().find(|i| i.game == game)
    }

    pub fn get_mut(&mut self, game: GameId) -> Option<&mut Install> {
        self.installs.iter_mut().find(|i| i.game == game)
    }

    /// Add or replace the install for `install.game`.
    pub fn put(&mut self, install: Install) {
        match self.get_mut(install.game) {
            Some(existing) => *existing = install,
            None => self.installs.push(install),
        }
    }

    pub fn remove(&mut self, game: GameId) {
        self.installs.retain(|i| i.game != game);
    }
}

pub fn load(path: &Path) -> io::Result<Option<State>> {
    match fs::read(path) {
        Ok(bytes) => serde_json::from_slice(&bytes).map(Some).map_err(io::Error::other),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e),
    }
}

pub fn save(path: &Path, state: &State) -> io::Result<()> {
    let text = serde_json::to_vec_pretty(state).map_err(io::Error::other)?;
    crate::paths::write_atomic(path, &text)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn zzz() -> Install {
        Install {
            game: GameId::Zzz,
            dir: "/g".into(),
            prefix: "/p".into(),
            runner: "GE-Proton10-34".into(),
            graphics: None,
            voice: None,
            extra_args: String::new(),
            env: BTreeMap::new(),
        }
    }

    #[test]
    fn put_replaces() {
        let mut s = State::default();
        s.put(zzz());
        let mut other = zzz();
        other.runner = "x".into();
        s.put(other);
        assert_eq!(s.installs.len(), 1);
        assert_eq!(s.get(GameId::Zzz).unwrap().runner, "x");
    }

    #[test]
    fn graphics_default() {
        let mut i = zzz();
        assert_eq!(i.graphics_args(), "");
        i.graphics = Some("-use-d3d12".into());
        assert_eq!(i.graphics_args(), "-use-d3d12");
    }

    #[test]
    fn missing_file_is_none_and_round_trips() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("installs.json");
        assert_eq!(load(&path).unwrap(), None);
        let mut s = State::default();
        s.put(zzz());
        s.selected = Some(GameId::Zzz);
        save(&path, &s).unwrap();
        assert_eq!(load(&path).unwrap(), Some(s));
    }
}
