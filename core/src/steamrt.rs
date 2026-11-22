//! Steam Linux Runtime: which one a Proton build needs, and installing it.
//!
//! Every Proton build ships a `toolmanifest.vdf` whose `require_tool_appid`
//! names the runtime it was built against. GE-Proton10 asks for 1628350
//! (SLR 3 "sniper"); newer builds may ask for 4183110 (SLR 4). Running a
//! Proton inside the wrong runtime is exactly the kind of mismatch that
//! breaks games, so the runtime is always read from the build itself.

use crate::archive::{self, Expected};
use crate::paths::Paths;
use crate::progress::Progress;
use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;

/// Steam publishes both runtimes on this branch; it's what the sniper copy
/// Decanter has always used came from.
const BRANCH: &str = "latest-public-beta";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Runtime {
    /// Steam Linux Runtime 3.0 "sniper", tool app 1628350.
    Sniper,
    /// Steam Linux Runtime 4.0, tool app 4183110.
    Steamrt4,
}

impl Runtime {
    pub const ALL: [Runtime; 2] = [Runtime::Sniper, Runtime::Steamrt4];

    pub fn from_appid(appid: &str) -> Option<Self> {
        match appid {
            "1628350" => Some(Runtime::Sniper),
            "4183110" => Some(Runtime::Steamrt4),
            _ => None,
        }
    }

    pub fn folder(self) -> &'static str {
        match self {
            Runtime::Sniper => "steamrt3",
            Runtime::Steamrt4 => "steamrt4",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Runtime::Sniper => "Steam Linux Runtime 3 (sniper)",
            Runtime::Steamrt4 => "Steam Linux Runtime 4",
        }
    }

    fn archive_name(self) -> &'static str {
        match self {
            Runtime::Sniper => "SteamLinuxRuntime_sniper.tar.xz",
            Runtime::Steamrt4 => "SteamLinuxRuntime_4.tar.xz",
        }
    }

    fn base_url(self) -> String {
        format!("https://repo.steampowered.com/{}/images/{BRANCH}", self.folder())
    }

    /// The word `VERSIONS.txt` mentions for this runtime, as a sanity check
    /// that the folder really holds the runtime we think it does.
    fn depot_major(self) -> &'static str {
        match self {
            Runtime::Sniper => "3.",
            Runtime::Steamrt4 => "4.",
        }
    }
}

/// Read `require_tool_appid` from a Proton build's `toolmanifest.vdf`.
pub fn required_appid(runner_dir: &Path) -> Result<String> {
    let path = runner_dir.join("toolmanifest.vdf");
    let text = fs::read_to_string(&path).with_context(|| format!("reading {}", path.display()))?;
    parse_vdf_value(&text, "require_tool_appid")
        .with_context(|| format!("{} doesn't name a required runtime", path.display()))
}

/// The runtime a Proton build needs. Fails clearly for builds that want a
/// runtime Decanter doesn't provide (e.g. the old SLR 2 "soldier").
pub fn required_runtime(runner_dir: &Path) -> Result<Runtime> {
    let appid = required_appid(runner_dir)?;
    Runtime::from_appid(&appid).with_context(|| {
        format!("this Proton build needs Steam tool app {appid}, which Decanter doesn't support")
    })
}

/// Find `"key" "value"` in a flat VDF file. Good enough for toolmanifest.vdf,
/// which has one key per line.
fn parse_vdf_value(text: &str, key: &str) -> Option<String> {
    let quoted_key = format!("\"{key}\"");
    text.lines().find_map(|line| {
        let rest = line.trim().strip_prefix(&quoted_key)?;
        let value = rest.trim();
        let value = value.strip_prefix('"')?;
        let end = value.find('"')?;
        Some(value[..end].to_string())
    })
}

/// Installed and looks complete: entry point present, right depot, and a
/// hash record from a finished install. No network.
pub fn ready(paths: &Paths, runtime: Runtime) -> bool {
    let dir = paths.steamrt_dir(runtime);
    dir.join("_v2-entry-point").exists()
        && archive::read_record(&dir).is_some()
        && fs::read_to_string(dir.join("VERSIONS.txt")).is_ok_and(|v| {
            v.lines().any(|l| l.starts_with("depot") && l.split_whitespace().nth(1).is_some_and(|ver| ver.starts_with(runtime.depot_major())))
        })
}

/// The installed depot version, e.g. "3.0.20261110.268881".
pub fn installed_version(paths: &Paths, runtime: Runtime) -> Option<String> {
    let text = fs::read_to_string(paths.steamrt_dir(runtime).join("VERSIONS.txt")).ok()?;
    text.lines()
        .find(|l| l.starts_with("depot"))
        .and_then(|l| l.split_whitespace().nth(1))
        .map(str::to_string)
}

/// Install `runtime` if missing, or update it if Steam published a new build.
/// Returns true if anything was downloaded.
pub fn ensure(agent: &ureq::Agent, paths: &Paths, runtime: Runtime, progress: &Progress) -> Result<bool> {
    let dir = paths.steamrt_dir(runtime);
    progress.set_label(format!("Checking {}", runtime.label()));
    let listing = crate::net::get_bytes(agent, &format!("{}/SHA256SUMS", runtime.base_url()), 1)?;
    let listing = String::from_utf8_lossy(&listing);
    let hash = archive::find_in_listing(&listing, runtime.archive_name())
        .with_context(|| format!("{} isn't listed in SHA256SUMS", runtime.archive_name()))?;
    let expected = Expected::new(&hash)?;
    if ready(paths, runtime) && archive::read_record(&dir).as_deref() == Some(expected.0.as_str()) {
        return Ok(false);
    }
    archive::install(
        agent,
        runtime.label(),
        &format!("{}/{}", runtime.base_url(), runtime.archive_name()),
        runtime.archive_name(),
        &expected,
        &dir,
        progress,
    )?;
    if !ready(paths, runtime) {
        bail!("{} installed but doesn't look complete", runtime.label());
    }
    Ok(true)
}

/// Make sure `runtime` is installed, downloading it only if it's missing.
/// Used on Play: never updates a working runtime behind the player's back.
pub fn ensure_present(agent: &ureq::Agent, paths: &Paths, runtime: Runtime, progress: &Progress) -> Result<()> {
    if ready(paths, runtime) {
        return Ok(());
    }
    ensure(agent, paths, runtime, progress).map(|_| ())
}

#[cfg(test)]
mod tests {
    use super::*;

    const GE10: &str = r#""manifest"
{
  "version" "2"
  "commandline" "/proton %verb%"
  "require_tool_appid" "1628350"
  "use_sessions" "1"
  "compatmanager_layer_name" "proton"
}"#;

    #[test]
    fn parses_toolmanifest() {
        assert_eq!(parse_vdf_value(GE10, "require_tool_appid").as_deref(), Some("1628350"));
        assert_eq!(parse_vdf_value(GE10, "commandline").as_deref(), Some("/proton %verb%"));
        assert_eq!(parse_vdf_value(GE10, "missing"), None);
    }

    #[test]
    fn tab_separated_vdf() {
        let text = "\"manifest\"\n{\n\t\"require_tool_appid\"\t\t\"4183110\"\n}";
        assert_eq!(parse_vdf_value(text, "require_tool_appid").as_deref(), Some("4183110"));
    }

    #[test]
    fn maps_appids() {
        assert_eq!(Runtime::from_appid("1628350"), Some(Runtime::Sniper));
        assert_eq!(Runtime::from_appid("4183110"), Some(Runtime::Steamrt4));
        assert_eq!(Runtime::from_appid("1391110"), None); // soldier
    }

    #[test]
    fn required_runtime_reads_file() {
        let tmp = tempfile::tempdir().unwrap();
        fs::write(tmp.path().join("toolmanifest.vdf"), GE10).unwrap();
        assert_eq!(required_runtime(tmp.path()).unwrap(), Runtime::Sniper);
    }
}
