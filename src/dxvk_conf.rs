//! The dxvk.conf the launcher owns. Rewritten before every launch so an old
//! hand edit (like the `dxgi.syncInterval = 0` that forced V-Sync off) can
//! never sneak back in.

use crate::config::Config;
use std::fs;
use std::io;

pub fn write(cfg: &Config) -> io::Result<()> {
    // Retire the old hand-written file in the game folder once, so nothing
    // launched another way picks it up either.
    let old = cfg.game_dir.join("dxvk.conf");
    let backup = cfg.game_dir.join("dxvk.conf.bak");
    if old.exists() && !backup.exists() {
        let _ = fs::rename(&old, &backup);
    }

    let mut text = String::from(
        "# Written by wine_launcher before every launch. Edits here are overwritten.\n\
         # V-Sync is NOT overridden: the in-game V-Sync setting is what counts.\n",
    );
    if cfg.fps_cap > 0 {
        // DX11 path. The DX12 path is capped with VKD3D_FRAME_RATE instead.
        text.push_str(&format!("dxgi.maxFrameRate = {}\n", cfg.fps_cap));
    }

    let path = cfg.dxvk_conf_path();
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let temp = path.with_extension("conf.tmp");
    fs::write(&temp, text)?;
    fs::rename(&temp, &path)
}
