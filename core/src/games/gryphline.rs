//! Gryphline's launcher API (Arknights: Endfield, global).
//!
//! The game ships as one big zip split into ~1 GB parts (`.zip.001`, ...),
//! each with an md5. fischl downloads and verifies the parts into
//! `<dir>/downloading/staging`, then merges and extracts them into the
//! install folder.

use super::{Art, GameId, Latest, Plan as GamePlan, block_on, clear_record, write_record};
use crate::net;
use crate::progress::{Phase, Progress};
use anyhow::{Context, Result, bail};
use fischl::download::game::{Game, Zipped};
use serde::Deserialize;
use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

const APPCODE: &str = "YDUTE5gscDZ229CW";
const LATEST_URL: &str = "https://launcher.gryphline.com/api/game/get_latest?appcode=YDUTE5gscDZ229CW&channel=6&sub_channel=6";
const PROXY_URL: &str = "https://launcher.gryphline.com/api/proxy/web/batch_proxy";

#[derive(Debug, Clone)]
pub struct Part {
    pub url: String,
    pub md5: String,
    pub size: u64,
}

#[derive(Debug)]
pub struct Plan {
    pub parts: Vec<Part>,
}

#[derive(Deserialize)]
struct LatestReply {
    version: String,
    pkg: Pkg,
}

#[derive(Deserialize)]
struct Pkg {
    packs: Vec<PackPart>,
    #[serde(default)]
    total_size: String,
    #[serde(default)]
    game_files_md5: String,
}

#[derive(Deserialize)]
struct PackPart {
    url: String,
    md5: String,
    package_size: String,
}

#[derive(Deserialize)]
struct ProxyReply {
    proxy_rsps: Vec<ProxyRsp>,
}

#[derive(Deserialize)]
struct ProxyRsp {
    #[serde(default)]
    get_main_bg_image_rsp: Option<BgRsp>,
}

#[derive(Deserialize)]
struct BgRsp {
    main_bg_image: BgImage,
}

#[derive(Deserialize)]
struct BgImage {
    url: String,
    #[serde(default)]
    video_url: String,
}

pub fn latest(agent: &ureq::Agent, game: GameId) -> Result<Latest> {
    let reply: LatestReply = net::get_json(agent, LATEST_URL)?;
    if reply.pkg.packs.is_empty() {
        bail!("Gryphline listed no download parts");
    }
    let mut parts = Vec::new();
    for p in reply.pkg.packs {
        if !p.url.starts_with("https://") || p.md5.len() != 32 {
            bail!("unexpected download part {:?}", p.url);
        }
        parts.push(Part { size: p.package_size.parse().context("part size")?, url: p.url, md5: p.md5 });
    }
    let download: u64 = parts.iter().map(|p| p.size).sum();
    let install: u64 = reply.pkg.total_size.parse().unwrap_or(download);
    // The md5 of the whole file list changes with any change to the game.
    let fingerprint = if reply.pkg.game_files_md5.is_empty() { parts[0].md5.clone() } else { reply.pkg.game_files_md5 };
    Ok(Latest {
        game,
        build_id: format!("{} {fingerprint}", reply.version),
        version: reply.version,
        download_size: download,
        install_size: install,
        plan: GamePlan::Gryphline(Plan { parts }),
    })
}

pub fn art(agent: &ureq::Agent, _game: GameId) -> Result<Art> {
    let req = serde_json::json!({ "proxy_reqs": [{
        "kind": "get_main_bg_image",
        "get_main_bg_image_req": {
            "appcode": APPCODE, "language": "en-us", "channel": "6",
            "sub_channel": "6", "platform": "Windows", "source": "launcher"
        }
    }]});
    let reply: ProxyReply = net::post_json(agent, PROXY_URL, &req)?;
    let bg = reply
        .proxy_rsps
        .into_iter()
        .find_map(|r| r.get_main_bg_image_rsp)
        .context("no background in the reply")?
        .main_bg_image;
    Ok(Art {
        background: Some(bg.url).filter(|u| !u.is_empty()),
        video: Some(bg.video_url).filter(|u| !u.is_empty()),
        icon: None,
    })
}

pub fn install(plan: &Plan, build_id: &str, short: &str, dir: &Path, progress: &Arc<Progress>) -> Result<()> {
    if super::installed_build(dir).as_deref() == Some(build_id) {
        return Ok(());
    }
    std::fs::create_dir_all(dir)?;
    clear_record(dir);
    let total: u64 = plan.parts.iter().map(|p| p.size).sum();
    let finished = Arc::new(AtomicU64::new(0));
    progress.set_bars(0, total, 0, 0);
    progress.set_phase(Phase::Downloading);
    let dir_str = dir.to_string_lossy().into_owned();

    for (i, part) in plan.parts.iter().enumerate() {
        progress.set_label(format!("Downloading {short} part {} of {}", i + 1, plan.parts.len()));
        let cb = {
            let finished = Arc::clone(&finished);
            let progress = Arc::clone(progress);
            move |current: u64, _total: u64, net: u64, disk: u64| {
                progress.set_bars(finished.load(Ordering::Relaxed) + current, total, 0, 0);
                progress.report_speeds(net, disk);
            }
        };
        let ok = block_on(<Game as Zipped>::download(
            part.url.clone(),
            part.md5.clone(),
            dir_str.clone(),
            false,
            false,
            cb,
            Some(progress.cancel_token()),
            None,
        ))?;
        if progress.cancelled() {
            bail!("paused");
        }
        if !ok {
            bail!("{short}: part {} failed to download (network, disk, or checksum); try again to resume", i + 1);
        }
        finished.fetch_add(part.size, Ordering::Relaxed);
    }

    // The first part's name, as fischl saved it (the URL's last segment).
    let first_name = plan.parts[0].url.rsplit('/').next().context("part URL has no file name")?;
    let staging = dir.join("downloading").join("staging");
    let first = staging.join(first_name);
    if !first.exists() {
        bail!("{short}: expected {} after downloading", first.display());
    }
    progress.set_phase(Phase::Installing);
    progress.set_label(format!("Extracting {short}"));
    let extracted = {
        let progress = Arc::clone(progress);
        fischl::utils::extract_archive_with_progress(
            first.to_string_lossy().into_owned(),
            dir_str.clone(),
            false,
            None,
            move |current, total| progress.set_install(current, total),
        )
    };
    if !extracted {
        bail!("{short}: extracting the game failed; check free disk space and try again");
    }
    let _ = std::fs::remove_dir_all(dir.join("downloading"));
    write_record(dir, build_id)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `cargo test -p decanter-core live_gryphline -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn live_gryphline() {
        let agent = net::agent();
        let l = latest(&agent, GameId::Endfield).unwrap();
        let a = art(&agent, GameId::Endfield).unwrap();
        println!("{} {} parts={} {:.1} GB art={:?}", l.version, l.build_id, match &l.plan { GamePlan::Gryphline(p) => p.parts.len(), _ => 0 }, l.download_size as f64 / 1e9, a.background);
        assert!(a.background.is_some());
    }
}
