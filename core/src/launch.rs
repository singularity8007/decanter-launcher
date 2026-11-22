//! Building and running the launch command:
//!
//! ```text
//! [powerprofilesctl launch -p performance --] <steamrt>/_v2-entry-point --verb=V -- \
//!     <reaper> SteamLaunch AppId=0 -- <runner>/proton V <exe> [args]
//! ```
//!
//! The same chain Steam and TwintailLauncher use: the Steam Linux Runtime
//! container the Proton build asks for, Steam's reaper to clean up leftover
//! processes, and Proton itself.

use crate::installs::Install;
use crate::settings::Settings;
use crate::{dxvk_conf, prefix};
use anyhow::{Context, Result, bail};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};

/// HUD contents when the HUD switch is on: what's needed to judge smoothness.
const HUD: &str = "fps,frametimes,gpuload,compiler";

/// NVIDIA's shader disk cache is ~1 GB by default and the driver prunes it
/// when full, bringing shader stutter back every session. 10 GB, never prune.
const GL_SHADER_CACHE_SIZE: &str = "10737418240";

/// Everything a launch needs, already resolved.
pub struct Launch<'a> {
    pub install: &'a Install,
    pub runner_dir: PathBuf,
    pub steamrt_dir: PathBuf,
    pub reaper: &'a Path,
    pub settings: &'a Settings,
}

impl Launch<'_> {
    fn entry_point(&self) -> PathBuf {
        self.steamrt_dir.join("_v2-entry-point")
    }

    /// Check the pieces exist, so a missing file is a clear error instead of
    /// a cryptic one from deep inside the container.
    pub fn preflight(&self) -> Result<()> {
        let checks = [
            (self.install.exe(), "the game's exe"),
            (self.runner_dir.join("proton"), "the Proton build"),
            (self.entry_point(), "the Steam Linux Runtime"),
            (self.reaper.to_path_buf(), "Steam's reaper helper"),
        ];
        for (path, what) in checks {
            if !path.exists() {
                bail!("{what} is missing: {}", path.display());
            }
        }
        Ok(())
    }

    /// Prepare the prefix folder and dxvk.conf, then start the game. Returns
    /// the running process; the caller waits on it.
    pub fn spawn(&self) -> Result<Child> {
        self.preflight()?;
        prefix::ensure(&self.install.prefix).context("creating the Proton data folder")?;
        dxvk_conf::write(&self.install.prefix, &self.install.dir, self.settings.fps_cap)
            .context("writing dxvk.conf")?;
        let mut cmd = self.command("waitforexitandrun");
        log::info!("launching: {cmd:?}");
        cmd.spawn().context("starting the launch chain")
    }

    /// The exe as Proton should see it: through the `s:` game drive when the
    /// game's profile enables `gamedrive` (ZZZ), otherwise via `Z:` (the
    /// whole Linux filesystem).
    fn exe_arg(&self) -> String {
        let info = self.install.game.info();
        let gamedrive = info.compat_config.split(',').any(|c| c == "gamedrive");
        if gamedrive {
            format!("s:\\{}", info.exe.replace('/', "\\"))
        } else {
            format!("Z:{}", self.install.exe().to_string_lossy().replace('/', "\\"))
        }
    }

    /// Let Proton create (or upgrade) the prefix inside the runtime, without
    /// starting the game: runs `wineboot` through the same chain and waits.
    /// Needed before winetricks touches a fresh prefix.
    pub fn init_prefix(&self) -> Result<()> {
        prefix::ensure(&self.install.prefix).context("creating the Proton data folder")?;
        if prefix::exists(&self.install.prefix) {
            return Ok(());
        }
        let mut cmd = self.build("waitforexitandrun", "wineboot".into(), false, false);
        log::info!("creating prefix: {cmd:?}");
        let _ = cmd.stdout(Stdio::null()).stderr(Stdio::null()).status().context("starting Proton to create the prefix")?;
        if !prefix::exists(&self.install.prefix) {
            bail!("Proton didn't create a prefix in {}", self.install.prefix.display());
        }
        Ok(())
    }

    /// Build (but don't run) the command that starts the game.
    pub fn command(&self, verb: &str) -> Command {
        self.build(verb, self.exe_arg(), true, self.settings.performance_mode)
    }

    fn build(&self, verb: &str, target: String, game_args: bool, performance: bool) -> Command {
        let install = self.install;
        let info = install.game.info();
        let pfx = install.prefix.join("pfx");

        // Hold the performance power profile for exactly as long as the game
        // runs. It's released automatically when the game exits.
        let mut cmd = if performance && power_profiles_available() {
            let mut c = Command::new("powerprofilesctl");
            c.args(["launch", "-p", "performance", "-r"])
                .arg(format!("Playing {}", info.name))
                .args(["-i", "decanter", "--"])
                .arg(self.entry_point());
            c
        } else {
            Command::new(self.entry_point())
        };

        cmd.arg(format!("--verb={verb}"))
            .arg("--")
            .arg(self.reaper)
            .args(["SteamLaunch", "AppId=0", "--"])
            .arg(self.runner_dir.join("proton"))
            .arg(verb)
            .arg(target);
        if game_args {
            cmd.args(install.graphics_args().split_whitespace());
            cmd.args(install.extra_args.split_whitespace());
        }

        cmd.current_dir(&install.dir);
        cmd.stdin(Stdio::null());

        cmd.env("WINEARCH", "win64")
            .env("WINEPREFIX", &pfx)
            .env("STEAM_COMPAT_APP_ID", "0")
            .env("STEAM_COMPAT_DATA_PATH", &install.prefix)
            .env("STEAM_COMPAT_INSTALL_PATH", &install.dir)
            .env("STEAM_COMPAT_CLIENT_INSTALL_PATH", "")
            .env("STEAM_COMPAT_TOOL_PATHS", &self.runner_dir)
            .env("STEAM_COMPAT_LIBRARY_PATHS", format!("{}:{}", install.dir.display(), pfx.display()))
            .env("STEAM_COMPAT_SHADER_PATH", install.prefix.join("shadercache"))
            // `=d` disables a DLL. Both are Steam-only pieces we don't have.
            .env("WINEDLLOVERRIDES", "lsteamclient=d;KRSDKExternal.exe=d")
            .env("SteamGameId", info.steam_game_id)
            .env("SteamOS", "1")
            .env("STEAM_COMPAT_CONFIG", info.compat_config)
            .env("PROTONFIXES_DISABLE", "1")
            .env("__GL_SHADER_DISK_CACHE", "1")
            .env("__GL_SHADER_DISK_CACHE_SIZE", GL_SHADER_CACHE_SIZE)
            .env("__GL_SHADER_DISK_CACHE_SKIP_CLEANUP", "1")
            .env("DXVK_CONFIG_FILE", dxvk_conf::path(&install.prefix));

        if self.settings.hud {
            cmd.env("DXVK_HUD", HUD);
        } else {
            cmd.env_remove("DXVK_HUD");
        }
        if self.settings.wayland {
            cmd.env("PROTON_ENABLE_WAYLAND", "1");
        } else {
            cmd.env_remove("PROTON_ENABLE_WAYLAND");
            cmd.env_remove("PROTON_USE_WAYLAND");
        }
        // DX11 is capped in dxvk.conf; DX12 goes through vkd3d-proton.
        if self.settings.fps_cap > 0 {
            cmd.env("VKD3D_FRAME_RATE", self.settings.fps_cap.to_string());
        }
        // The player's own variables win over everything above.
        for (k, v) in &install.env {
            if !k.is_empty() && !k.contains('=') {
                cmd.env(k, v);
            }
        }
        cmd
    }
}

/// True if power-profiles-daemon answers, so `powerprofilesctl launch` won't
/// fail before the game even starts.
fn power_profiles_available() -> bool {
    Command::new("powerprofilesctl")
        .arg("get")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|s| s.success())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::games::GameId;
    use std::collections::BTreeMap;

    fn install(game: GameId) -> Install {
        Install {
            game,
            dir: "/games/x".into(),
            prefix: "/prefixes/x".into(),
            runner: "GE-Proton10-34".into(),
            graphics: None,
            voice: None,
            extra_args: String::new(),
            env: BTreeMap::new(),
            pin_runner: false,
        }
    }

    fn args(cmd: &Command) -> Vec<String> {
        cmd.get_args().map(|a| a.to_string_lossy().into_owned()).collect()
    }

    fn env(cmd: &Command, key: &str) -> Option<String> {
        cmd.get_envs().find(|(k, _)| *k == key).and_then(|(_, v)| v.map(|v| v.to_string_lossy().into_owned()))
    }

    #[test]
    fn zzz_matches_the_known_smooth_chain() {
        let i = install(GameId::Zzz);
        let s = Settings { performance_mode: false, ..Settings::default() };
        let l = Launch { install: &i, runner_dir: "/r/GE".into(), steamrt_dir: "/rt/steamrt3".into(), reaper: Path::new("/bin/reaper"), settings: &s };
        let cmd = l.command("waitforexitandrun");
        assert_eq!(cmd.get_program(), "/rt/steamrt3/_v2-entry-point");
        assert_eq!(
            args(&cmd),
            ["--verb=waitforexitandrun", "--", "/bin/reaper", "SteamLaunch", "AppId=0", "--", "/r/GE/proton", "waitforexitandrun", "s:\\ZenlessZoneZero.exe"]
        );
        assert_eq!(env(&cmd, "STEAM_COMPAT_CONFIG").as_deref(), Some("noxalia,gamedrive"));
        assert_eq!(env(&cmd, "WINEPREFIX").as_deref(), Some("/prefixes/x/pfx"));
        assert_eq!(env(&cmd, "PROTON_ENABLE_WAYLAND"), None);
        assert_eq!(cmd.get_current_dir(), Some(Path::new("/games/x")));
    }

    #[test]
    fn graphics_and_extra_args_follow_the_exe() {
        let mut i = install(GameId::Wuwa);
        i.extra_args = "-foo  -bar".into();
        let s = Settings { performance_mode: false, fps_cap: 120, ..Settings::default() };
        let l = Launch { install: &i, runner_dir: "/r".into(), steamrt_dir: "/rt".into(), reaper: Path::new("/reaper"), settings: &s };
        let cmd = l.command("waitforexitandrun");
        let a = args(&cmd);
        let exe = a.iter().position(|x| x.ends_with("Client-Win64-Shipping.exe")).unwrap();
        assert_eq!(a[exe], "Z:\\games\\x\\Client\\Binaries\\Win64\\Client-Win64-Shipping.exe");
        assert_eq!(&a[exe + 1..], ["-dx12", "-krqlv=hd", "-foo", "-bar"]);
        assert_eq!(env(&cmd, "SteamGameId").as_deref(), Some("3513350"));
        assert_eq!(env(&cmd, "VKD3D_FRAME_RATE").as_deref(), Some("120"));
    }

    #[test]
    fn player_env_wins() {
        let mut i = install(GameId::Genshin);
        i.env.insert("DXVK_HUD".into(), "full".into());
        let s = Settings { performance_mode: false, ..Settings::default() };
        let l = Launch { install: &i, runner_dir: "/r".into(), steamrt_dir: "/rt".into(), reaper: Path::new("/reaper"), settings: &s };
        assert_eq!(env(&l.command("run"), "DXVK_HUD").as_deref(), Some("full"));
    }
}
