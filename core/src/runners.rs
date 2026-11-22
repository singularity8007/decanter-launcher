//! Proton builds ("runners"): what's published, what's installed, installing
//! and removing them.
//!
//! Releases come straight from each project's GitHub releases. Every asset
//! there carries a `digest` (SHA-256) computed by GitHub, which is what the
//! download is verified against; the project's own checksum file is the
//! fallback for releases older than GitHub digests.

use crate::archive::{self, Expected};
use crate::paths::{Paths, write_atomic};
use crate::progress::Progress;
use crate::steamrt::{self, Runtime};
use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;
use std::time::{Duration, SystemTime};

/// The Proton build ZZZ is known to run smoothly on (the one Twintail uses
/// on this machine). Default for new installs. GE-Proton11 made it choppy.
pub const DEFAULT_RUNNER: &str = "GE-Proton10-34";

/// How long a fetched release list is trusted before asking GitHub again.
/// GitHub allows 60 unauthenticated requests an hour.
const CACHE_FOR: Duration = Duration::from_secs(30 * 60);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Family {
    Ge,
    Cachyos,
    Em,
}

impl Family {
    pub const ALL: [Family; 3] = [Family::Ge, Family::Cachyos, Family::Em];

    pub fn label(self) -> &'static str {
        match self {
            Family::Ge => "Proton GE",
            Family::Cachyos => "Proton CachyOS",
            Family::Em => "Proton EM",
        }
    }

    pub fn repo(self) -> &'static str {
        match self {
            Family::Ge => "GloriousEggroll/proton-ge-custom",
            Family::Cachyos => "CachyOS/proton-cachyos",
            Family::Em => "BananaWorks07/Proton",
        }
    }

    /// The folder name an install of `tag` gets. Unique across families.
    pub fn runner_name(self, tag: &str) -> String {
        match self {
            Family::Ge => tag.to_string(),                  // GE-Proton10-34
            Family::Cachyos => format!("proton-{tag}"),     // proton-cachyos-11.0-20261015-slr
            Family::Em => format!("proton-{tag}"),          // proton-EM-10.0-37-HDR
        }
    }

    /// Which family an installed folder name belongs to, if any.
    pub fn of_name(name: &str) -> Option<Family> {
        if name.starts_with("GE-Proton") {
            Some(Family::Ge)
        } else if name.starts_with("proton-cachyos") {
            Some(Family::Cachyos)
        } else if name.starts_with("proton-EM") {
            Some(Family::Em)
        } else {
            None
        }
    }
}

/// One published build that can be installed.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Release {
    pub family: Family,
    pub tag: String,
    /// Install folder name, see [`Family::runner_name`].
    pub name: String,
    pub prerelease: bool,
    pub published_at: String,
    pub url: String,
    pub archive_name: String,
    pub size: u64,
    /// SHA-256 from GitHub, when present.
    pub sha256: Option<String>,
    /// A `sha256sum`/`sha512sum` style file published next to the archive.
    pub checksum_url: Option<String>,
}

/// One Proton build on disk.
#[derive(Debug, Clone, Serialize)]
pub struct Installed {
    pub name: String,
    pub family: Option<Family>,
    /// The runtime its toolmanifest asks for; None if unreadable/unsupported.
    pub runtime: Option<Runtime>,
    /// Set when the toolmanifest couldn't be used, to explain why.
    pub problem: Option<String>,
}

// ---- GitHub API shapes (only what we use) ----

#[derive(Deserialize)]
struct GhRelease {
    tag_name: String,
    #[serde(default)]
    prerelease: bool,
    #[serde(default)]
    draft: bool,
    #[serde(default)]
    published_at: Option<String>,
    assets: Vec<GhAsset>,
}

#[derive(Deserialize, Clone)]
struct GhAsset {
    name: String,
    browser_download_url: String,
    #[serde(default)]
    size: u64,
    #[serde(default)]
    digest: Option<String>,
}

#[derive(Serialize, Deserialize)]
struct CachedList {
    fetched: SystemTime,
    releases: Vec<Release>,
}

/// Published builds for `family`, newest first. Served from a 30-minute disk
/// cache; if GitHub can't be reached, an older cached list is used instead.
pub fn releases(agent: &ureq::Agent, paths: &Paths, family: Family, force: bool) -> Result<Vec<Release>> {
    let cache = paths.cache_dir().join(match family {
        Family::Ge => "releases-ge.json",
        Family::Cachyos => "releases-cachyos.json",
        Family::Em => "releases-em.json",
    });
    let cached: Option<CachedList> = fs::read(&cache).ok().and_then(|b| serde_json::from_slice(&b).ok());
    if !force {
        if let Some(c) = &cached {
            if c.fetched.elapsed().is_ok_and(|age| age < CACHE_FOR) {
                return Ok(c.releases.clone());
            }
        }
    }
    let url = format!("https://api.github.com/repos/{}/releases?per_page=60", family.repo());
    match crate::net::get_json::<Vec<GhRelease>>(agent, &url) {
        Ok(list) => {
            let releases: Vec<Release> = list.into_iter().filter(|r| !r.draft).filter_map(|r| to_release(family, r)).collect();
            let text = serde_json::to_vec(&CachedList { fetched: SystemTime::now(), releases: releases.clone() })?;
            let _ = write_atomic(&cache, &text);
            Ok(releases)
        }
        Err(e) => match cached {
            Some(c) => Ok(c.releases),
            None => Err(e),
        },
    }
}

/// Pick the x86_64 Linux tarball out of a release, plus how to verify it.
fn to_release(family: Family, r: GhRelease) -> Option<Release> {
    let is_tarball = |n: &str| n.ends_with(".tar.gz") || n.ends_with(".tar.xz");
    let other_arch = |n: &str| {
        let n = n.to_ascii_lowercase();
        n.contains("arm64") || n.contains("aarch64") || n.contains("x86_64_v")
    };
    let candidates: Vec<&GhAsset> = r.assets.iter().filter(|a| is_tarball(&a.name) && !other_arch(&a.name)).collect();
    let asset = candidates
        .iter()
        .find(|a| a.name.contains("x86_64"))
        .or_else(|| (candidates.len() == 1).then(|| &candidates[0]))?;
    let asset = (*asset).clone();
    let sha256 = asset
        .digest
        .as_deref()
        .and_then(|d| d.strip_prefix("sha256:"))
        .filter(|h| h.len() == 64)
        .map(str::to_ascii_lowercase);
    // "GE-Proton10-34.tar.gz" -> "GE-Proton10-34"; checksum files are named after the stem.
    let stem = asset.name.trim_end_matches(".tar.gz").trim_end_matches(".tar.xz");
    let checksum_url = r
        .assets
        .iter()
        .find(|a| a.name == format!("{stem}.sha512sum") || a.name == format!("{stem}.sha256sum"))
        .map(|a| a.browser_download_url.clone());
    let name = family.runner_name(&r.tag_name);
    if !safe_name(&name) {
        return None;
    }
    Some(Release {
        family,
        name,
        prerelease: r.prerelease,
        published_at: r.published_at.unwrap_or_default(),
        url: asset.browser_download_url,
        archive_name: asset.name,
        size: asset.size,
        sha256,
        checksum_url,
        tag: r.tag_name,
    })
}

/// A folder name that can't escape the runners folder.
pub fn safe_name(name: &str) -> bool {
    !name.is_empty()
        && name != "."
        && name != ".."
        && !name.starts_with('.')
        && name.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.' | '+'))
}

/// Every Proton build in the runners folder (folders holding a `proton` script).
pub fn installed(paths: &Paths) -> Vec<Installed> {
    let Ok(entries) = fs::read_dir(paths.runners_dir()) else {
        return Vec::new();
    };
    let mut list: Vec<Installed> = entries
        .flatten()
        .filter_map(|e| {
            let name = e.file_name().to_string_lossy().into_owned();
            if name.starts_with('.') || !e.path().join("proton").exists() {
                return None;
            }
            let (runtime, problem) = match steamrt::required_runtime(&e.path()) {
                Ok(rt) => (Some(rt), None),
                Err(err) => (None, Some(format!("{err:#}"))),
            };
            Some(Installed { family: Family::of_name(&name), name, runtime, problem })
        })
        .collect();
    list.sort_by(|a, b| b.name.cmp(&a.name));
    list
}

pub fn is_installed(paths: &Paths, name: &str) -> bool {
    safe_name(name) && paths.runner_dir(name).join("proton").exists() && archive::read_record(&paths.runner_dir(name)).is_some()
}

/// Download, verify and install one release.
pub fn install(agent: &ureq::Agent, paths: &Paths, release: &Release, progress: &Progress) -> Result<()> {
    if !safe_name(&release.name) {
        bail!("refusing odd runner name {:?}", release.name);
    }
    progress.set_label(format!("Checking {}", release.name));
    let expected = match (&release.sha256, &release.checksum_url) {
        (Some(hash), _) => Expected::new(hash)?,
        (None, Some(url)) => {
            let listing = crate::net::get_bytes(agent, url, 1)?;
            let hash = archive::find_in_listing(&String::from_utf8_lossy(&listing), &release.archive_name)
                .with_context(|| format!("{} isn't listed in its checksum file", release.archive_name))?;
            Expected::new(&hash)?
        }
        (None, None) => bail!("{} has no published checksum, so it can't be verified", release.name),
    };
    let dest = paths.runner_dir(&release.name);
    archive::install(agent, &release.name, &release.url, &release.archive_name, &expected, &dest, progress)?;
    if !dest.join("proton").exists() {
        let _ = fs::remove_dir_all(&dest);
        bail!("{} doesn't contain a proton script; not a Proton build", release.name);
    }
    // Fail now rather than at launch if it wants a runtime we can't provide.
    steamrt::required_runtime(&dest)?;
    Ok(())
}

/// Delete an installed build. A legacy symlink is unlinked, never followed.
pub fn remove(paths: &Paths, name: &str) -> Result<()> {
    if !safe_name(name) {
        bail!("refusing odd runner name {name:?}");
    }
    let dir = paths.runner_dir(name);
    let meta = fs::symlink_metadata(&dir).with_context(|| format!("{name} isn't installed"))?;
    if meta.file_type().is_symlink() {
        fs::remove_file(&dir)?;
    } else {
        fs::remove_dir_all(&dir)?;
    }
    Ok(())
}

/// Find a release by its install name in the cached lists (no network).
pub fn find_cached(agent: &ureq::Agent, paths: &Paths, name: &str) -> Option<Release> {
    let family = Family::of_name(name)?;
    releases(agent, paths, family, false).ok()?.into_iter().find(|r| r.name == name)
}

/// Is there a newer build of the same family than `name`? Compares publish
/// order in the release list (newest first), never guesses from names.
pub fn newer_than(list: &[Release], name: &str) -> Option<Release> {
    let pos = list.iter().position(|r| r.name == name)?;
    list[..pos].iter().find(|r| !r.prerelease).cloned()
}

pub fn runner_wine(dir: &Path) -> std::path::PathBuf {
    dir.join("files/bin/wine")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn asset(name: &str, digest: Option<&str>) -> GhAsset {
        GhAsset {
            name: name.into(),
            browser_download_url: format!("https://example/{name}"),
            size: 1,
            digest: digest.map(Into::into),
        }
    }

    fn rel(tag: &str, assets: Vec<GhAsset>) -> GhRelease {
        GhRelease { tag_name: tag.into(), prerelease: false, draft: false, published_at: None, assets }
    }

    #[test]
    fn ge10_has_no_arch_suffix() {
        let r = to_release(
            Family::Ge,
            rel("GE-Proton10-34", vec![asset("GE-Proton10-34.sha512sum", None), asset("GE-Proton10-34.tar.gz", Some("sha256:51c580b66a833c73998fe00f0717eeac57197654040a2f2ed5189e3ee68d773d"))]),
        )
        .unwrap();
        assert_eq!(r.name, "GE-Proton10-34");
        assert_eq!(r.archive_name, "GE-Proton10-34.tar.gz");
        assert_eq!(r.sha256.as_deref(), Some("51c580b66a833c73998fe00f0717eeac57197654040a2f2ed5189e3ee68d773d"));
        assert!(r.checksum_url.unwrap().ends_with("GE-Proton10-34.sha512sum"));
    }

    #[test]
    fn ge11_picks_x86_64_not_aarch64() {
        let r = to_release(
            Family::Ge,
            rel("GE-Proton11-12", vec![asset("GE-Proton11-12-aarch64.tar.gz", None), asset("GE-Proton11-12-x86_64.tar.gz", None), asset("GE-Proton11-12-x86_64.sha512sum", None)]),
        )
        .unwrap();
        assert_eq!(r.archive_name, "GE-Proton11-12-x86_64.tar.gz");
        assert!(r.checksum_url.unwrap().ends_with("GE-Proton11-12-x86_64.sha512sum"));
    }

    #[test]
    fn cachyos_skips_v3_and_arm() {
        let r = to_release(
            Family::Cachyos,
            rel(
                "cachyos-11.0-20261015-slr",
                vec![
                    asset("proton-cachyos-11.0-20261015-slr-arm64.tar.xz", None),
                    asset("proton-cachyos-11.0-20261015-slr-x86_64.tar.xz", None),
                    asset("proton-cachyos-11.0-20261015-slr-x86_64_v3.tar.xz", None),
                ],
            ),
        )
        .unwrap();
        assert_eq!(r.archive_name, "proton-cachyos-11.0-20261015-slr-x86_64.tar.xz");
        assert_eq!(r.name, "proton-cachyos-11.0-20261015-slr");
    }

    #[test]
    fn em_single_tarball() {
        let r = to_release(Family::Em, rel("EM-10.0-37-HDR", vec![asset("proton-EM-10.0-37-HDR.tar.xz", None), asset("proton-EM-10.0-37-HDR.sha256sum", None)])).unwrap();
        assert_eq!(r.name, "proton-EM-10.0-37-HDR");
        assert_eq!(Family::of_name(&r.name), Some(Family::Em));
    }

    #[test]
    fn names_are_safe() {
        assert!(safe_name("GE-Proton10-34"));
        assert!(!safe_name("../etc"));
        assert!(!safe_name(".hidden"));
        assert!(!safe_name("a/b"));
    }

    #[test]
    fn newer_than_uses_list_order() {
        let mk = |name: &str, pre: bool| Release {
            family: Family::Ge,
            tag: name.into(),
            name: name.into(),
            prerelease: pre,
            published_at: String::new(),
            url: String::new(),
            archive_name: String::new(),
            size: 0,
            sha256: None,
            checksum_url: None,
        };
        let list = vec![mk("GE-Proton11-1", true), mk("GE-Proton10-35", false), mk("GE-Proton10-34", false)];
        assert_eq!(newer_than(&list, "GE-Proton10-34").unwrap().name, "GE-Proton10-35");
        assert!(newer_than(&list, "GE-Proton10-35").is_none());
    }
}
