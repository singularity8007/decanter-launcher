//! Download-verify-extract for runner and runtime tarballs.
//!
//! The archive is streamed to disk while being hashed, compared against a
//! published hash, and only then extracted into a temp folder that is renamed
//! into place. A failed or damaged download never touches a working install.

use crate::progress::{Phase, Progress};
use anyhow::{Context, Result, bail};
use flate2::read::GzDecoder;
use liblzma::read::XzDecoder;
use sha2::{Sha256, Sha512};
use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::{Component, Path, PathBuf};
use tar::Archive;

const MAX_ATTEMPTS: u32 = 3;

/// File inside each install folder holding the hash of the archive it was
/// installed from.
pub const RECORD_NAME: &str = ".installed_hash";

/// A hash the archive must match. SHA-256 or SHA-512, as lowercase hex.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Expected(pub String);

impl Expected {
    pub fn new(hex: &str) -> Result<Self> {
        let hex = hex.trim().to_ascii_lowercase();
        if !(hex.len() == 64 || hex.len() == 128) || !hex.chars().all(|c| c.is_ascii_hexdigit()) {
            bail!("not a SHA-256/512 hash: {hex:?}");
        }
        Ok(Self(hex))
    }
}

/// Find the hash for `archive_name` in a `sha256sum`-style listing
/// (`<hash>  <file>` or `<hash> *<file>` per line).
pub fn find_in_listing(listing: &str, archive_name: &str) -> Option<String> {
    listing.lines().find_map(|line| {
        let (hash, name) = line.trim().split_once(char::is_whitespace)?;
        let name = name.trim().trim_start_matches('*');
        let base = name.rsplit('/').next().unwrap_or(name);
        (base == archive_name).then(|| hash.to_ascii_lowercase())
    })
}

/// The hash recorded when `dest` was last installed, if any.
pub fn read_record(dest: &Path) -> Option<String> {
    fs::read_to_string(dest.join(RECORD_NAME)).ok().map(|s| s.trim().to_string())
}

/// Download `url`, check it against `expected`, and install its contents into
/// `dest` (the archive's top-level folder is dropped). `dest` is replaced only
/// after everything succeeded.
pub fn install(
    agent: &ureq::Agent,
    name: &str,
    url: &str,
    archive_name: &str,
    expected: &Expected,
    dest: &Path,
    progress: &Progress,
) -> Result<()> {
    let parent = dest.parent().context("install folder has no parent")?;
    fs::create_dir_all(parent)?;
    let dest_name = dest.file_name().context("install folder has no name")?.to_string_lossy();
    // Hidden work files next to the destination, so a rename finishes the job
    // and nothing half-done ever has the real name.
    let archive = parent.join(format!(".{dest_name}.{archive_name}.part"));
    let staging = parent.join(format!(".{dest_name}.extracting"));

    let mut attempt = 1;
    loop {
        if progress.cancelled() {
            let _ = fs::remove_file(&archive);
            bail!("paused");
        }
        progress.set_phase(Phase::Downloading);
        progress.set_label(format!("Downloading {name}"));
        match download(agent, url, &archive, expected, progress) {
            Ok(actual) if actual == expected.0 => break,
            Ok(actual) => progress.note(format!(
                "The {name} download didn't match its checksum (got {}…), so it was fetched again.",
                &actual[..12]
            )),
            Err(e) if progress.cancelled() => {
                let _ = fs::remove_file(&archive);
                return Err(e.context("paused"));
            }
            Err(e) => progress.note(format!("The {name} download was interrupted ({e:#}), retrying.")),
        }
        if attempt == MAX_ATTEMPTS {
            let _ = fs::remove_file(&archive);
            bail!("giving up on {archive_name} after {MAX_ATTEMPTS} attempts");
        }
        attempt += 1;
    }

    progress.set_phase(Phase::Installing);
    progress.set_label(format!("Installing {name}"));
    if staging.exists() {
        fs::remove_dir_all(&staging)?;
    }
    let result = extract(&archive, archive_name, &staging, progress).and_then(|()| {
        fs::write(staging.join(RECORD_NAME), &expected.0)?;
        // remove_dir_all on a symlink removes only the link, never its target.
        if fs::symlink_metadata(dest).is_ok() {
            if fs::symlink_metadata(dest)?.file_type().is_symlink() {
                fs::remove_file(dest)?;
            } else {
                fs::remove_dir_all(dest)?;
            }
        }
        fs::rename(&staging, dest)?;
        Ok(())
    });
    let _ = fs::remove_file(&archive);
    if result.is_err() {
        let _ = fs::remove_dir_all(&staging);
    }
    result
}

/// Stream `url` into `dest`, hashing while writing. Returns the lowercase hex hash.
fn download(
    agent: &ureq::Agent,
    url: &str,
    dest: &Path,
    expected: &Expected,
    progress: &Progress,
) -> Result<String> {
    let mut hasher: Box<dyn sha2::digest::DynDigest> = match expected.0.len() {
        64 => Box::new(Sha256::default()),
        _ => Box::new(Sha512::default()),
    };
    let mut response = agent.get(url).call().with_context(|| format!("GET {url}"))?;
    let total = response
        .headers()
        .get("content-length")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.parse::<u64>().ok())
        .unwrap_or(0);
    progress.set_bars(0, total, 0, 0);
    let mut reader = response.body_mut().with_config().limit(u64::MAX).reader();
    let mut file = File::create(dest)?;
    let mut buf = vec![0u8; 256 * 1024];
    loop {
        if progress.cancelled() {
            bail!("paused");
        }
        let n = reader.read(&mut buf)?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
        file.write_all(&buf[..n])?;
        progress.add(n as u64);
        progress.add_net(n as u64);
        progress.add_disk(n as u64);
    }
    file.sync_all()?;
    Ok(hasher.finalize().iter().map(|b| format!("{b:02x}")).collect())
}

/// Extract a .tar.xz or .tar.gz (type taken from `archive_name`) into `dest`,
/// dropping the top-level folder. Entries that would land outside `dest` are
/// refused.
pub fn extract(archive: &Path, archive_name: &str, dest: &Path, progress: &Progress) -> Result<()> {
    let file = File::open(archive)?;
    let size = file.metadata()?.len();
    let counted = CountingReader { inner: file, progress, read: 0, total: size };
    progress.set_install(0, size);
    let reader: Box<dyn Read> = if archive_name.ends_with(".tar.xz") || archive_name.ends_with(".txz") {
        Box::new(XzDecoder::new(counted))
    } else if archive_name.ends_with(".tar.gz") || archive_name.ends_with(".tgz") {
        Box::new(GzDecoder::new(counted))
    } else {
        bail!("unsupported archive type: {archive_name}");
    };
    fs::create_dir_all(dest)?;
    let mut tar = Archive::new(reader);
    tar.set_preserve_permissions(true);
    for entry in tar.entries()? {
        if progress.cancelled() {
            bail!("paused");
        }
        let mut entry = entry?;
        let path = entry.path()?.into_owned();
        let stripped: PathBuf = path.components().skip(1).collect();
        if stripped.as_os_str().is_empty() {
            continue; // the top folder itself
        }
        if !stripped.components().all(|c| matches!(c, Component::Normal(_))) {
            bail!("unsafe path in archive: {}", path.display());
        }
        let out = dest.join(&stripped);
        if let Some(parent) = out.parent() {
            fs::create_dir_all(parent)?;
        }
        entry.unpack(&out).with_context(|| format!("unpacking {}", stripped.display()))?;
    }
    Ok(())
}

/// Reports how far through the compressed archive extraction has read.
struct CountingReader<'a, R> {
    inner: R,
    progress: &'a Progress,
    read: u64,
    total: u64,
}

impl<R: Read> Read for CountingReader<'_, R> {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        let n = self.inner.read(buf)?;
        self.read += n as u64;
        self.progress.set_install(self.read.min(self.total), self.total);
        self.progress.add_disk(n as u64);
        Ok(n)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn listing_lookup() {
        let listing = "9dc4  *SteamLinuxRuntime_sniper.tar.xz\nabcd  GE-Proton10-34.tar.gz\n";
        assert_eq!(find_in_listing(listing, "SteamLinuxRuntime_sniper.tar.xz").as_deref(), Some("9dc4"));
        assert_eq!(find_in_listing(listing, "GE-Proton10-34.tar.gz").as_deref(), Some("abcd"));
        assert_eq!(find_in_listing(listing, "sniper.tar.xz"), None);
    }

    #[test]
    fn expected_validates() {
        assert!(Expected::new(&"a".repeat(64)).is_ok());
        assert!(Expected::new(&"A".repeat(128)).is_ok());
        assert!(Expected::new("abc").is_err());
        assert!(Expected::new(&"z".repeat(64)).is_err());
    }

    #[test]
    fn extract_strips_top_folder_and_installs() {
        let tmp = tempfile::tempdir().unwrap();
        let tarball = tmp.path().join("pkg.tar.gz");
        {
            let gz = flate2::write::GzEncoder::new(File::create(&tarball).unwrap(), flate2::Compression::fast());
            let mut builder = tar::Builder::new(gz);
            let data = b"#!/bin/sh\n";
            let mut header = tar::Header::new_gnu();
            header.set_size(data.len() as u64);
            header.set_mode(0o755);
            header.set_cksum();
            builder.append_data(&mut header, "top/proton", &data[..]).unwrap();
            builder.into_inner().unwrap().finish().unwrap();
        }
        let dest = tmp.path().join("out");
        extract(&tarball, "pkg.tar.gz", &dest, &Progress::default()).unwrap();
        assert_eq!(fs::read(dest.join("proton")).unwrap(), b"#!/bin/sh\n");
    }
}
