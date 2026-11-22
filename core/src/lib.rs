//! Decanter's engine: games, Proton runners, Steam Linux Runtimes and the
//! launch chain. No GUI code lives here; the Tauri app in `src-tauri` is a
//! thin layer over [`backend::Backend`].

pub mod archive;
pub mod backend;
pub mod dxvk_conf;
pub mod games;
pub mod installs;
pub mod launch;
pub mod migrate;
pub mod net;
pub mod paths;
pub mod playtime;
pub mod prefix;
pub mod progress;
pub mod runners;
pub mod settings;
pub mod steamrt;

pub use backend::Backend;
pub use games::GameId;
pub use paths::Paths;
