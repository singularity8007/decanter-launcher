//! Kuro Games' launcher index (Wuthering Waves, global).
//!
//! The game ships as resource packs, each a JSON list of plain files with
//! md5s. Decanter installs `common` + `hd` (the quality the `-krqlv=hd` launch
//! argument selects), using fischl's per-file repair: every listed file is
//! checked against its md5 and only missing or changed files are downloaded,
//! straight into place. The same pass is a fresh install, an update, and a
//! resume.

use super::{Art, GameId, Latest, Plan as GamePlan, block_on, clear_record, write_record};
use crate::net;
use crate::progress::{Phase, Progress};
use anyhow::{Context, Result, bail};
use fischl::download::game::{Game, Kuro};
use serde::Deserialize;
use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

const BASE: &str = "https://prod-alicdn-gamestarter.kurogame.com/launcher/game/50004_P7xcUZnEr1AXIGON25E6KjpOgTlVrg6e/G153/official";
const FALLBACK_CDN: &str = "https://hw-pcdownload-aws.aki-game.net";
const PACKS: [&str; 2] = ["common", "hd"];

#[derive(Debug, Clone)]
pub struct Pack {
    pub name: String,
    pub version: String,
    pub index_url: String,
    pub index_md5: String,
    /// Files live at `<base_url>/<dest>` (no trailing slash).
    pub base_url: String,
    pub size: u64,
}

#[derive(Debug)]
pub struct Plan {
    pub packs: Vec<Pack>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Index {
    #[serde(default)]
    cdn_list: Vec<Cdn>,
    resource_packs: HashMap<String, ResourcePack>,
    config: Config,
}

#[derive(Deserialize)]
struct Cdn {
    url: String,
    #[serde(rename = "P", default)]
    priority: i64,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ResourcePack {
    version: String,
    index_file: String,
    base_url: String,
    #[serde(default)]
    index_file_md5: String,
    #[serde(default)]
    size: u64,
    #[serde(default)]
    un_compress_size: u64,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Config {
    function_code: FunctionCode,
}

#[derive(Deserialize)]
struct FunctionCode {
    background: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct BackgroundInfo {
    #[serde(default)]
    first_frame_image: Option<String>,
    #[serde(default)]
    background_file: Option<String>,
    #[serde(default)]
    background_file_type: i64,
}

fn index(agent: &ureq::Agent) -> Result<Index> {
    net::get_json(agent, &format!("{BASE}/index.json"))
}

/// The CDN Kuro weights highest, without a trailing slash.
fn pick_cdn(list: &[Cdn]) -> String {
    list.iter()
        .filter(|c| c.url.starts_with("https://"))
        .max_by_key(|c| c.priority)
        .map(|c| c.url.trim_end_matches('/').to_string())
        .unwrap_or_else(|| FALLBACK_CDN.to_string())
}

fn join(cdn: &str, path: &str) -> String {
    format!("{cdn}/{}", path.trim_start_matches('/'))
}

pub fn latest(agent: &ureq::Agent, game: GameId) -> Result<Latest> {
    let idx = index(agent)?;
    let cdn = pick_cdn(&idx.cdn_list);
    let mut packs = Vec::new();
    for name in PACKS {
        let p = idx.resource_packs.get(name).with_context(|| format!("resource pack {name:?} is missing"))?;
        packs.push(Pack {
            name: name.to_string(),
            version: p.version.clone(),
            index_url: join(&cdn, &p.index_file),
            index_md5: p.index_file_md5.clone(),
            base_url: join(&cdn, &p.base_url).trim_end_matches('/').to_string(),
            size: p.un_compress_size.max(p.size),
        });
    }
    let version = packs[0].version.clone();
    if packs.iter().any(|p| p.version != version) {
        bail!("resource packs disagree on the version");
    }
    let fingerprint: Vec<&str> = packs.iter().map(|p| p.index_md5.as_str()).collect();
    let size: u64 = packs.iter().map(|p| p.size).sum();
    Ok(Latest {
        game,
        build_id: format!("{version} {}", fingerprint.join(",")),
        version,
        download_size: size,
        install_size: size,
        plan: GamePlan::Kuro(Plan { packs }),
    })
}

pub fn art(agent: &ureq::Agent, _game: GameId) -> Result<Art> {
    let idx = index(agent)?;
    let url = format!("{BASE}/background/{}/en.json", idx.config.function_code.background);
    let bg: BackgroundInfo = net::get_json(agent, &url)?;
    Ok(Art {
        background: bg.first_frame_image,
        video: (bg.background_file_type == 2).then_some(bg.background_file).flatten(),
        icon: None,
    })
}

pub fn install(plan: &Plan, build_id: &str, short: &str, dir: &Path, progress: &Arc<Progress>) -> Result<()> {
    if super::installed_build(dir).as_deref() == Some(build_id) {
        return Ok(());
    }
    std::fs::create_dir_all(dir)?;
    clear_record(dir);
    let grand_total: u64 = plan.packs.iter().map(|p| p.size).sum();
    let finished = Arc::new(AtomicU64::new(0)); // bytes of packs already done
    progress.set_bars(0, grand_total, 0, grand_total);

    for pack in &plan.packs {
        progress.set_label(format!("{short} {}: {} files", pack.version, pack.name));
        progress.set_phase(Phase::Verifying);
        let cb = {
            let finished = Arc::clone(&finished);
            // fischl calls this from its own threads; Progress is all atomics + mutexes.
            let progress = Arc::clone(progress);
            move |dl: u64, _dl_total: u64, inst: u64, _inst_total: u64, net: u64, disk: u64, phase: u8| {
                let base = finished.load(Ordering::Relaxed);
                progress.set_bars(base + dl, grand_total, base + inst, grand_total);
                progress.report_speeds(net, disk);
                match phase {
                    1 | 4 => progress.set_phase(Phase::Verifying),
                    2 => progress.set_phase(Phase::Downloading),
                    3 | 5 => progress.set_phase(Phase::Installing),
                    _ => {}
                }
            }
        };
        let ok = block_on(<Game as Kuro>::repair_game(
            pack.index_url.clone(),
            pack.base_url.clone(),
            dir.to_string_lossy().into_owned(),
            false,
            cb,
            Some(progress.cancel_token()),
            None,
            // No keep-list: pruning against one pack's list would delete the other pack's files.
            None,
        ))?;
        if progress.cancelled() {
            bail!("paused");
        }
        if !ok {
            bail!("{short}: downloading the {} pack failed (network or disk); try again to resume", pack.name);
        }
        finished.fetch_add(pack.size, Ordering::Relaxed);
    }
    let _ = std::fs::remove_dir_all(dir.join("repairing"));
    write_record(dir, build_id)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn picks_highest_priority_cdn() {
        let list = vec![
            Cdn { url: "https://a.example/".into(), priority: 10 },
            Cdn { url: "https://b.example/".into(), priority: 99 },
            Cdn { url: "https://c.example/".into(), priority: 0 },
        ];
        assert_eq!(pick_cdn(&list), "https://b.example");
        assert_eq!(pick_cdn(&[]), FALLBACK_CDN);
    }

    #[test]
    fn joins_paths() {
        assert_eq!(join("https://cdn", "launcher/x/zip/"), "https://cdn/launcher/x/zip/");
        assert_eq!(join("https://cdn", "/a"), "https://cdn/a");
    }

    /// `cargo test -p decanter-core live_kuro -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn live_kuro() {
        let agent = net::agent();
        let l = latest(&agent, GameId::Wuwa).unwrap();
        let a = art(&agent, GameId::Wuwa).unwrap();
        println!("{} {} {:.1} GB art={:?}", l.version, l.build_id, l.download_size as f64 / 1e9, a.background);
        if let GamePlan::Kuro(p) = &l.plan {
            for pack in &p.packs {
                let body = net::get_bytes(&agent, &pack.index_url, 64).unwrap();
                assert_eq!(format!("{:x}", <md5::Md5 as md5::Digest>::digest(&body)), pack.index_md5);
                println!("{} ok: {}", pack.name, pack.base_url);
            }
        }
    }
}
