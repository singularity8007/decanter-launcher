use crate::config::Config;
use crate::progress::Progress;
use anyhow::{bail, Context, Result};
use flate2::read::GzDecoder;
use sha2::{digest::DynDigest, Sha256, Sha512};
use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::time::Duration;
use tar::Archive;
use xz2::read::XzDecoder;

const MAX_ATTEMPTS: u32 = 3;

/// The GE-Proton release ZZZ is known to run smoothly on: the exact one
/// TwintailLauncher uses on this machine. Pinned on purpose: "latest"
/// (GE-Proton11, DXVK 3) is what made the game choppy.
pub const GE_TAG: &str = "GE-Proton10-34";

/// GE-Proton10 needs Steam Linux Runtime 3 "sniper" (its toolmanifest asks
/// for app 1628350), the same runtime Twintail launches ZZZ through.
const STEAMRT_BASE: &str = "https://repo.steampowered.com/steamrt3/images/latest-public-beta";
const STEAMRT_ARCHIVE: &str = "SteamLinuxRuntime_sniper.tar.xz";

/// True when the pinned GE-Proton and the sniper runtime are both installed,
/// checked from local files only (no network).
pub fn runtime_ready(cfg: &Config) -> bool {
    let ge_ok = fs::read_to_string(cfg.runner_dir.join("version"))
        .is_ok_and(|v| v.split_whitespace().any(|w| w == GE_TAG))
        && cfg.runner_dir.join("proton").exists()
        && read_record(&cfg.runner_dir).is_some();
    let rt_ok = cfg.entry_point().exists()
        && fs::read_to_string(cfg.steamrt_dir.join("VERSIONS.txt"))
            .is_ok_and(|v| v.contains("sniper"))
        && read_record(&cfg.steamrt_dir).is_some();
    ge_ok && rt_ok
}

/// How long to wait for a server to start answering once we're connected.
/// Without this, a server that accepts the connection and then goes silent
/// would hang us forever. It only limits the wait for the reply to begin, so
/// big downloads aren't cut short.
const REPLY_TIMEOUT: Duration = Duration::from_secs(30);

/// File inside each install folder holding the published hash of the archive
/// it was installed from. Startup compares it to the latest published hash.
const RECORD_NAME: &str = ".installed_hash";

/// Everything that differs between the things we install.
struct Package {
    name: &'static str,
    /// Install folder. The launcher owns it and wipes it on reinstall.
    dest: PathBuf,
    archive_name: String,
    archive_url: String,
    /// Text file listing the published hash (`<hash>  <filename>` per line).
    hash_url: String,
}

/// Run every update check. Progress and warnings go to `progress`.
pub fn update_all(cfg: &Config, progress: &Progress) -> Result<()> {

    let agent = build_agent();

    ensure(&agent, &steamrt_package(cfg), progress)
        .or_else(|e| use_installed_copy("Steam Linux Runtime", &cfg.steamrt_dir, e, progress))?;

    // Building the GE-Proton package needs the network (to learn the latest tag).
    progress.set_label(format!("Checking {GE_TAG}..."));
    ge_proton_package(cfg)
        .and_then(|pkg| ensure(&agent, &pkg, progress))
        .or_else(|e| use_installed_copy("GE-Proton", &cfg.runner_dir, e, progress))
}

/// If the update check failed but a copy is already installed, keep going
/// with it (offline play) instead of failing; the window shows a warning.
fn use_installed_copy(
    name: &str,
    dest: &Path,
    err: anyhow::Error,
    progress: &Progress,
) -> Result<()> {
    if read_record(dest).is_some() {
        progress.note(format!(
            "Couldn't check {name} for updates just now, so the version you already have is being used."
        ));
        Ok(())
    } else {
        Err(err)
    }
}

fn steamrt_package(cfg: &Config) -> Package {
    Package {
        name: "Steam Linux Runtime",
        dest: cfg.steamrt_dir.clone(),
        archive_name: STEAMRT_ARCHIVE.into(),
        archive_url: format!("{STEAMRT_BASE}/{STEAMRT_ARCHIVE}"),
        hash_url: format!("{STEAMRT_BASE}/SHA256SUMS"),
    }
}

fn ge_proton_package(cfg: &Config) -> Result<Package> {
    let tag = GE_TAG;
    let base = format!("https://github.com/GloriousEggroll/proton-ge-custom/releases/download/{tag}");
    // GE-Proton10 archives have no "-x86_64" suffix (GE-Proton11 added it).
    let archive_name = format!("{tag}.tar.gz");
    Ok(Package {
        name: "GE-Proton",
        dest: cfg.runner_dir.clone(),
        archive_url: format!("{base}/{archive_name}"),
        hash_url: format!("{base}/{tag}.sha512sum"),
        archive_name,
    })
}

/// Bring `pkg` up to date: skip if the recorded hash matches the latest
/// published one, otherwise download, verify, wipe, extract and record.
fn ensure(agent: &ureq::Agent, pkg: &Package, progress: &Progress) -> Result<()> {
    progress.set_label(format!("Checking {}...", pkg.name));
    let expected = published_hash(agent, pkg)?;
    if read_record(&pkg.dest).as_deref() == Some(expected.as_str()) {
        return Ok(()); // already the latest
    }

    // The archive sits next to the install folder, so wiping the folder
    // never deletes it.
    let work_dir = pkg.dest.parent().context("install folder has no parent")?;
    fs::create_dir_all(work_dir)?;
    let archive = work_dir.join(&pkg.archive_name);

    // Download and verify, retrying on failure. A wrong hash counts as a
    // failure (truncated or corrupted download).
    let mut attempt = 1;
    loop {
        progress.set_label(format!(
            "Downloading {} (attempt {attempt} of {MAX_ATTEMPTS})...",
            pkg.name
        ));
        match download(agent, &pkg.archive_url, &archive, &expected) {
            Ok(actual) if actual == expected => break,
            Ok(_) => progress.note(format!(
                "The {} download looked damaged, so it was fetched again.",
                pkg.name
            )),
            Err(_) => progress.note(format!(
                "The {} download was interrupted, so it was tried again.",
                pkg.name
            )),
        }
        if attempt == MAX_ATTEMPTS {
            bail!("giving up on {} after {MAX_ATTEMPTS} attempts", pkg.archive_name);
        }
        attempt += 1;
    }

    // Only wipe the old install once the new archive is verified, so a
    // failed download leaves the old install usable.
    progress.set_label(format!("Installing {}...", pkg.name));
    if pkg.dest.exists() {
        fs::remove_dir_all(&pkg.dest)?;
    }
    extract(&archive, &pkg.dest)?;

    // Last, on purpose: only a finished extraction gets a record.
    fs::write(pkg.dest.join(RECORD_NAME), &expected)?;

    // DECISION (yours): the archive is redundant once extracted, so delete
    // it. Remove this line to keep it as a local cache.
    fs::remove_file(&archive)?;
    Ok(())
}

/// The hash we recorded when we last installed into `dest`, if any.
fn read_record(dest: &Path) -> Option<String> {
    fs::read_to_string(dest.join(RECORD_NAME))
        .ok()
        .map(|s| s.trim().to_string())
}

pub fn build_agent() -> ureq::Agent {
    let config = ureq::Agent::config_builder()
        .timeout_connect(Some(Duration::from_secs(15)))
        .timeout_recv_response(Some(REPLY_TIMEOUT))
        // Total budget for reading one body. Generous for slow connections.
        .timeout_recv_body(Some(Duration::from_secs(30 * 60)))
        .build();
    ureq::Agent::new_with_config(config)
}

/// Fetch the hash file and pull out the hash for our archive.
fn published_hash(agent: &ureq::Agent, pkg: &Package) -> Result<String> {
    let text = agent
        .get(pkg.hash_url.as_str())
        .call()?
        .body_mut()
        .read_to_string()?;

    text.lines()
        .find_map(|line| {
            let (hash, name) = line.split_once(char::is_whitespace)?;
            let name = name.trim().trim_start_matches('*');
            name.ends_with(pkg.archive_name.as_str()).then(|| hash.to_lowercase())
        })
        .with_context(|| format!("{} not listed in {}", pkg.archive_name, pkg.hash_url))
}

/// SHA-256 hashes are 64 hex characters and SHA-512 are 128, so the
/// published hash itself tells us which algorithm to use.
fn hasher_for(expected: &str) -> Result<Box<dyn DynDigest>> {
    match expected.len() {
        64 => Ok(Box::new(Sha256::default())),
        128 => Ok(Box::new(Sha512::default())),
        n => bail!("unrecognized hash length: {n}"),
    }
}

/// Stream `url` into `dest`, hashing as we write, and return the hash as
/// lowercase hex. The archive is never held in memory.
fn download(agent: &ureq::Agent, url: &str, dest: &Path, expected: &str) -> Result<String> {
    let mut hasher = hasher_for(expected)?;
    let mut reader = agent.get(url).call()?.into_body().into_reader();
    let mut file = File::create(dest)?;
    let mut buf = [0u8; 64 * 1024];

    loop {
        let n = reader.read(&mut buf)?;
        if n == 0 {
            break; // end of the body
        }
        hasher.update(&buf[..n]);
        file.write_all(&buf[..n])?;
    }
    Ok(hasher.finalize().iter().map(|b| format!("{b:02x}")).collect())
}

/// Extract a .tar.xz or .tar.gz into `dest`, dropping the archive's
/// top-level folder so the contents land directly in `dest`.
fn extract(archive: &Path, dest: &Path) -> Result<()> {
    // File -> decompressor -> tar. Only the bytes tar asks for get
    // decompressed, so nothing large sits on disk uncompressed.
    let file = File::open(archive)?;
    let reader: Box<dyn Read> = match archive.extension().and_then(|e| e.to_str()) {
        Some("xz") => Box::new(XzDecoder::new(file)),
        Some("gz") => Box::new(GzDecoder::new(file)),
        other => bail!("unsupported archive type: {other:?}"),
    };
    let mut tar = Archive::new(reader);

    for entry in tar.entries()? {
        let mut entry = entry?;

        // Drop the first path component (the top-level folder).
        let stripped: PathBuf = entry.path()?.components().skip(1).collect();
        if stripped.as_os_str().is_empty() {
            continue; // the top folder itself
        }

        let out = dest.join(&stripped);
        // entry.unpack() doesn't create missing parent folders.
        if let Some(parent) = out.parent() {
            fs::create_dir_all(parent)?;
        }
        entry.unpack(&out)?;
    }
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;

    /// Real network install of the pinned runtime, exactly what Play does
    /// when it's missing. `cargo test install_runtime -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn install_runtime() {
        let cfg = Config::hardcoded().unwrap();
        println!("ready before: {}", runtime_ready(&cfg));
        let progress = Progress::default();
        update_all(&cfg, &progress).unwrap();
        println!("notes: {:?}", progress.take_notes());
        assert!(runtime_ready(&cfg), "runtime not ready after install");
        println!("ready after: true");
    }
}
