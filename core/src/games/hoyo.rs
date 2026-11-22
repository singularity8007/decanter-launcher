//! HoYoverse's launcher API (global servers): current build, Sophon download
//! plan, and the home-screen artwork.

use super::{Art, GameId, Latest, Plan as GamePlan};
use crate::net;
use anyhow::{Context, Result, bail};
use serde::Deserialize;
use serde::de::DeserializeOwned;
use std::collections::HashSet;

const LAUNCHER_ID: &str = "VYTpXlbWo8";
const API: &str = "https://sg-hyp-api.hoyoverse.com/hyp/hyp-connect/api";
const BUILD_URL: &str = "https://sg-downloader-api.hoyoverse.com/downloader/sophon_chunk/api/getBuild";

/// HoYo's id for each game on the global launcher.
fn game_id(game: GameId) -> &'static str {
    match game {
        GameId::Zzz => "U5hbdsT9W7",
        GameId::Hsr => "4ziysqXOQ8",
        GameId::Genshin => "gopR6Cufr3",
        other => unreachable!("{other:?} isn't a HoYoverse game"),
    }
}

/// One downloadable piece: the base game, a voice pack, a chapter...
#[derive(Debug)]
pub struct Item {
    pub name: String,
    pub manifest_url: String,
    /// Chunks live at `<chunk_url_prefix>/<chunk id>`.
    pub chunk_url_prefix: String,
    pub compressed: u64,
    pub uncompressed: u64,
}

#[derive(Debug)]
pub struct Plan {
    pub tag: String,
    pub items: Vec<Item>,
}

// ---- API reply shapes. Only the fields we use; serde ignores the rest. ----

#[derive(Deserialize)]
struct Reply<T> {
    retcode: i64,
    message: String,
    data: Option<T>,
}

#[derive(Deserialize)]
struct BranchesData {
    game_branches: Vec<GameBranch>,
}

#[derive(Deserialize)]
struct GameBranch {
    main: Branch,
}

#[derive(Deserialize)]
struct Branch {
    package_id: String,
    branch: String,
    password: String,
    categories: Vec<Category>,
}

#[derive(Deserialize)]
struct Category {
    matching_field: String,
    #[serde(default)]
    scenarios: Vec<String>,
}

#[derive(Deserialize)]
struct BuildData {
    tag: String,
    manifests: Vec<BuildManifest>,
}

#[derive(Deserialize)]
struct BuildManifest {
    matching_field: String,
    manifest: ManifestRef,
    manifest_download: Location,
    chunk_download: Location,
    stats: Stats,
}

#[derive(Deserialize)]
struct ManifestRef {
    id: String,
}

#[derive(Deserialize)]
struct Location {
    url_prefix: String,
    encryption: i64,
    compression: i64,
}

// The API sends these numbers as strings.
#[derive(Deserialize)]
struct Stats {
    compressed_size: String,
    uncompressed_size: String,
}

#[derive(Deserialize)]
struct BasicInfoData {
    game_info_list: Vec<BasicInfo>,
}

#[derive(Deserialize)]
struct BasicInfo {
    #[serde(default)]
    backgrounds: Vec<Background>,
}

#[derive(Deserialize)]
struct Background {
    background: Option<Image>,
    video: Option<Image>,
}

#[derive(Deserialize)]
struct Image {
    url: String,
}

#[derive(Deserialize)]
struct GamesData {
    games: Vec<GameEntry>,
}

#[derive(Deserialize)]
struct GameEntry {
    id: String,
    display: Display,
}

#[derive(Deserialize)]
struct Display {
    icon: Option<Image>,
}

/// Which categories a playable install needs. ZZZ marks them with
/// CATEGORY_SCENARIO_BASE (which includes "game"); Genshin and HSR mark
/// nothing as base, and there the "game" category alone is the game. Then
/// the chosen voice pack on top.
fn wanted_categories(categories: &[Category], voice: &str) -> Result<HashSet<String>> {
    let mut wanted: HashSet<String> = categories
        .iter()
        .filter(|c| c.scenarios.iter().any(|s| s == "CATEGORY_SCENARIO_BASE"))
        .map(|c| c.matching_field.clone())
        .collect();
    if wanted.is_empty() {
        wanted.insert("game".to_string());
    }
    if !categories.iter().any(|c| c.matching_field == "game") || !wanted.contains("game") {
        bail!("the base game is missing from the build's categories");
    }
    if !voice.is_empty() {
        if !categories.iter().any(|c| c.matching_field == voice) {
            bail!("voice pack {voice:?} isn't offered for this game");
        }
        wanted.insert(voice.to_string());
    }
    Ok(wanted)
}

/// Ask HoYoverse for the current build and pick what an install needs.
pub fn latest(agent: &ureq::Agent, game: GameId, voice: &str) -> Result<Latest> {
    // `[]` in `game_ids[]` is percent-encoded so the URL parser accepts it.
    let url = format!("{API}/getGameBranches?launcher_id={LAUNCHER_ID}&game_ids%5B%5D={}", game_id(game));
    let branches: BranchesData = get(agent, &url)?;
    let main = branches.game_branches.into_iter().next().context("no game branch in the reply")?.main;
    let wanted = wanted_categories(&main.categories, voice)?;

    let url = format!(
        "{BUILD_URL}?branch={}&package_id={}&password={}",
        main.branch, main.package_id, main.password
    );
    let build: BuildData = get(agent, &url)?;

    let mut items = Vec::new();
    for m in build.manifests {
        if !wanted.contains(&m.matching_field) {
            continue;
        }
        // We only handle plain zstd data. Fail loudly if HoYoverse changes that.
        for loc in [&m.manifest_download, &m.chunk_download] {
            if loc.encryption != 0 || loc.compression != 1 {
                bail!("{}: unsupported encryption/compression", m.matching_field);
            }
        }
        items.push(Item {
            manifest_url: format!("{}/{}", m.manifest_download.url_prefix, m.manifest.id),
            chunk_url_prefix: m.chunk_download.url_prefix,
            compressed: m.stats.compressed_size.parse()?,
            uncompressed: m.stats.uncompressed_size.parse()?,
            name: m.matching_field,
        });
    }
    if items.len() != wanted.len() {
        bail!("wanted {} pieces but the build lists {}", wanted.len(), items.len());
    }

    let plan = Plan { tag: build.tag, items };
    Ok(Latest {
        game,
        version: plan.tag.clone(),
        build_id: build_id(&plan),
        download_size: plan.items.iter().map(|i| i.compressed).sum(),
        install_size: plan.items.iter().map(|i| i.uncompressed).sum(),
        plan: GamePlan::Sophon(plan),
    })
}

/// Tag plus every manifest id. Manifest ids embed a checksum, so this changes
/// whenever any piece changes. Same format the ZZZ-only launcher wrote, so an
/// existing install is recognised as up to date.
pub fn build_id(plan: &Plan) -> String {
    let ids: Vec<&str> = plan.items.iter().filter_map(|i| i.manifest_url.rsplit('/').next()).collect();
    format!("{} {}", plan.tag, ids.join(","))
}

/// The launcher's current key art (and icon) for this game.
pub fn art(agent: &ureq::Agent, game: GameId) -> Result<Art> {
    let url = format!("{API}/getAllGameBasicInfo?launcher_id={LAUNCHER_ID}&language=en-us&game_id={}", game_id(game));
    let info: BasicInfoData = get(agent, &url)?;
    let bg = info.game_info_list.into_iter().next().and_then(|g| g.backgrounds.into_iter().next());

    let url = format!("{API}/getGames?launcher_id={LAUNCHER_ID}&language=en-us");
    let icon = get::<GamesData>(agent, &url)
        .ok()
        .and_then(|d| d.games.into_iter().find(|g| g.id == game_id(game)))
        .and_then(|g| g.display.icon)
        .map(|i| i.url);

    Ok(Art {
        background: bg.as_ref().and_then(|b| b.background.as_ref()).map(|i| i.url.clone()),
        video: bg.as_ref().and_then(|b| b.video.as_ref()).map(|i| i.url.clone()),
        icon,
    })
}

/// GET a HoYo API URL and return its `data`, or an error if the API refused.
fn get<T: DeserializeOwned>(agent: &ureq::Agent, url: &str) -> Result<T> {
    let reply: Reply<T> = net::get_json(agent, url)?;
    if reply.retcode != 0 {
        bail!("HoYo API error {}: {}", reply.retcode, reply.message);
    }
    reply.data.context("the reply had no data")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cat(field: &str, base: bool) -> Category {
        let mut scenarios = vec!["CATEGORY_SCENARIO_FULL".to_string()];
        if base {
            scenarios.push("CATEGORY_SCENARIO_BASE".to_string());
        }
        Category { matching_field: field.into(), scenarios }
    }

    #[test]
    fn zzz_uses_base_scenario() {
        let cats = vec![cat("game", true), cat("10101", true), cat("10108", false), cat("en-us", false)];
        let w = wanted_categories(&cats, "en-us").unwrap();
        let mut v: Vec<_> = w.into_iter().collect();
        v.sort();
        assert_eq!(v, ["10101", "en-us", "game"]);
    }

    #[test]
    fn genshin_falls_back_to_game() {
        let cats = vec![cat("game", false), cat("en-us", false), cat("ja-jp", false)];
        let w = wanted_categories(&cats, "ja-jp").unwrap();
        let mut v: Vec<_> = w.into_iter().collect();
        v.sort();
        assert_eq!(v, ["game", "ja-jp"]);
    }

    #[test]
    fn unknown_voice_is_an_error() {
        let cats = vec![cat("game", false)];
        assert!(wanted_categories(&cats, "fr-fr").is_err());
        assert_eq!(wanted_categories(&cats, "").unwrap().len(), 1);
    }

    /// Live: the API still answers in the shape we expect for all three games.
    /// `cargo test -p decanter-core live_hoyo -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn live_hoyo() {
        let agent = net::agent();
        for g in [GameId::Zzz, GameId::Hsr, GameId::Genshin] {
            let l = latest(&agent, g, "en-us").unwrap();
            let a = art(&agent, g).unwrap();
            println!("{g:?}: {} ({} pieces, {:.1} GB) art={:?}", l.version, match &l.plan { GamePlan::Sophon(p) => p.items.len(), _ => 0 }, l.download_size as f64 / 1e9, a.background);
            assert!(a.background.is_some());
        }
    }
}

#[cfg(test)]
mod live_install_check {
    /// The existing ZZZ install must count as up to date (no re-download).
    /// `cargo test -p decanter-core live_zzz_record -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn live_zzz_record() {
        let dir = crate::migrate::legacy_root().join("game");
        let installed = crate::games::installed_build(&dir).expect("no ZZZ install record");
        let latest = super::latest(&crate::net::agent(), super::GameId::Zzz, "en-us").unwrap();
        println!("installed: {}…\nlatest:    {}…", &installed[..60], &latest.build_id[..60]);
        assert_eq!(installed, latest.build_id);
    }
}
