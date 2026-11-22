//! One-time Windows components (winetricks verbs) a game's prefix needs,
//! e.g. vcrun2022 for Wuthering Waves and Honkai: Star Rail.
//!
//! winetricks is the upstream script, pinned to one release and verified by
//! its SHA-256 before it ever runs. It downloads Microsoft's redistributables
//! and checks each one against the hash written in the script. It runs with
//! the game's own Proton build (its bundled Wine) against the game's prefix,
//! and every verb that finishes is recorded so it never runs twice.

use crate::paths::{Paths, write_atomic};
use crate::progress::{Phase, Progress};
use anyhow::{Context, Result, bail};
use sha2::{Digest, Sha256};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

const WINETRICKS_TAG: &str = "20260125";
const WINETRICKS_SHA256: &str = "431f82fc74000e6c864409f1d8fb495d696c03928808e3e8acffc45179312a7b";

/// Verbs already installed, one per line, inside the Proton data folder.
const RECORD: &str = ".decanter_components";

/// The verbs from `wanted` this prefix doesn't have yet.
pub fn missing<'a>(prefix_dir: &Path, wanted: &[&'a str]) -> Vec<&'a str> {
    let done = fs::read_to_string(prefix_dir.join(RECORD)).unwrap_or_default();
    wanted.iter().copied().filter(|v| !done.lines().any(|l| l.trim() == *v)).collect()
}

fn record(prefix_dir: &Path, verbs: &[&str]) -> Result<()> {
    let mut done = fs::read_to_string(prefix_dir.join(RECORD)).unwrap_or_default();
    for v in verbs {
        if !done.lines().any(|l| l.trim() == *v) {
            done.push_str(v);
            done.push('\n');
        }
    }
    write_atomic(&prefix_dir.join(RECORD), done.as_bytes())?;
    Ok(())
}

/// The pinned winetricks script, downloaded once and verified every time.
fn winetricks(agent: &ureq::Agent, paths: &Paths) -> Result<PathBuf> {
    let path = paths.root.join("tools").join(format!("winetricks-{WINETRICKS_TAG}"));
    let ok = |bytes: &[u8]| format!("{:x}", Sha256::digest(bytes)) == WINETRICKS_SHA256;
    if fs::read(&path).is_ok_and(|b| ok(&b)) {
        return Ok(path);
    }
    let url = format!("https://raw.githubusercontent.com/Winetricks/winetricks/{WINETRICKS_TAG}/src/winetricks");
    let bytes = crate::net::get_bytes(agent, &url, 4)?;
    if !ok(&bytes) {
        bail!("the downloaded winetricks doesn't match the pinned checksum; refusing to run it");
    }
    write_atomic(&path, &bytes)?;
    Ok(path)
}

fn have(tool: &str) -> bool {
    Command::new(tool).arg("--version").stdout(Stdio::null()).stderr(Stdio::null()).status().is_ok()
}

/// Install `verbs` into the prefix at `prefix_dir` using the Wine inside
/// `runner_dir`. The prefix must already exist (Proton creates it).
pub fn install(agent: &ureq::Agent, paths: &Paths, prefix_dir: &Path, runner_dir: &Path, verbs: &[&str], progress: &Progress) -> Result<()> {
    let todo = missing(prefix_dir, verbs);
    if todo.is_empty() {
        return Ok(());
    }
    if !have("cabextract") {
        bail!("winetricks needs cabextract to set up this game. Install it with: sudo apt install cabextract");
    }
    progress.set_phase(Phase::Installing);
    progress.set_bars(0, 0, 0, 0);
    progress.set_label(format!("Installing Windows components: {} (one time only)", todo.join(", ")));
    let script = winetricks(agent, paths)?;
    let wine = runner_dir.join("files/bin/wine");
    let wineserver = runner_dir.join("files/bin/wineserver");
    if !wine.exists() {
        bail!("{} has no bundled Wine at files/bin/wine", runner_dir.display());
    }
    let log_path = prefix_dir.join("winetricks.log");
    let log = fs::File::create(&log_path).context("creating winetricks.log")?;
    let status = Command::new("sh")
        .arg(&script)
        .arg("--unattended")
        .args(&todo)
        .env("WINEPREFIX", prefix_dir.join("pfx"))
        .env("WINE", &wine)
        .env("WINESERVER", &wineserver)
        .env("WINEARCH", "win64")
        .env("W_CACHE", paths.cache_dir().join("winetricks"))
        .env("WINETRICKS_LATEST_VERSION_CHECK", "disabled")
        .env("WINEDEBUG", "-all")
        // No Mono/Gecko install prompts.
        .env("WINEDLLOVERRIDES", "mscoree,mshtml=")
        .stdin(Stdio::null())
        .stdout(log.try_clone()?)
        .stderr(log)
        .status()
        .context("running winetricks")?;
    // Let the prefix's wineserver finish writing before Proton takes over.
    let _ = Command::new(&wineserver).arg("-w").env("WINEPREFIX", prefix_dir.join("pfx")).status();
    if !status.success() {
        let tail: String = fs::read_to_string(&log_path)
            .unwrap_or_default()
            .lines()
            .rev()
            .take(4)
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .collect::<Vec<_>>()
            .join(" | ");
        bail!("winetricks failed ({status}): {tail}  (full log: {})", log_path.display());
    }
    record(prefix_dir, &todo)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn records_and_reports_missing() {
        let tmp = tempfile::tempdir().unwrap();
        assert_eq!(missing(tmp.path(), &["vcrun2022", "corefonts"]), ["vcrun2022", "corefonts"]);
        record(tmp.path(), &["vcrun2022"]).unwrap();
        assert_eq!(missing(tmp.path(), &["vcrun2022", "corefonts"]), ["corefonts"]);
        record(tmp.path(), &["vcrun2022", "corefonts"]).unwrap();
        assert!(missing(tmp.path(), &["vcrun2022", "corefonts"]).is_empty());
        assert_eq!(fs::read_to_string(tmp.path().join(RECORD)).unwrap(), "vcrun2022\ncorefonts\n");
    }

    /// Downloads the pinned script and checks the hash.
    /// `cargo test -p decanter-core live_winetricks -- --ignored`
    #[test]
    #[ignore]
    fn live_winetricks() {
        let tmp = tempfile::tempdir().unwrap();
        let paths = Paths::new(tmp.path().to_path_buf(), "/reaper".into());
        let p = winetricks(&crate::net::agent(), &paths).unwrap();
        assert!(fs::read_to_string(p).unwrap().contains("WINETRICKS_VERSION=20260125"));
    }
}
