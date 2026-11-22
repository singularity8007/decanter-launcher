//! The games Decanter knows, and one interface over three download systems:
//!
//! | Game                      | Publisher API       | Download engine           |
//! |---------------------------|---------------------|---------------------------|
//! | Zenless Zone Zero         | HoYoverse hyp       | Sophon chunks (ours)      |
//! | Honkai: Star Rail         | HoYoverse hyp       | Sophon chunks (ours)      |
//! | Genshin Impact            | HoYoverse hyp       | Sophon chunks (ours)      |
//! | Wuthering Waves           | Kuro launcher index | fischl `Kuro` (per file)  |
//! | Arknights: Endfield       | Gryphline launcher  | fischl `Zipped` (packs)   |
//!
//! Every engine leaves the same marker in the install folder: `.installed_version`
//! holding a build id (`"<version> <fingerprint>"`), written last and removed
//! when work starts, so an interrupted install is never mistaken for a finished one.

pub mod gryphline;
pub mod hoyo;
pub mod kuro;
pub mod sophon;

use crate::progress::Progress;
use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;

/// Written last into the install folder, see the module docs.
pub const RECORD_NAME: &str = ".installed_version";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GameId {
    Zzz,
    Hsr,
    Genshin,
    Wuwa,
    Endfield,
}

/// A graphics API choice, passed to the game as command-line arguments.
#[derive(Debug, Clone, Copy, Serialize)]
pub struct GraphicsOption {
    pub label: &'static str,
    pub args: &'static str,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Publisher {
    Hoyoverse,
    Kuro,
    Gryphline,
}

/// Everything fixed about a game.
#[derive(Debug, Clone, Serialize)]
pub struct GameInfo {
    pub id: GameId,
    pub name: &'static str,
    pub short: &'static str,
    /// Folder name under games/, prefixes/, playtime/.
    pub slug: &'static str,
    pub publisher: Publisher,
    /// The exe, relative to the install folder.
    pub exe: &'static str,
    /// Proton's STEAM_COMPAT_CONFIG (`gamedrive` also makes the exe go
    /// through the `s:` drive).
    pub compat_config: &'static str,
    /// SteamGameId Proton sees. "0" unless the game has a real Steam id it needs.
    pub steam_game_id: &'static str,
    pub graphics: &'static [GraphicsOption],
    pub default_graphics: &'static str,
    /// Shown in the GUI. The rest are backend-only until they're verified.
    pub visible: bool,
    /// Known gaps, shown to the player before installing.
    pub notes: &'static str,
    /// Proton build a new install starts with.
    pub default_runner: &'static str,
    /// winetricks verbs the prefix needs before the first launch.
    pub components: &'static [&'static str],
}

const NO_GRAPHICS: &[GraphicsOption] = &[];

static GAMES: [GameInfo; 5] = [
    GameInfo {
        id: GameId::Zzz,
        name: "Zenless Zone Zero",
        short: "ZZZ",
        slug: "zzz",
        publisher: Publisher::Hoyoverse,
        exe: "ZenlessZoneZero.exe",
        // Exactly the profile ZZZ runs smoothly with (Twintail's nap_global).
        compat_config: "noxalia,gamedrive",
        steam_game_id: "0",
        graphics: &[
            GraphicsOption { label: "DirectX 11", args: "" },
            GraphicsOption { label: "DirectX 12", args: "-use-d3d12" },
        ],
        default_graphics: "",
        visible: true,
        notes: "",
        default_runner: crate::runners::DEFAULT_RUNNER,
        components: &[],
    },
    GameInfo {
        id: GameId::Hsr,
        name: "Honkai: Star Rail",
        short: "HSR",
        slug: "hsr",
        publisher: Publisher::Hoyoverse,
        exe: "StarRail.exe",
        compat_config: "noxalia",
        steam_game_id: "0",
        graphics: NO_GRAPHICS,
        default_graphics: "",
        visible: true,
        notes: "Not tested on Linux with Decanter yet. Twintail also drops in a closed-source DLL patch for this game; Decanter doesn't ship it, so if it won't start, that's the likely reason.",
        default_runner: crate::runners::DEFAULT_RUNNER,
        components: &["vcrun2022"],
    },
    GameInfo {
        id: GameId::Genshin,
        name: "Genshin Impact",
        short: "Genshin",
        slug: "genshin",
        publisher: Publisher::Hoyoverse,
        exe: "GenshinImpact.exe",
        compat_config: "noxalia",
        steam_game_id: "0",
        graphics: NO_GRAPHICS,
        default_graphics: "",
        visible: true,
        notes: "",
        default_runner: crate::runners::DEFAULT_RUNNER,
        components: &[],
    },
    GameInfo {
        id: GameId::Wuwa,
        name: "Wuthering Waves",
        short: "WuWa",
        slug: "wuwa",
        publisher: Publisher::Kuro,
        exe: "Client/Binaries/Win64/Client-Win64-Shipping.exe",
        compat_config: "noopwr,noxalia",
        steam_game_id: "3513350",
        graphics: &[
            GraphicsOption { label: "DirectX 11", args: "-dx11 -krqlv=hd" },
            GraphicsOption { label: "DirectX 12", args: "-dx12 -krqlv=hd" },
        ],
        default_graphics: "-dx12 -krqlv=hd",
        visible: true,
        notes: "Not tested on Linux with Decanter yet. First launch installs vcrun2022, corefonts and d3dcompiler_47 into its prefix (needs cabextract).",
        default_runner: crate::runners::DEFAULT_RUNNER,
        components: &["vcrun2022", "corefonts", "d3dcompiler_47"],
    },
    GameInfo {
        id: GameId::Endfield,
        name: "Arknights: Endfield",
        short: "Endfield",
        slug: "endfield",
        publisher: Publisher::Gryphline,
        exe: "Endfield.exe",
        compat_config: "noxalia",
        steam_game_id: "0",
        graphics: &[
            GraphicsOption { label: "DirectX 11", args: "-force-d3d11" },
            GraphicsOption { label: "Vulkan", args: "-force-vulkan" },
        ],
        default_graphics: "-force-vulkan",
        visible: true,
        notes: "Ships as ~130 GB of zip parts that are merged before extracting, so it needs about twice that free while installing. Updates re-download the full game for now.",
        default_runner: "proton-twintail-11.0-20261016-slr",
        components: &[],
    },
];

impl GameId {
    pub const ALL: [GameId; 5] = [GameId::Zzz, GameId::Hsr, GameId::Genshin, GameId::Wuwa, GameId::Endfield];

    pub fn info(self) -> &'static GameInfo {
        GAMES.iter().find(|g| g.id == self).expect("every GameId has an entry")
    }
}

/// Official artwork for the home screen, from the publisher's own launcher API.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Art {
    pub background: Option<String>,
    pub video: Option<String>,
    pub icon: Option<String>,
}

/// What the publisher says is current, and how to get it.
#[derive(Debug)]
pub struct Latest {
    pub game: GameId,
    /// Human version, e.g. "3.3.0".
    pub version: String,
    /// Exact build fingerprint; equal build ids mean nothing to download.
    pub build_id: String,
    pub download_size: u64,
    pub install_size: u64,
    pub plan: Plan,
}

#[derive(Debug)]
pub enum Plan {
    Sophon(hoyo::Plan),
    Kuro(kuro::Plan),
    Gryphline(gryphline::Plan),
}

/// Per-install choices that affect what gets downloaded.
#[derive(Debug, Clone, Default)]
pub struct Options {
    /// HoYo voice pack ("en-us", "ja-jp", "ko-kr", "zh-cn", "" = none).
    pub voice: Option<String>,
}

pub const DEFAULT_VOICE: &str = "en-us";

/// Ask the publisher for the current build. Network only; changes nothing.
pub fn fetch_latest(agent: &ureq::Agent, game: GameId, opts: &Options) -> Result<Latest> {
    match game.info().publisher {
        Publisher::Hoyoverse => hoyo::latest(agent, game, opts.voice.as_deref().unwrap_or(DEFAULT_VOICE)),
        Publisher::Kuro => kuro::latest(agent, game),
        Publisher::Gryphline => gryphline::latest(agent, game),
    }
}

pub fn fetch_art(agent: &ureq::Agent, game: GameId) -> Result<Art> {
    match game.info().publisher {
        Publisher::Hoyoverse => hoyo::art(agent, game),
        Publisher::Kuro => kuro::art(agent, game),
        Publisher::Gryphline => gryphline::art(agent, game),
    }
}

/// Bring `dir` to `latest`: a fresh install, an update, or the rest of an
/// interrupted one. Stops early (with an error mentioning "paused") when
/// `progress.cancel()` is called; running it again resumes.
pub fn install(latest: &Latest, dir: &Path, progress: &std::sync::Arc<Progress>) -> Result<()> {
    let short = latest.game.info().short;
    match &latest.plan {
        Plan::Sophon(plan) => sophon::sync(plan, &latest.build_id, short, dir, progress),
        Plan::Kuro(plan) => kuro::install(plan, &latest.build_id, short, dir, progress),
        Plan::Gryphline(plan) => gryphline::install(plan, &latest.build_id, short, dir, progress),
    }
}

/// The build id of a finished install in `dir`, if there is one.
pub fn installed_build(dir: &Path) -> Option<String> {
    fs::read_to_string(dir.join(RECORD_NAME)).ok().map(|s| s.trim().to_string()).filter(|s| !s.is_empty())
}

/// "3.3.0" from a build id like "3.3.0 manifest_…".
pub fn version_of(build_id: &str) -> &str {
    build_id.split_whitespace().next().unwrap_or(build_id)
}

/// True if an install or update has fully finished in `dir`.
pub fn is_complete(dir: &Path) -> bool {
    installed_build(dir).is_some()
}

/// Run a fischl future to completion on a private runtime.
pub(crate) fn block_on<F: std::future::Future>(fut: F) -> Result<F::Output> {
    let rt = tokio::runtime::Builder::new_multi_thread().enable_all().build()?;
    Ok(rt.block_on(fut))
}

/// Record a finished install. Last step of every engine.
pub(crate) fn write_record(dir: &Path, build_id: &str) -> Result<()> {
    crate::paths::write_atomic(&dir.join(RECORD_NAME), build_id.as_bytes())?;
    Ok(())
}

/// Mark an install as unfinished before changing files.
pub(crate) fn clear_record(dir: &Path) {
    let _ = fs::remove_file(dir.join(RECORD_NAME));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_game_has_info() {
        for id in GameId::ALL {
            let info = id.info();
            assert_eq!(info.id, id);
            assert!(!info.exe.is_empty());
            if !info.graphics.is_empty() {
                assert!(info.graphics.iter().any(|g| g.args == info.default_graphics));
            }
        }
    }

    #[test]
    fn version_from_build_id() {
        assert_eq!(version_of("3.3.0 manifest_a,manifest_b"), "3.3.0");
        assert_eq!(version_of("1.6.4"), "1.6.4");
    }
}
