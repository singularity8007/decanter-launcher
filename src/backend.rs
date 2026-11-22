//! The only thing the window talks to.
//!
//! The GUI calls `launch()`, `install_game()` and `check_updates()`, reads and
//! saves the settings, and draws whatever `view()` returns. Everything else
//! (threads, downloads, play time,
//! the wording of messages, deciding whether a launch "failed") lives here or
//! further back, so no logic is duplicated between the two sides.

use crate::config::Config;
use crate::installer::GameStatus;
use crate::playtime::{self, PlayStats};
use crate::progress::Progress;
use crate::settings::{self, Field, Toggle};
use crate::{downloader, installer, launch_chain};
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

/// A launch that ends non-zero sooner than this counts as "the game didn't
/// start". Longer than this counts as a normal play session.
const EARLY_EXIT: Duration = Duration::from_secs(30);

// ---- Messages shown to the player. Kept gentle: say what probably happened
// and what to try, and keep the technical text for the "details" line. ----

const NOT_READY: &str = "ZZZ isn't ready to play yet. Press \"Install/Update ZZZ\" first. \
If you already did and it got interrupted, press it again to resume your download.";

const LAUNCH_FAILED: &str = "ZZZ didn't start this time. HoYo's servers may just be slow to answer. \
Check your internet connection, or press \"Check for updates\", then try again.";

const INSTALL_FAILED: &str = "ZZZ didn't finish downloading. Nothing is lost. \
Check your internet connection, then press \"Install/Update ZZZ\" again and it will carry on from where it stopped.";

const NO_DISK_SPACE: &str = "There isn't enough free disk space for this yet. \
Free up some space, then press \"Install/Update ZZZ\" again.";

const UPDATE_FAILED: &str = "Couldn't check for updates just now. This usually means the connection dropped. \
Check your internet and try again in a moment.";

const CRASHED: &str = "Something unexpected went wrong inside the launcher. \
Please try again; if it keeps happening, the details below can help.";

/// A job that didn't work out: friendly text for the player, plus the
/// technical reason in case they want to share it.
#[derive(Clone)]
pub struct Failure {
    text: String,
    details: Option<String>,
}

impl Failure {
    fn plain(text: impl Into<String>) -> Self {
        Self { text: text.into(), details: None }
    }

    fn with_details(text: impl Into<String>, details: impl std::fmt::Display) -> Self {
        Self { text: text.into(), details: Some(details.to_string()) }
    }
}

#[derive(Clone)]
enum State {
    Idle,
    Running,
    Done(String),
    Failed(Failure),
}

/// Something to tell the player. Errors are shown in a popup, good news in
/// the status line.
pub struct Notice {
    pub text: String,
    /// The technical reason, shown small and dim under an error.
    pub details: Option<String>,
    pub is_error: bool,
}

/// Everything the window needs to draw one frame, already decided.
pub struct View {
    /// A job is running: the window disables the buttons.
    pub busy: bool,
    /// What the job is doing right now.
    pub label: String,
    /// 0.0 to 1.0, or None when the job has no known size.
    pub fraction: Option<f32>,
    /// Result of the last job, if it has ended.
    pub notice: Option<Notice>,
    /// Total play time, e.g. "94 hours 37 minutes".
    pub play_time: String,
    /// Date of the last launch, e.g. "Oct 1, 2026", or "Never".
    pub last_played: String,
}

/// Cheap to clone: every field is shared, so all clones are the same backend.
#[derive(Clone)]
pub struct Backend {
    cfg: Arc<Config>,
    progress: Arc<Progress>,
    state: Arc<Mutex<State>>,
    stats: Arc<PlayStats>,
}

impl Backend {
    pub fn new(cfg: Config) -> Self {
        let stats = PlayStats::load(cfg.stats_path.clone());
        Self {
            cfg: Arc::new(cfg),
            progress: Arc::new(Progress::default()),
            state: Arc::new(Mutex::new(State::Idle)),
            stats: Arc::new(stats),
        }
    }

    /// Install ZZZ, or bring an existing install up to the latest version.
    /// Only changed files are downloaded.
    pub fn install_game(&self) {
        self.start(|cfg, progress| {
            installer::ensure_game(&cfg.game_dir, progress).map_err(|e| {
                let details = format!("{e:#}");
                let text = if details.contains("not enough disk space") {
                    NO_DISK_SPACE
                } else {
                    INSTALL_FAILED
                };
                Failure::with_details(text, details)
            })?;
            Ok("ZZZ is up to date and ready to play.".into())
        });
    }

    /// Update GE-Proton and the Steam Linux Runtime, and say whether ZZZ has
    /// a newer version (without downloading it).
    pub fn check_updates(&self) {
        self.start(|cfg, progress| {
            downloader::update_all(cfg, progress)
                .map_err(|e| Failure::with_details(UPDATE_FAILED, format!("{e:#}")))?;
            let game = match installer::check(&cfg.game_dir, progress) {
                Ok(GameStatus::UpToDate) => "ZZZ is up to date.".to_string(),
                Ok(GameStatus::UpdateAvailable) => "A new ZZZ version is out! Press \"Install/Update ZZZ\" to download it."
                    .to_string(),
                Ok(GameStatus::NotInstalled) => {
                    "ZZZ isn't installed yet. Press \"Install/Update ZZZ\" to get started."
                        .to_string()
                }
                // Offline or HoYo down: not a failure of the runtime update.
                Err(_) => "Couldn't check ZZZ just now. Try again in a moment.".to_string(),
            };
            Ok(format!(
                "Proton and the Steam Linux Runtime are up to date.\n{game}"
            ))
        });
    }

    /// Just try to run the game: no network, no update checks.
    pub fn launch(&self) {
        let stats = Arc::clone(&self.stats);
        self.start(move |cfg, progress| {
            // The only check: false while an install or update is unfinished,
            // so a half-updated game is never started. Paths are hardcoded and
            // not checked here; if one is missing, the launch itself fails.
            if !installer::is_complete(&cfg.game_dir) {
                return Err(Failure::plain(NOT_READY));
            }
            // Out of the box: if the pinned GE-Proton / Steam Runtime pair
            // isn't installed yet (first run, or an older launcher left
            // GE-Proton11 + SLR4 behind), fetch it now instead of asking.
            if !downloader::runtime_ready(cfg) {
                progress.set_label(format!(
                    "Getting {} and the Steam Linux Runtime (one time only)...",
                    downloader::GE_TAG
                ));
                downloader::update_all(cfg, progress)
                    .map_err(|e| Failure::with_details(UPDATE_FAILED, format!("{e:#}")))?;
                if !downloader::runtime_ready(cfg) {
                    return Err(Failure::with_details(
                        UPDATE_FAILED,
                        format!("{} or the Steam Linux Runtime is still missing", downloader::GE_TAG),
                    ));
                }
                progress.reset();
            }
            progress.set_label("ZZZ is running. Have fun!");
            let started = Instant::now();
            // Counts play time and stamps today's date; stops counting when
            // this goes out of scope, however the job ends.
            let _session = stats.start_session();
            match launch_chain::run(cfg) {
                // Clean exit, or it ran a normal length of time: a normal session.
                Ok(status) if status.success() || started.elapsed() >= EARLY_EXIT => {
                    Ok("ZZZ has closed.".into())
                }
                Ok(status) => Err(Failure::with_details(
                    LAUNCH_FAILED,
                    format!("exit status: {status}"),
                )),
                Err(e) => Err(Failure::with_details(LAUNCH_FAILED, format!("{e:#}"))),
            }
        });
    }

    /// The path in use right now for one box on the settings page.
    pub fn path_in_use(&self, field: Field) -> String {
        self.cfg.path(field).display().to_string()
    }

    /// Save a path the player typed. Called on every keystroke, so closing the
    /// launcher never loses a change. It takes effect the next time the
    /// launcher starts, because the running launcher already loaded its paths.
    pub fn set_path(&self, field: Field, path: &str) {
        if !field.editable() {
            return;
        }
        let mut saved = settings::load(&self.cfg.settings_path);
        saved.set(field, path);
        // Nothing else depends on this write, so a failure isn't worth a popup.
        let _ = settings::save(&self.cfg.settings_path, &saved);
    }

    /// A launch switch as it is in use right now.
    pub fn flag_in_use(&self, toggle: Toggle) -> bool {
        self.cfg.flag(toggle)
    }

    /// The FPS cap in use right now, "" for none.
    pub fn fps_cap_in_use(&self) -> String {
        match self.cfg.fps_cap {
            0 => String::new(),
            n => n.to_string(),
        }
    }

    /// Save a launch switch. Takes effect the next time the launcher starts.
    pub fn set_flag(&self, toggle: Toggle, on: bool) {
        let mut saved = settings::load(&self.cfg.settings_path);
        saved.set_flag(toggle, on);
        let _ = settings::save(&self.cfg.settings_path, &saved);
    }

    /// Save the FPS cap the player typed. Empty or 0 = no cap; anything that
    /// isn't a number is ignored.
    pub fn set_fps_cap(&self, typed: &str) {
        let typed = typed.trim();
        let cap = if typed.is_empty() { 0 } else {
            match typed.parse::<u32>() {
                Ok(n) => n,
                Err(_) => return,
            }
        };
        let mut saved = settings::load(&self.cfg.settings_path);
        saved.set_fps_cap(cap);
        let _ = settings::save(&self.cfg.settings_path, &saved);
    }

    /// The player closed the error popup.
    pub fn dismiss(&self) {
        let mut state = self.state.lock().unwrap();
        if matches!(*state, State::Failed(_)) {
            *state = State::Idle;
        }
    }

    /// True while a job (including a play session) is running.
    pub fn is_busy(&self) -> bool {
        matches!(*self.state.lock().unwrap(), State::Running)
    }

    /// Called regularly by the window: saves a running play session.
    pub fn tick(&self) {
        self.stats.checkpoint();
    }

    /// Snapshot for the window to draw. Cheap; called every frame.
    pub fn view(&self) -> View {
        let snap = self.progress.snapshot();
        let state = self.state.lock().unwrap().clone();
        View {
            busy: matches!(state, State::Running),
            label: snap.label,
            fraction: (snap.total > 0).then(|| snap.done as f32 / snap.total as f32),
            notice: match state {
                State::Done(text) => Some(Notice { text, details: None, is_error: false }),
                State::Failed(f) => Some(Notice {
                    text: f.text,
                    details: f.details,
                    is_error: true,
                }),
                State::Idle | State::Running => None,
            },
            play_time: playtime::describe(self.stats.total()),
            last_played: self.stats.last_played().unwrap_or_else(|| "Never".into()),
        }
    }

    /// Run `job` on its own thread so the window never freezes. Ignored if a
    /// job is already running. Warnings the job noted are appended to its result.
    fn start<F>(&self, job: F)
    where
        F: FnOnce(&Config, &Progress) -> Result<String, Failure> + Send + 'static,
    {
        {
            let mut state = self.state.lock().unwrap();
            if matches!(*state, State::Running) {
                return;
            }
            *state = State::Running;
        }
        self.progress.reset();

        let cfg = Arc::clone(&self.cfg);
        let progress = Arc::clone(&self.progress);
        let state = Arc::clone(&self.state);
        thread::spawn(move || {
            // catch_unwind so a panic in a job shows as an error instead of
            // leaving the window stuck on "running" forever.
            let result = catch_unwind(AssertUnwindSafe(|| job(&cfg, &progress)))
                .unwrap_or_else(|_| {
                    Err(Failure::with_details(CRASHED, "the job panicked"))
                });
            let notes = progress.take_notes();
            *state.lock().unwrap() = match result {
                Ok(mut text) => {
                    for note in notes {
                        text.push('\n');
                        text.push_str(&note);
                    }
                    State::Done(text)
                }
                Err(failure) => State::Failed(failure),
            };
        });
    }
}