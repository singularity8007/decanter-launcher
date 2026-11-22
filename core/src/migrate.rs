//! First start: adopt the ZZZ-only launcher's setup in `~/wine_launcher`.
//!
//! Nothing there is moved, changed or deleted:
//! - the ZZZ install and its prefix are used where they are (their paths go
//!   into `installs.json`);
//! - GE-Proton10-34 and the Steam Linux Runtimes are linked into Decanter's
//!   folders with symlinks (removing or updating them later only replaces
//!   the link);
//! - play time and launch switches are copied.

use crate::games::{self, GameId};
use crate::installs::{Install, State};
use crate::paths::Paths;
use crate::runners::DEFAULT_RUNNER;
use crate::settings::{self, Settings};
use crate::steamrt::Runtime;
use serde::Deserialize;
use std::collections::BTreeMap;
use std::fs;
use std::os::unix::fs::symlink;
use std::path::{Path, PathBuf};

/// The old launcher's settings file (only the keys worth carrying over).
#[derive(Deserialize, Default)]
#[serde(default)]
struct OldSettings {
    #[serde(alias = "proton_folder")]
    runner_dir: Option<String>,
    steamrt_dir: Option<String>,
    prefix_dir: Option<String>,
    game_dir: Option<String>,
    stats_path: Option<String>,
    wayland: Option<bool>,
    hud: Option<bool>,
    dx12: Option<bool>,
    performance_mode: Option<bool>,
    fps_cap: Option<u32>,
}

/// What happened, for the log.
#[derive(Debug, Default)]
pub struct Report {
    pub adopted_game: Option<PathBuf>,
    pub linked: Vec<String>,
}

pub fn legacy_root() -> PathBuf {
    crate::paths::home().join("wine_launcher")
}

/// Build the first `State` (and settings) from `legacy`, if it holds a
/// finished ZZZ install. Returns None when there's nothing to adopt.
pub fn adopt(paths: &Paths, legacy: &Path) -> Option<(State, Settings, Report)> {
    let old: OldSettings = fs::read_to_string(legacy.join("settings.json"))
        .ok()
        .and_then(|t| serde_json::from_str(&t).ok())
        .unwrap_or_default();
    let pick = |saved: &Option<String>, default: PathBuf| match saved.as_deref().map(str::trim) {
        Some(p) if !p.is_empty() => PathBuf::from(p),
        _ => default,
    };

    let game_dir = pick(&old.game_dir, legacy.join("game"));
    if !games::is_complete(&game_dir) {
        return None;
    }
    let prefix_dir = pick(&old.prefix_dir, legacy.join("prefix"));
    let runner_src = pick(&old.runner_dir, legacy.join("proton").join("ge-proton"));
    let sniper_src = pick(&old.steamrt_dir, legacy.join("steamrt").join("steamrt3"));
    let rt4_src = legacy.join("steamrt").join("steamrt4");
    let stats_src = pick(&old.stats_path, legacy.join("playtime.json"));

    let mut report = Report { adopted_game: Some(game_dir.clone()), ..Report::default() };

    // The runner keeps its real name so it shows up correctly in the list.
    let runner_name = fs::read_to_string(runner_src.join("version"))
        .ok()
        .and_then(|v| v.split_whitespace().find(|w| w.starts_with("GE-Proton")).map(str::to_string))
        .filter(|n| crate::runners::safe_name(n))
        .unwrap_or_else(|| DEFAULT_RUNNER.to_string());
    if runner_src.join("proton").exists() && link(&runner_src, &paths.runner_dir(&runner_name)) {
        report.linked.push(runner_name.clone());
    }
    for (src, rt) in [(sniper_src, Runtime::Sniper), (rt4_src, Runtime::Steamrt4)] {
        if src.join("_v2-entry-point").exists() && link(&src, &paths.steamrt_dir(rt)) {
            report.linked.push(rt.folder().to_string());
        }
    }

    let dest_stats = paths.playtime_file(GameId::Zzz);
    if stats_src.exists() && !dest_stats.exists() {
        if let Some(parent) = dest_stats.parent() {
            let _ = fs::create_dir_all(parent);
        }
        let _ = fs::copy(&stats_src, &dest_stats);
    }

    let defaults = Settings::default();
    let settings = Settings {
        wayland: old.wayland.unwrap_or(defaults.wayland),
        hud: old.hud.unwrap_or(defaults.hud),
        performance_mode: old.performance_mode.unwrap_or(defaults.performance_mode),
        fps_cap: old.fps_cap.unwrap_or(defaults.fps_cap),
        ..defaults
    };

    let install = Install {
        game: GameId::Zzz,
        dir: game_dir,
        prefix: prefix_dir,
        runner: runner_name,
        graphics: old.dx12.unwrap_or(false).then(|| "-use-d3d12".to_string()),
        voice: None,
        extra_args: String::new(),
        env: BTreeMap::new(),
    };
    let state = State { installs: vec![install], selected: Some(GameId::Zzz) };
    Some((state, settings, report))
}

/// Symlink `dest` -> `src` unless `dest` already exists. True if linked.
fn link(src: &Path, dest: &Path) -> bool {
    if fs::symlink_metadata(dest).is_ok() {
        return false;
    }
    if let Some(parent) = dest.parent() {
        if fs::create_dir_all(parent).is_err() {
            return false;
        }
    }
    let src = fs::canonicalize(src).unwrap_or_else(|_| src.to_path_buf());
    symlink(&src, dest).is_ok()
}

/// Load the saved state, adopting the old launcher's setup on first start.
pub fn load_or_adopt(paths: &Paths, legacy: Option<&Path>) -> std::io::Result<(State, Settings)> {
    let settings_path = paths.settings_file();
    if let Some(state) = crate::installs::load(&paths.installs_file())? {
        return Ok((state, settings::load(&settings_path)));
    }
    let (state, settings) = match legacy.and_then(|l| adopt(paths, l)) {
        Some((state, settings, report)) => {
            log::info!("adopted the ZZZ-only launcher's setup: {report:?}");
            if !settings_path.exists() {
                settings::save(&settings_path, &settings)?;
            }
            (state, settings::load(&settings_path))
        }
        None => (State::default(), settings::load(&settings_path)),
    };
    crate::installs::save(&paths.installs_file(), &state)?;
    Ok((state, settings))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A fake old setup in a temp folder, adopted into a temp data root.
    #[test]
    fn adopts_without_touching_old_files() {
        let tmp = tempfile::tempdir().unwrap();
        let legacy = tmp.path().join("wine_launcher");
        let game = legacy.join("game");
        let ge = legacy.join("proton/ge-proton");
        let rt = legacy.join("steamrt/steamrt3");
        fs::create_dir_all(&game).unwrap();
        fs::create_dir_all(&ge).unwrap();
        fs::create_dir_all(&rt).unwrap();
        fs::write(game.join(games::RECORD_NAME), "3.3.0 manifest_a").unwrap();
        fs::write(ge.join("proton"), "").unwrap();
        fs::write(ge.join("version"), "1763000000 GE-Proton10-34\n").unwrap();
        fs::write(rt.join("_v2-entry-point"), "").unwrap();
        fs::write(legacy.join("playtime.json"), r#"{"total_seconds":23487,"last_played":"2026-11-20"}"#).unwrap();
        fs::write(legacy.join("settings.json"), r#"{"runner_dir":"","hud":true,"dx12":true,"fps_cap":138}"#).unwrap();

        let paths = Paths::new(tmp.path().join("data"), "/reaper".into());
        let (state, settings, report) = adopt(&paths, &legacy).unwrap();
        let zzz = state.get(GameId::Zzz).unwrap();
        assert_eq!(zzz.dir, game);
        assert_eq!(zzz.prefix, legacy.join("prefix"));
        assert_eq!(zzz.runner, "GE-Proton10-34");
        assert_eq!(zzz.graphics.as_deref(), Some("-use-d3d12"));
        assert!(settings.hud);
        assert_eq!(settings.fps_cap, 138);
        assert_eq!(report.linked, ["GE-Proton10-34", "steamrt3"]);
        assert!(fs::symlink_metadata(paths.runner_dir("GE-Proton10-34")).unwrap().file_type().is_symlink());
        assert!(paths.runner_dir("GE-Proton10-34").join("proton").exists());
        assert_eq!(fs::read_to_string(paths.playtime_file(GameId::Zzz)).unwrap(), fs::read_to_string(legacy.join("playtime.json")).unwrap());
        // The old files are untouched.
        assert!(ge.join("proton").exists() && !fs::symlink_metadata(&ge).unwrap().file_type().is_symlink());
    }

    #[test]
    fn nothing_to_adopt_without_a_finished_install() {
        let tmp = tempfile::tempdir().unwrap();
        let paths = Paths::new(tmp.path().join("data"), "/reaper".into());
        assert!(adopt(&paths, &tmp.path().join("nope")).is_none());
    }
}
