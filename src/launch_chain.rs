use crate::config::Config;
use crate::{dxvk_conf, prefix};
use anyhow::Result;
use std::process::{Command, ExitStatus, Stdio};

/// HUD contents when the HUD switch is on: what's needed to judge smoothness.
const HUD: &str = "fps,frametimes,gpuload,compiler";

/// NVIDIA's shader disk cache is ~1 GB by default and the driver prunes it
/// when full, which throws away compiled pipelines and brings shader
/// stutter back every session. 10 GB, and never prune.
const GL_SHADER_CACHE_SIZE: &str = "10737418240";

/// Launch the game and block until it exits.
pub fn run(cfg: &Config) -> Result<ExitStatus> {
    // Create the Wine prefix on the first run; a no-op afterwards.
    prefix::ensure_prefix(cfg)?;
    dxvk_conf::write(cfg)?;
    let mut cmd = build(cfg, "waitforexitandrun");
    println!("launching: {cmd:?}");
    Ok(cmd.status()?)
}

/// The exe as Twintail passes it: through Proton's `s:` game drive
/// (`gamedrive`, mapped to the install folder), e.g. `s:\ZenlessZoneZero.exe`.
/// An exe outside the install folder is passed as a plain Linux path.
fn game_exe_arg(cfg: &Config) -> String {
    match cfg.exe.strip_prefix(&cfg.game_dir) {
        Ok(rel) if !rel.as_os_str().is_empty() => {
            format!("s:\\{}", rel.to_string_lossy().replace('/', "\\"))
        }
        _ => cfg.exe.to_string_lossy().into_owned(),
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

/// Build (but don't run) the command that launches `cfg.exe` inside the
/// Steam Linux Runtime, the same way Steam and TwintailLauncher do:
///
///   [powerprofilesctl launch -p performance --] _v2-entry-point --verb=V -- reaper SteamLaunch AppId=0 -- proton V exe [args]
///
/// `verb` is "run" (fire and forget) or "waitforexitandrun" (block until
/// the game exits).
pub fn build(cfg: &Config, verb: &str) -> Command {
    // Proton keeps its Wine prefix in <data path>/pfx.
    let pfx = cfg.prefix_dir.join("pfx");

    // STEAM_COMPAT_INSTALL_PATH is the game's install folder.
    let install_dir = &cfg.game_dir;

    // Hold the performance power profile for exactly as long as the game runs
    // (EPP=performance on intel_pstate: faster clock ramp, steadier frame times).
    // It's released automatically when the game exits.
    let mut cmd = if cfg.performance && power_profiles_available() {
        let mut c = Command::new("powerprofilesctl");
        c.args(["launch", "-p", "performance", "-r", "Playing ZZZ", "-i", "wine_launcher", "--"])
            .arg(cfg.entry_point());
        c
    } else {
        // The entry script that sets up the container. Everything about
        // bwrap/pressure-vessel stays hidden inside it.
        Command::new(cfg.entry_point())
    };

    cmd.arg(format!("--verb={verb}"))
        // Everything after this `--` is the program to run inside the container.
        .arg("--")
        // reaper cleans up leftover processes when the game exits. AppId=0
        // because we're not a real Steam game.
        .arg(&cfg.reaper_path)
        .args(["SteamLaunch", "AppId=0", "--"])
        // The runner (the `proton` script) followed by its verb and the exe.
        .arg(cfg.runner_dir.join("proton"))
        .arg(verb)
        .arg(game_exe_arg(cfg));

    if cfg.dx12 {
        cmd.arg("-use-d3d12");
    }

    // The game folder as working directory, like Twintail and the official launcher.
    cmd.current_dir(install_dir);

    // Environment that Proton expects Steam to have set up. Without these it
    // refuses to start, since we're not running under a real Steam client.
    cmd.env("WINEARCH", "win64")
        .env("WINEPREFIX", &pfx)
        .env("STEAM_COMPAT_APP_ID", "0")
        .env("STEAM_COMPAT_DATA_PATH", &cfg.prefix_dir)
        .env("STEAM_COMPAT_INSTALL_PATH", install_dir)
        .env("STEAM_COMPAT_CLIENT_INSTALL_PATH", "")
        .env("STEAM_COMPAT_TOOL_PATHS", &cfg.runner_dir)
        .env(
            "STEAM_COMPAT_LIBRARY_PATHS",
            format!("{}:{}", install_dir.display(), pfx.display()),
        )
        .env("STEAM_COMPAT_SHADER_PATH", cfg.prefix_dir.join("shadercache"))
        // `=d` disables a DLL. Both are Steam-only pieces we don't have.
        .env("WINEDLLOVERRIDES", "lsteamclient=d;KRSDKExternal.exe=d")
        .env("SteamGameId", "0")
        .env("SteamOS", "1")
        // Exactly TwintailLauncher's ZZZ profile (game-manifests nap_global.json):
        // no xalia helper process, the install folder as drive s:, and
        // protonfixes off.
        .env("STEAM_COMPAT_CONFIG", "noxalia,gamedrive")
        .env("PROTONFIXES_DISABLE", "1")
        // Keep compiled shaders between sessions (see GL_SHADER_CACHE_SIZE).
        .env("__GL_SHADER_DISK_CACHE", "1")
        .env("__GL_SHADER_DISK_CACHE_SIZE", GL_SHADER_CACHE_SIZE)
        .env("__GL_SHADER_DISK_CACHE_SKIP_CLEANUP", "1")
        // The launcher's own dxvk.conf (no forced V-Sync; optional FPS cap).
        .env("DXVK_CONFIG_FILE", cfg.dxvk_conf_path());

    // The HUD is off unless asked for, and an inherited one is cleared too.
    if cfg.hud {
        cmd.env("DXVK_HUD", HUD);
    } else {
        cmd.env_remove("DXVK_HUD");
    }

    if cfg.wayland {
        cmd.env("PROTON_ENABLE_WAYLAND", "1");
    } else {
        cmd.env_remove("PROTON_ENABLE_WAYLAND");
        cmd.env_remove("PROTON_USE_WAYLAND");
    }

    // The DX11 path is capped in dxvk.conf; DX12 goes through vkd3d-proton.
    if cfg.fps_cap > 0 {
        cmd.env("VKD3D_FRAME_RATE", cfg.fps_cap.to_string());
    }

    cmd
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `cargo test dry_run -- --nocapture` prints the exact launch command
    /// and environment without starting anything.
    #[test]
    fn dry_run() {
        let mut cfg = Config::hardcoded().unwrap();
        for (dx12, cap) in [(false, 0), (true, 138)] {
            cfg.dx12 = dx12;
            cfg.fps_cap = cap;
            let cmd = build(&cfg, "waitforexitandrun");
            println!("\n--- dx12={dx12} cap={cap}\nprogram: {:?}", cmd.get_program());
            println!("args: {:?}", cmd.get_args().collect::<Vec<_>>());
            println!("cwd: {:?}", cmd.get_current_dir());
            for (k, v) in cmd.get_envs() {
                println!("  {k:?} = {v:?}");
            }
            let args: Vec<_> = cmd.get_args().collect();
            assert_eq!(args.iter().any(|a| *a == "-use-d3d12"), dx12);
            assert!(args.iter().any(|a| *a == "s:\\ZenlessZoneZero.exe"));
        }
    }

    /// Full chain smoke test with a tiny console exe (no game):
    /// `cargo test smoke -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn smoke() {
        let mut cfg = Config::hardcoded().unwrap();
        cfg.exe = cfg.game_dir.parent().unwrap().join("test/real_exe/7zr.exe");
        cfg.performance = false;
        let status = run(&cfg).unwrap();
        println!("exit: {status}");
        assert!(status.success());
    }
}
