//! The process reaper that sits between the Steam Linux Runtime and Proton
//! (`reaper SteamLaunch AppId=0 -- proton ...`) and cleans up processes the
//! game leaves behind.
//!
//! Decanter uses the reaper TwintailLauncher ships, so the launch chain stays
//! identical to Twintail's. It isn't part of this repository: it's downloaded
//! once from Twintail's GPL-3.0 repository at a pinned commit, and its
//! SHA-256 is checked every time before it runs.

use crate::paths::{Paths, write_atomic};
use anyhow::{Context, Result, bail};
use sha2::{Digest, Sha256};
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;

/// TwintailLauncher commit the reaper is taken from.
const REV: &str = "53bb67939890c193e3450a91389b1d9bd9be519c";
const SHA256: &str = "61881279e7e517b8fded22866289384840b81f0c3284d4cf741416c41f1d2e93";

/// Where the reaper lives once downloaded.
pub fn path(paths: &Paths) -> PathBuf {
    paths.root.join("tools").join(format!("reaper-{}", &REV[..12]))
}

fn verified(bytes: &[u8]) -> bool {
    format!("{:x}", Sha256::digest(bytes)) == SHA256
}

/// Whether the downloaded reaper is there and matches the pinned checksum.
pub fn ready(paths: &Paths) -> bool {
    fs::read(path(paths)).is_ok_and(|b| verified(&b))
}

/// Download the pinned reaper if it's missing or doesn't verify.
pub fn ensure(agent: &ureq::Agent, paths: &Paths) -> Result<PathBuf> {
    let path = path(paths);
    if ready(paths) {
        return Ok(path);
    }
    let url = format!("https://raw.githubusercontent.com/TwintailTeam/TwintailLauncher/{REV}/src-tauri/resources/reaper");
    let bytes = crate::net::get_bytes(agent, &url, 4).context("downloading the process reaper")?;
    if !verified(&bytes) {
        bail!("the downloaded reaper doesn't match the pinned checksum; refusing to run it");
    }
    write_atomic(&path, &bytes)?;
    fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).context("making the reaper executable")?;
    Ok(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `cargo test -p decanter-core live_reaper -- --ignored`
    #[test]
    #[ignore]
    fn live_reaper() {
        let tmp = tempfile::tempdir().unwrap();
        let paths = Paths::new(tmp.path().to_path_buf());
        let p = ensure(&crate::net::agent(), &paths).unwrap();
        assert!(ready(&paths));
        assert_eq!(fs::metadata(&p).unwrap().permissions().mode() & 0o111, 0o111);
    }
}
