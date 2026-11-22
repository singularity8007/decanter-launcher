//! The dxvk.conf Decanter owns, rewritten before every launch so an old hand
//! edit (like the `dxgi.syncInterval = 0` that forced V-Sync off) can never
//! sneak back in. Lives in the game's Proton data folder.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

pub fn path(prefix_dir: &Path) -> PathBuf {
    prefix_dir.join("dxvk.conf")
}

pub fn write(prefix_dir: &Path, game_dir: &Path, fps_cap: u32) -> io::Result<()> {
    // Retire an old hand-written file in the game folder once, so nothing
    // launched another way picks it up either.
    let old = game_dir.join("dxvk.conf");
    let backup = game_dir.join("dxvk.conf.bak");
    if old.exists() && !backup.exists() {
        let _ = fs::rename(&old, &backup);
    }

    let mut text = String::from(
        "# Written by Decanter before every launch. Edits here are overwritten.\n\
         # V-Sync is NOT overridden: the in-game V-Sync setting is what counts.\n",
    );
    if fps_cap > 0 {
        // DX11 path. The DX12 path is capped with VKD3D_FRAME_RATE instead.
        text.push_str(&format!("dxgi.maxFrameRate = {fps_cap}\n"));
    }
    crate::paths::write_atomic(&path(prefix_dir), text.as_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn writes_cap_and_retires_old_file() {
        let tmp = tempfile::tempdir().unwrap();
        let game = tmp.path().join("game");
        let pfx = tmp.path().join("pfx");
        fs::create_dir_all(&game).unwrap();
        fs::write(game.join("dxvk.conf"), "dxgi.syncInterval = 0").unwrap();
        write(&pfx, &game, 138).unwrap();
        assert!(fs::read_to_string(path(&pfx)).unwrap().contains("dxgi.maxFrameRate = 138"));
        assert!(game.join("dxvk.conf.bak").exists());
        assert!(!game.join("dxvk.conf").exists());
        write(&pfx, &game, 0).unwrap();
        assert!(!fs::read_to_string(path(&pfx)).unwrap().contains("maxFrameRate"));
    }
}
