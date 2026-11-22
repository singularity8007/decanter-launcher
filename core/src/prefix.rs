//! The Proton data folder (STEAM_COMPAT_DATA_PATH) for a game.
//!
//! Proton creates and upgrades the Wine prefix inside it (`pfx/`) on every
//! launch, inside the Steam Linux Runtime, the same way Steam does. Decanter
//! only makes sure the folder exists. (Running `wineboot` from the host, as
//! the ZZZ-only launcher did, breaks with Proton builds that are made to run
//! inside the runtime, such as Proton-CachyOS "slr" builds.)

use std::fs;
use std::io;
use std::path::Path;

pub fn ensure(prefix_dir: &Path) -> io::Result<()> {
    fs::create_dir_all(prefix_dir)
}

/// True once Proton has created a prefix here.
pub fn exists(prefix_dir: &Path) -> bool {
    prefix_dir.join("pfx").join("system.reg").exists()
}
