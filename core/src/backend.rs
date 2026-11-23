//! The only thing the UI talks to.
//!
//! Reads (`games`, `view`, `settings`, ...) are cheap snapshots. Work that
//! takes time (installing, updating, playing, installing Proton) runs as a
//! *job* on its own thread, one at a time; the UI polls [`Backend::view`] for
//! progress. A sampler thread turns byte counters into speeds for the
//! download graph and keeps play time saved.

use crate::games::{self, Art, GameId, GraphicsOption, Publisher};
use crate::installs::{Install, State};
use crate::launch::Launch;
use crate::paths::Paths;
use crate::playtime::{self, PlayStats};
use crate::progress::{Phase, Progress, Snapshot};
use crate::runners::{self, Family, Release};
use crate::settings::{self, Settings};
use crate::steamrt::{self, Runtime};
use anyhow::{Context, Result, anyhow, bail};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap, VecDeque};
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::PathBuf;
use std::sync::{Arc, Mutex, Weak};
use std::thread;
use std::time::{Duration, Instant};

/// A launch that exits non-zero sooner than this counts as "didn't start".
const EARLY_EXIT: Duration = Duration::from_secs(30);
const SAMPLE_EVERY: Duration = Duration::from_millis(500);
/// 60 seconds of speed history for the graph.
const HISTORY: usize = 120;
/// How long a fetched "latest version" is trusted for the home screen.
const CATALOG_FOR: Duration = Duration::from_secs(15 * 60);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum JobKind {
    Install,
    Play,
    Runner,
    Runtime,
}

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum JobStatus {
    Idle,
    Running { kind: JobKind, game: Option<GameId>, title: String },
    Paused { kind: JobKind, game: Option<GameId>, title: String },
    Done { kind: JobKind, game: Option<GameId>, message: String },
    Failed { kind: JobKind, game: Option<GameId>, message: String, details: String },
}

/// Everything the downloads view and the play button need, at one moment.
#[derive(Debug, Clone, Serialize)]
pub struct JobView {
    pub status: JobStatus,
    pub progress: Snapshot,
    /// Bytes per second.
    pub net_bps: u64,
    pub disk_bps: u64,
    pub peak_bps: u64,
    /// Network speed samples, oldest first, one per half second.
    pub history: Vec<u64>,
    pub eta_secs: Option<u64>,
    /// A game whose process is running right now.
    pub running_game: Option<GameId>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum GameStatus {
    /// No install entry.
    NotInstalled,
    /// An install entry, but the download never finished.
    Incomplete,
    /// Installed; latest unknown or equal.
    Ready,
    /// Installed and the publisher has a newer build.
    UpdateAvailable,
}

#[derive(Debug, Clone, Serialize)]
pub struct GameView {
    pub id: GameId,
    pub name: &'static str,
    pub short: &'static str,
    pub publisher: Publisher,
    pub visible: bool,
    pub notes: &'static str,
    pub status: GameStatus,
    pub install: Option<Install>,
    pub installed_version: Option<String>,
    pub latest_version: Option<String>,
    pub download_size: Option<u64>,
    pub install_size: Option<u64>,
    pub art: Art,
    pub graphics_options: &'static [GraphicsOption],
    pub default_graphics: &'static str,
    pub play_time: String,
    pub last_played: Option<String>,
    pub default_dir: PathBuf,
    /// The chosen Proton build is installed (else Play downloads it first).
    pub runner_installed: bool,
    pub default_runner: &'static str,
    pub components: &'static [&'static str],
}

#[derive(Debug, Clone, Serialize)]
pub struct InstalledRunner {
    #[serde(flatten)]
    pub runner: runners::Installed,
    pub used_by: Vec<GameId>,
    /// A newer non-prerelease build of the same family (from the cached list).
    pub newer: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ReleaseView {
    #[serde(flatten)]
    pub release: Release,
    pub installed: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct RuntimeView {
    pub runtime: Runtime,
    pub label: &'static str,
    pub ready: bool,
    pub version: Option<String>,
}

/// Per-install options the player can change.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct InstallOptions {
    pub runner: Option<String>,
    /// Some("") picks the game's default.
    pub graphics: Option<String>,
    pub voice: Option<String>,
    pub extra_args: Option<String>,
    pub env: Option<BTreeMap<String, String>>,
    pub pin_runner: Option<bool>,
}

#[derive(Clone)]
struct CatalogEntry {
    fetched: Instant,
    version: String,
    build_id: String,
    download_size: u64,
    install_size: u64,
}

#[derive(Default)]
struct Sampler {
    last: Option<(Instant, u64, u64, u64)>, // (at, net, disk, done)
    net_bps: u64,
    disk_bps: u64,
    peak_bps: u64,
    done_rate: f64,
    history: VecDeque<u64>,
}

struct Inner {
    paths: Paths,
    agent: ureq::Agent,
    state: Mutex<State>,
    settings: Mutex<Settings>,
    progress: Arc<Progress>,
    status: Mutex<JobStatus>,
    stats: Mutex<HashMap<GameId, Arc<PlayStats>>>,
    catalog: Mutex<HashMap<GameId, CatalogEntry>>,
    art: Mutex<HashMap<GameId, Art>>,
    sampler: Mutex<Sampler>,
    running: Mutex<Option<GameId>>,
}

/// Cheap to clone; all clones are the same backend.
#[derive(Clone)]
pub struct Backend {
    inner: Arc<Inner>,
}

impl Backend {
    /// Load state (adopting the old ZZZ-only setup on first start) and start
    /// the sampler thread.
    pub fn new(paths: Paths) -> Result<Self> {
        Self::open(paths, Some(crate::migrate::legacy_root()))
    }

    /// Like [`Backend::new`], with the old launcher's folder given explicitly
    /// (None = never adopt anything).
    pub fn open(paths: Paths, legacy: Option<PathBuf>) -> Result<Self> {
        std::fs::create_dir_all(&paths.root).with_context(|| format!("creating {}", paths.root.display()))?;
        let (state, settings) = crate::migrate::load_or_adopt(&paths, legacy.as_deref()).context("loading installs.json")?;
        let art = load_art_cache(&paths);
        let inner = Arc::new(Inner {
            agent: crate::net::agent(),
            state: Mutex::new(state),
            settings: Mutex::new(settings),
            progress: Arc::new(Progress::default()),
            status: Mutex::new(JobStatus::Idle),
            stats: Mutex::new(HashMap::new()),
            catalog: Mutex::new(HashMap::new()),
            art: Mutex::new(art),
            sampler: Mutex::new(Sampler::default()),
            running: Mutex::new(None),
            paths,
        });
        spawn_sampler(Arc::downgrade(&inner));
        Ok(Self { inner })
    }

    pub fn paths(&self) -> &Paths {
        &self.inner.paths
    }

    // ---------------------------------------------------------------- games

    pub fn games(&self) -> Vec<GameView> {
        let state = self.inner.state.lock().unwrap().clone();
        let catalog = self.inner.catalog.lock().unwrap().clone();
        let art = self.inner.art.lock().unwrap().clone();
        GameId::ALL
            .iter()
            .map(|&id| {
                let info = id.info();
                let install = state.get(id).cloned();
                let built = install.as_ref().and_then(|i| games::installed_build(&i.dir));
                let latest = catalog.get(&id);
                let status = match (&install, &built, latest) {
                    (None, _, _) => GameStatus::NotInstalled,
                    (Some(_), None, _) => GameStatus::Incomplete,
                    (Some(_), Some(b), Some(l)) if *b != l.build_id => GameStatus::UpdateAvailable,
                    (Some(_), Some(_), _) => GameStatus::Ready,
                };
                let stats = self.stats(id);
                GameView {
                    id,
                    name: info.name,
                    short: info.short,
                    publisher: info.publisher,
                    visible: info.visible,
                    notes: info.notes,
                    status,
                    runner_installed: install.as_ref().is_some_and(|i| runners::is_installed(&self.inner.paths, &i.runner)),
                    default_runner: info.default_runner,
                    components: info.components,
                    installed_version: built.as_deref().map(|b| games::version_of(b).to_string()),
                    latest_version: latest.map(|l| l.version.clone()),
                    download_size: latest.map(|l| l.download_size),
                    install_size: latest.map(|l| l.install_size),
                    art: art.get(&id).cloned().unwrap_or_default(),
                    graphics_options: info.graphics,
                    default_graphics: info.default_graphics,
                    play_time: playtime::describe(stats.total()),
                    last_played: stats.last_played(),
                    default_dir: self.inner.paths.game_default_dir(id),
                    install,
                }
            })
            .collect()
    }

    pub fn selected(&self) -> Option<GameId> {
        self.inner.state.lock().unwrap().selected
    }

    pub fn select(&self, game: GameId) -> Result<()> {
        self.update_state(|s| {
            s.selected = Some(game);
            Ok(())
        })
    }

    /// Ask the publisher for the latest build and art (network; blocking).
    /// Cached for 15 minutes unless `force`.
    pub fn refresh(&self, game: GameId, force: bool) -> Result<()> {
        let fresh = self.inner.catalog.lock().unwrap().get(&game).is_some_and(|c| c.fetched.elapsed() < CATALOG_FOR);
        if fresh && !force {
            return Ok(());
        }
        let opts = self.options_for(game);
        let art = games::fetch_art(&self.inner.agent, game);
        if let Ok(art) = &art {
            self.inner.art.lock().unwrap().insert(game, art.clone());
            save_art_cache(&self.inner.paths, &self.inner.art.lock().unwrap());
        }
        let latest = games::fetch_latest(&self.inner.agent, game, &opts)?;
        self.remember(&latest);
        art.map(|_| ())
    }

    /// Install a game (or resume/repair into an existing folder). `dir`
    /// defaults to the data root; pointing it at an existing install reuses
    /// every file that's already correct.
    pub fn install(&self, game: GameId, dir: Option<PathBuf>, runner: Option<String>) -> Result<()> {
        if let Some(dir) = &dir {
            if !dir.is_absolute() {
                bail!("the install folder must be an absolute path");
            }
        }
        if let Some(r) = &runner {
            if !runners::safe_name(r) {
                bail!("odd runner name {r:?}");
            }
        }
        // Refuse before touching installs.json, so a refused install never
        // leaves a half-made entry behind.
        if self.is_busy() {
            bail!("another task is still running");
        }
        let paths = &self.inner.paths;
        self.update_state(|s| {
            let install = match s.get(game).cloned() {
                Some(mut existing) => {
                    if let Some(dir) = dir {
                        existing.dir = dir;
                    }
                    if let Some(r) = runner {
                        existing.runner = r;
                    }
                    existing
                }
                None => Install {
                    game,
                    dir: dir.unwrap_or_else(|| paths.game_default_dir(game)),
                    prefix: paths.prefix_default_dir(game),
                    runner: runner.unwrap_or_else(|| game.info().default_runner.to_string()),
                    graphics: None,
                    voice: None,
                    extra_args: String::new(),
                    env: BTreeMap::new(),
                    pin_runner: false,
                },
            };
            s.put(install);
            s.selected = Some(game);
            Ok(())
        })?;
        let title = format!("Installing {}", game.info().name);
        self.start(JobKind::Install, Some(game), title, move |b, progress| {
            b.sync_game(game, progress)?;
            b.prepare_runner(game, progress)?;
            Ok(format!("{} is installed and ready to play.", game.info().name))
        })
    }

    /// Update (if enabled and needed), make sure Proton and the runtime are
    /// there, then run the game until it exits.
    pub fn play(&self, game: GameId) -> Result<()> {
        if self.inner.state.lock().unwrap().get(game).is_none() {
            bail!("{} isn't installed", game.info().name);
        }
        let title = format!("Starting {}", game.info().name);
        self.start(JobKind::Play, Some(game), title, move |b, progress| {
            let install = b.install_of(game)?;
            let auto = b.inner.settings.lock().unwrap().auto_update_games;
            if auto {
                b.sync_game(game, progress)?;
            } else if !games::is_complete(&install.dir) {
                bail!("the install isn't finished yet; press Install to resume it");
            }
            if b.inner.settings.lock().unwrap().auto_update_proton {
                b.update_runner(game, progress);
            }
            let (runner_dir, runtime) = b.prepare_runner(game, progress)?;
            b.prepare_prefix(game, &runner_dir, runtime, progress)?;
            b.run_game(game, runner_dir, runtime, progress)
        })
    }

    /// Ask the running job to stop at the next safe point. Downloads resume
    /// from where they stopped when started again.
    pub fn pause(&self) {
        self.inner.progress.cancel();
    }

    /// Change how a game is launched. Takes effect on the next Play.
    pub fn set_install_options(&self, game: GameId, opts: InstallOptions) -> Result<()> {
        if let Some(r) = &opts.runner {
            if !runners::safe_name(r) {
                bail!("odd runner name {r:?}");
            }
        }
        if let Some(env) = &opts.env {
            if env.keys().any(|k| k.is_empty() || k.contains('=') || k.contains('\0')) {
                bail!("environment variable names can't be empty or contain '='");
            }
        }
        self.update_state(|s| {
            let install = s.get_mut(game).with_context(|| format!("{} isn't installed", game.info().name))?;
            if let Some(r) = opts.runner {
                install.runner = r;
            }
            if let Some(g) = opts.graphics {
                install.graphics = (!g.is_empty()).then_some(g);
            }
            if let Some(v) = opts.voice {
                install.voice = Some(v);
            }
            if let Some(a) = opts.extra_args {
                install.extra_args = a;
            }
            if let Some(e) = opts.env {
                install.env = e;
            }
            if let Some(p) = opts.pin_runner {
                install.pin_runner = p;
            }
            Ok(())
        })
    }

    // -------------------------------------------------------------- runners

    pub fn runner_releases(&self, family: Family, force: bool) -> Result<Vec<ReleaseView>> {
        let list = runners::releases(&self.inner.agent, &self.inner.paths, family, force)?;
        Ok(list
            .into_iter()
            .map(|release| ReleaseView { installed: runners::is_installed(&self.inner.paths, &release.name), release })
            .collect())
    }

    /// Installed builds, which games use them, and whether a newer build of
    /// the same family is out (from cached lists; no network).
    pub fn installed_runners(&self) -> Vec<InstalledRunner> {
        let state = self.inner.state.lock().unwrap().clone();
        let mut lists: HashMap<Family, Vec<Release>> = HashMap::new();
        runners::installed(&self.inner.paths)
            .into_iter()
            .map(|runner| {
                let newer = runner.family.and_then(|f| {
                    let list = lists.entry(f).or_insert_with(|| cached_releases(&self.inner.paths, f));
                    runners::newer_than(list, &runner.name).map(|r| r.name)
                });
                let used_by = state.installs.iter().filter(|i| i.runner == runner.name).map(|i| i.game).collect();
                InstalledRunner { runner, used_by, newer }
            })
            .collect()
    }

    pub fn install_runner(&self, family: Family, name: String) -> Result<()> {
        let release = runners::releases(&self.inner.agent, &self.inner.paths, family, false)?
            .into_iter()
            .find(|r| r.name == name)
            .with_context(|| format!("{name} isn't a published {} build", family.label()))?;
        let title = format!("Installing {}", release.name);
        self.start(JobKind::Runner, None, title, move |b, progress| {
            runners::install(&b.inner.agent, &b.inner.paths, &release, progress)?;
            let runtime = steamrt::required_runtime(&b.inner.paths.runner_dir(&release.name))?;
            steamrt::ensure_present(&b.inner.agent, &b.inner.paths, runtime, progress)?;
            Ok(format!("{} is installed.", release.name))
        })
    }

    pub fn remove_runner(&self, name: &str) -> Result<()> {
        if self.is_busy() {
            bail!("wait for the current task to finish first");
        }
        let users: Vec<&str> = self
            .inner
            .state
            .lock()
            .unwrap()
            .installs
            .iter()
            .filter(|i| i.runner == name)
            .map(|i| i.game.info().name)
            .collect();
        if !users.is_empty() {
            bail!("{name} is used by {}; pick another Proton for it first", users.join(", "));
        }
        runners::remove(&self.inner.paths, name)
    }

    // ------------------------------------------------------------- runtimes

    pub fn runtimes(&self) -> Vec<RuntimeView> {
        Runtime::ALL
            .iter()
            .map(|&rt| RuntimeView {
                runtime: rt,
                label: rt.label(),
                ready: steamrt::ready(&self.inner.paths, rt),
                version: steamrt::installed_version(&self.inner.paths, rt),
            })
            .collect()
    }

    /// Update every installed Steam Linux Runtime to Steam's latest build.
    pub fn update_runtimes(&self) -> Result<()> {
        self.start(JobKind::Runtime, None, "Updating Steam Linux Runtimes".into(), |b, progress| {
            let mut updated = Vec::new();
            for rt in Runtime::ALL {
                if steamrt::ready(&b.inner.paths, rt) && steamrt::ensure(&b.inner.agent, &b.inner.paths, rt, progress)? {
                    updated.push(rt.label());
                }
            }
            Ok(if updated.is_empty() {
                "The Steam Linux Runtimes are up to date.".to_string()
            } else {
                format!("Updated: {}.", updated.join(", "))
            })
        })
    }

    // ------------------------------------------------------------- settings

    pub fn settings(&self) -> Settings {
        self.inner.settings.lock().unwrap().clone()
    }

    pub fn set_settings(&self, new: Settings) -> Result<()> {
        settings::save(&self.inner.paths.settings_file(), &new)?;
        *self.inner.settings.lock().unwrap() = new;
        Ok(())
    }

    // ------------------------------------------------------------------ job

    pub fn view(&self) -> JobView {
        let sampler = self.inner.sampler.lock().unwrap();
        let progress = self.inner.progress.snapshot();
        let remaining = progress.total.saturating_sub(progress.done);
        let eta_secs = (sampler.done_rate > 1.0 && remaining > 0 && progress.phase == Phase::Downloading)
            .then(|| (remaining as f64 / sampler.done_rate) as u64);
        JobView {
            status: self.inner.status.lock().unwrap().clone(),
            progress,
            net_bps: sampler.net_bps,
            disk_bps: sampler.disk_bps,
            peak_bps: sampler.peak_bps,
            history: sampler.history.iter().copied().collect(),
            eta_secs,
            running_game: *self.inner.running.lock().unwrap(),
        }
    }

    pub fn is_busy(&self) -> bool {
        matches!(*self.inner.status.lock().unwrap(), JobStatus::Running { .. })
    }

    /// Clear a finished, failed or paused job's message.
    pub fn dismiss(&self) {
        let mut status = self.inner.status.lock().unwrap();
        if !matches!(*status, JobStatus::Running { .. }) {
            *status = JobStatus::Idle;
        }
    }

    // ------------------------------------------------------------ internals

    fn stats(&self, game: GameId) -> Arc<PlayStats> {
        let mut stats = self.inner.stats.lock().unwrap();
        Arc::clone(stats.entry(game).or_insert_with(|| Arc::new(PlayStats::load(self.inner.paths.playtime_file(game)))))
    }

    fn install_of(&self, game: GameId) -> Result<Install> {
        self.inner.state.lock().unwrap().get(game).cloned().with_context(|| format!("{} isn't installed", game.info().name))
    }

    fn options_for(&self, game: GameId) -> games::Options {
        games::Options { voice: self.inner.state.lock().unwrap().get(game).and_then(|i| i.voice.clone()) }
    }

    fn update_state(&self, change: impl FnOnce(&mut State) -> Result<()>) -> Result<()> {
        let mut state = self.inner.state.lock().unwrap();
        let mut next = state.clone();
        change(&mut next)?;
        crate::installs::save(&self.inner.paths.installs_file(), &next)?;
        *state = next;
        Ok(())
    }

    fn remember(&self, latest: &games::Latest) {
        self.inner.catalog.lock().unwrap().insert(
            latest.game,
            CatalogEntry {
                fetched: Instant::now(),
                version: latest.version.clone(),
                build_id: latest.build_id.clone(),
                download_size: latest.download_size,
                install_size: latest.install_size,
            },
        );
    }

    /// Bring the game's folder to the publisher's latest build. Offline with
    /// a finished install: keep playing what's there, with a note.
    fn sync_game(&self, game: GameId, progress: &Arc<Progress>) -> Result<()> {
        let install = self.install_of(game)?;
        let name = game.info().name;
        progress.set_phase(Phase::Preparing);
        progress.set_label(format!("Checking {name} for updates"));
        let latest = match games::fetch_latest(&self.inner.agent, game, &self.options_for(game)) {
            Ok(l) => l,
            Err(e) if games::is_complete(&install.dir) => {
                progress.note(format!("Couldn't check {name} for updates ({e:#}), so the installed version was used."));
                return Ok(());
            }
            Err(e) => return Err(e.context(format!("couldn't reach the {name} servers"))),
        };
        self.remember(&latest);
        if games::installed_build(&install.dir).as_deref() == Some(latest.build_id.as_str()) {
            return Ok(());
        }
        games::install(&latest, &install.dir, progress)
    }

    /// Make sure the game's Proton build and the runtime it needs are
    /// installed. Downloads only what's missing; never updates behind the
    /// player's back.
    fn prepare_runner(&self, game: GameId, progress: &Arc<Progress>) -> Result<(PathBuf, Runtime)> {
        let install = self.install_of(game)?;
        let paths = &self.inner.paths;
        if !runners::is_installed(paths, &install.runner) {
            let family = Family::of_name(&install.runner)
                .with_context(|| format!("{} isn't installed and Decanter doesn't know where to get it", install.runner))?;
            let release = runners::releases(&self.inner.agent, paths, family, false)?
                .into_iter()
                .find(|r| r.name == install.runner)
                .with_context(|| format!("{} isn't installed and isn't published anymore; pick another Proton", install.runner))?;
            runners::install(&self.inner.agent, paths, &release, progress)?;
        }
        let runner_dir = paths.runner_dir(&install.runner);
        let runtime = steamrt::required_runtime(&runner_dir)?;
        if !steamrt::ready(paths, runtime) {
            progress.set_label(format!("Getting {} (one time only)", runtime.label()));
            steamrt::ensure_present(&self.inner.agent, paths, runtime, progress)?;
        }
        if !crate::reaper::ready(paths) {
            progress.set_label("Getting the process reaper (one time only)".to_string());
            crate::reaper::ensure(&self.inner.agent, paths)?;
        }
        Ok((runner_dir, runtime))
    }

    /// Proton auto-update: move the game to the newest stable build of its
    /// Proton family. Never blocks playing: any failure leaves the current
    /// build in place with a note. Pinned games and unknown builds are skipped.
    fn update_runner(&self, game: GameId, progress: &Arc<Progress>) {
        let Ok(install) = self.install_of(game) else { return };
        if install.pin_runner {
            return;
        }
        let Some(family) = Family::of_name(&install.runner) else { return };
        progress.set_phase(Phase::Preparing);
        progress.set_label(format!("Checking {} for updates", family.label()));
        let paths = &self.inner.paths;
        let newest = match runners::releases(&self.inner.agent, paths, family, false) {
            Ok(list) => runners::newer_than(&list, &install.runner),
            Err(e) => {
                progress.note(format!("Couldn't check {} for updates ({e:#}).", family.label()));
                return;
            }
        };
        let Some(newest) = newest else { return };
        if !runners::is_installed(paths, &newest.name) {
            if let Err(e) = runners::install(&self.inner.agent, paths, &newest, progress) {
                if progress.cancelled() {
                    return;
                }
                progress.note(format!("{} couldn't be installed ({e:#}), so {} was kept.", newest.name, install.runner));
                return;
            }
        }
        let switched = self.update_state(|s| {
            if let Some(i) = s.get_mut(game) {
                i.runner = newest.name.clone();
            }
            Ok(())
        });
        match switched {
            Ok(()) => progress.note(format!("Proton updated: {} now uses {} (was {}).", game.info().name, newest.name, install.runner)),
            Err(e) => progress.note(format!("Couldn't switch to {} ({e:#}).", newest.name)),
        }
    }

    /// One-time prefix setup before a launch: let Proton create the prefix,
    /// install the game's Windows components, and apply WuWa's SDK patch.
    fn prepare_prefix(&self, game: GameId, runner_dir: &PathBuf, runtime: Runtime, progress: &Arc<Progress>) -> Result<()> {
        let install = self.install_of(game)?;
        let info = game.info();
        let missing = crate::components::missing(&install.prefix, info.components);
        if !missing.is_empty() {
            let settings = self.settings();
            let launch = Launch {
                install: &install,
                runner_dir: runner_dir.clone(),
                steamrt_dir: self.inner.paths.steamrt_dir(runtime),
                reaper: &crate::reaper::path(&self.inner.paths),
                settings: &settings,
            };
            progress.set_phase(Phase::Installing);
            progress.set_label(format!("Setting up {}'s Wine prefix (one time only)", info.name));
            launch.init_prefix()?;
            crate::components::install(&self.inner.agent, &self.inner.paths, &install.prefix, runner_dir, info.components, progress)?;
        }
        if game == GameId::Wuwa && games::kuro::apply_channel_patch(&install.dir)? {
            log::info!("applied the Kuro SDK channel patch");
        }
        Ok(())
    }

    fn run_game(&self, game: GameId, runner_dir: PathBuf, runtime: Runtime, progress: &Progress) -> Result<String> {
        let install = self.install_of(game)?;
        let settings = self.settings();
        let launch = Launch {
            install: &install,
            runner_dir,
            steamrt_dir: self.inner.paths.steamrt_dir(runtime),
            reaper: &crate::reaper::path(&self.inner.paths),
            settings: &settings,
        };
        let name = game.info().name;
        progress.set_phase(Phase::Running);
        progress.set_bars(0, 0, 0, 0);
        progress.set_label(format!("{name} is running"));
        let stats = self.stats(game);
        let started = Instant::now();
        let mut child = launch.spawn()?;
        *self.inner.running.lock().unwrap() = Some(game);
        let status = {
            let _session = stats.start_session();
            child.wait()
        };
        *self.inner.running.lock().unwrap() = None;
        let status = status.context("waiting for the game")?;
        if status.success() || started.elapsed() >= EARLY_EXIT {
            Ok(format!("{name} has closed."))
        } else {
            Err(anyhow!("{name} exited right away ({status}). The servers may be slow, or the Proton build may not suit it."))
        }
    }

    /// Run `job` on its own thread. Refused while another job runs.
    fn start<F>(&self, kind: JobKind, game: Option<GameId>, title: String, job: F) -> Result<()>
    where
        F: FnOnce(&Backend, &Arc<Progress>) -> Result<String> + Send + 'static,
    {
        {
            let mut status = self.inner.status.lock().unwrap();
            if matches!(*status, JobStatus::Running { .. }) {
                bail!("another task is still running");
            }
            *status = JobStatus::Running { kind, game, title: title.clone() };
        }
        self.inner.progress.reset();
        *self.inner.sampler.lock().unwrap() = Sampler::default();
        let backend = self.clone();
        thread::Builder::new().name("decanter-job".into()).spawn(move || {
            let progress = Arc::clone(&backend.inner.progress);
            let result = catch_unwind(AssertUnwindSafe(|| job(&backend, &progress)))
                .unwrap_or_else(|_| Err(anyhow!("the task crashed unexpectedly")));
            let notes = progress.take_notes();
            let next = match result {
                Ok(mut message) => {
                    for note in notes {
                        message.push('\n');
                        message.push_str(&note);
                    }
                    JobStatus::Done { kind, game, message }
                }
                Err(_) if progress.cancelled() => JobStatus::Paused { kind, game, title },
                Err(e) => {
                    log::warn!("job failed: {e:#}");
                    JobStatus::Failed { kind, game, message: friendly(kind, game, &e), details: format!("{e:#}") }
                }
            };
            *backend.inner.status.lock().unwrap() = next;
        })?;
        Ok(())
    }
}

/// One plain sentence about what went wrong; the details line has the rest.
fn friendly(kind: JobKind, game: Option<GameId>, e: &anyhow::Error) -> String {
    let text = format!("{e:#}");
    let name = game.map_or("the game", |g| g.info().name);
    if text.contains("not enough disk space") {
        return "There isn't enough free disk space for this yet. Free some up and try again; it continues where it stopped.".into();
    }
    match kind {
        JobKind::Install => format!("{name} didn't finish downloading. Nothing is lost: try again and it continues where it stopped."),
        JobKind::Play => format!("{name} couldn't start."),
        JobKind::Runner => "That Proton build couldn't be installed.".into(),
        JobKind::Runtime => "The Steam Linux Runtime couldn't be updated.".into(),
    }
}

fn cached_releases(paths: &Paths, family: Family) -> Vec<Release> {
    // Never hits the network: an ancient cache is fine for an "update available" hint.
    #[derive(Deserialize)]
    struct Cached {
        releases: Vec<Release>,
    }
    let file = paths.cache_dir().join(family.cache_file());
    std::fs::read(file).ok().and_then(|b| serde_json::from_slice::<Cached>(&b).ok()).map(|c| c.releases).unwrap_or_default()
}

fn art_cache_file(paths: &Paths) -> PathBuf {
    paths.cache_dir().join("art.json")
}

fn load_art_cache(paths: &Paths) -> HashMap<GameId, Art> {
    std::fs::read(art_cache_file(paths)).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default()
}

fn save_art_cache(paths: &Paths, art: &HashMap<GameId, Art>) {
    if let Ok(bytes) = serde_json::to_vec(art) {
        let _ = crate::paths::write_atomic(&art_cache_file(paths), &bytes);
    }
}

/// Twice a second: turn counters into speeds, and save running play time.
/// Stops when the backend is dropped.
fn spawn_sampler(inner: Weak<Inner>) {
    let _ = thread::Builder::new().name("decanter-sampler".into()).spawn(move || {
        loop {
            thread::sleep(SAMPLE_EVERY);
            let Some(inner) = inner.upgrade() else { return };
            let (net, disk, reported) = inner.progress.counters();
            let done = inner.progress.snapshot().done;
            let busy = matches!(*inner.status.lock().unwrap(), JobStatus::Running { .. });
            {
                let mut s = inner.sampler.lock().unwrap();
                let now = Instant::now();
                if let Some((at, last_net, last_disk, last_done)) = s.last {
                    let secs = now.duration_since(at).as_secs_f64().max(0.001);
                    let (net_bps, disk_bps) = reported.unwrap_or((
                        (net.saturating_sub(last_net) as f64 / secs) as u64,
                        (disk.saturating_sub(last_disk) as f64 / secs) as u64,
                    ));
                    s.net_bps = net_bps;
                    s.disk_bps = disk_bps;
                    s.peak_bps = s.peak_bps.max(net_bps);
                    let rate = done.saturating_sub(last_done) as f64 / secs;
                    // Smooth the ETA so it doesn't jump around every half second.
                    s.done_rate = if s.done_rate == 0.0 { rate } else { s.done_rate * 0.9 + rate * 0.1 };
                    if busy {
                        s.history.push_back(net_bps);
                        while s.history.len() > HISTORY {
                            s.history.pop_front();
                        }
                    }
                }
                s.last = Some((now, net, disk, done));
            }
            for stats in inner.stats.lock().unwrap().values() {
                stats.checkpoint();
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn backend(tmp: &tempfile::TempDir) -> Backend {
        Backend::open(Paths::new(tmp.path().join("data")), None).unwrap()
    }

    #[test]
    fn fresh_backend_lists_every_game() {
        let tmp = tempfile::tempdir().unwrap();
        let b = backend(&tmp);
        let games = b.games();
        assert_eq!(games.len(), 5);
        assert!(games.iter().any(|g| g.id == GameId::Zzz && g.visible));
        assert!(matches!(b.view().status, JobStatus::Idle));
    }

    #[test]
    fn options_validate_env_names() {
        let tmp = tempfile::tempdir().unwrap();
        let b = backend(&tmp);
        b.update_state(|s| {
            s.put(Install {
                game: GameId::Zzz,
                dir: "/g".into(),
                prefix: "/p".into(),
                runner: "GE-Proton10-34".into(),
                graphics: None,
                voice: None,
                extra_args: String::new(),
                env: BTreeMap::new(),
                pin_runner: false,
            });
            Ok(())
        })
        .unwrap();
        let bad = InstallOptions { env: Some(BTreeMap::from([("A=B".to_string(), "x".to_string())])), ..Default::default() };
        assert!(b.set_install_options(GameId::Zzz, bad).is_err());
        let good = InstallOptions { graphics: Some("-use-d3d12".into()), ..Default::default() };
        b.set_install_options(GameId::Zzz, good).unwrap();
        assert_eq!(b.games()[0].install.as_ref().unwrap().graphics.as_deref(), Some("-use-d3d12"));
        // Saved to disk too.
        let saved = crate::installs::load(&b.paths().installs_file()).unwrap().unwrap();
        assert_eq!(saved.get(GameId::Zzz).unwrap().graphics.as_deref(), Some("-use-d3d12"));
    }
}

#[cfg(test)]
mod live {
    use super::*;

    /// First start against the real ~/wine_launcher, into a throwaway data
    /// root: adoption, runner/runtime resolution and the exact launch
    /// command, without launching anything.
    /// `cargo test -p decanter-core live_first_start -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn live_first_start() {
        let tmp = tempfile::tempdir().unwrap();
        let legacy = crate::migrate::legacy_root();
        let b = Backend::open(Paths::new(tmp.path().join("data")), Some(legacy.clone())).unwrap();
        let reaper = crate::reaper::ensure(&crate::net::agent(), b.paths()).unwrap();
        let zzz = b.games().into_iter().find(|g| g.id == GameId::Zzz).unwrap();
        println!("status={:?} version={:?} play_time={} runner_installed={}", zzz.status, zzz.installed_version, zzz.play_time, zzz.runner_installed);
        assert_eq!(zzz.status, GameStatus::Ready);
        assert!(zzz.runner_installed);
        b.refresh(GameId::Zzz, true).unwrap();
        let zzz = b.games().into_iter().find(|g| g.id == GameId::Zzz).unwrap();
        assert_eq!(zzz.status, GameStatus::Ready, "existing install must be up to date");
        println!("art: {:?}", zzz.art.background);

        let install = zzz.install.unwrap();
        let runner_dir = b.paths().runner_dir(&install.runner);
        let runtime = steamrt::required_runtime(&runner_dir).unwrap();
        println!("runner {} needs {:?}; ready={}", install.runner, runtime, steamrt::ready(b.paths(), runtime));
        assert_eq!(runtime, Runtime::Sniper);
        assert!(steamrt::ready(b.paths(), runtime));
        let settings = b.settings();
        let launch = Launch { install: &install, runner_dir, steamrt_dir: b.paths().steamrt_dir(runtime), reaper: &reaper, settings: &settings };
        launch.preflight().unwrap();
        let cmd = launch.command("waitforexitandrun");
        println!("{:?} {:?}", cmd.get_program(), cmd.get_args().collect::<Vec<_>>());
        for r in b.installed_runners() {
            println!("installed runner: {} {:?} used_by={:?}", r.runner.name, r.runner.runtime, r.used_by);
        }
        // Nothing in the old folder became a symlink or moved.
        for p in ["game", "prefix", "proton/ge-proton", "steamrt/steamrt3"] {
            let m = std::fs::symlink_metadata(legacy.join(p)).unwrap();
            assert!(m.is_dir() && !m.file_type().is_symlink(), "{p} changed");
        }
    }

    /// `cargo test -p decanter-core live_runner_lists -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn live_runner_lists() {
        let tmp = tempfile::tempdir().unwrap();
        let b = Backend::open(Paths::new(tmp.path().join("data")), None).unwrap();
        for f in Family::ALL {
            let list = b.runner_releases(f, true).unwrap();
            let verified = list.iter().filter(|r| r.release.sha256.is_some() || r.release.checksum_url.is_some()).count();
            println!("{}: {} releases ({verified} verifiable), newest {}", f.label(), list.len(), list[0].release.name);
            assert!(!list.is_empty());
        }
        let ge = b.runner_releases(Family::Ge, false).unwrap();
        let pinned = ge.iter().find(|r| r.release.name == runners::DEFAULT_RUNNER).expect("GE-Proton10-34 listed");
        assert_eq!(pinned.release.sha256.as_deref(), Some("51c580b66a833c73998fe00f0717eeac57197654040a2f2ed5189e3ee68d773d"));
    }
}
