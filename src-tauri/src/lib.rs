//! Decanter's Tauri layer: turns UI calls into `decanter_core::Backend`
//! calls. No logic of its own beyond window handling.

use decanter_core::backend::{
    GameView, InstallOptions, InstalledRunner, JobView, ReleaseView, RuntimeView,
};
use decanter_core::runners::Family;
use decanter_core::settings::Settings;
use decanter_core::{Backend, GameId, Paths};
use serde::Serialize;
use std::path::PathBuf;
use tauri::{Manager, State};

const REPO_URL: &str = env!("CARGO_PKG_REPOSITORY");

/// Errors cross to the UI as plain text.
type Res<T> = Result<T, String>;

fn err(e: impl std::fmt::Display) -> String {
    e.to_string()
}

/// Run blocking backend work (network, disk) off the UI thread.
async fn blocking<T: Send + 'static>(f: impl FnOnce() -> anyhow::Result<T> + Send + 'static) -> Res<T> {
    tauri::async_runtime::spawn_blocking(f).await.map_err(err)?.map_err(|e| format!("{e:#}"))
}

#[derive(Serialize)]
struct AppInfo {
    version: &'static str,
    repo_url: &'static str,
    data_dir: PathBuf,
}

#[derive(Serialize)]
struct FamilyView {
    id: Family,
    label: &'static str,
    repo: &'static str,
}

#[tauri::command]
fn app_info(b: State<'_, Backend>) -> AppInfo {
    AppInfo { version: env!("CARGO_PKG_VERSION"), repo_url: REPO_URL, data_dir: b.paths().root.clone() }
}

#[tauri::command]
fn get_games(b: State<'_, Backend>) -> Vec<GameView> {
    b.games()
}

#[tauri::command]
fn get_selected(b: State<'_, Backend>) -> Option<GameId> {
    b.selected()
}

#[tauri::command]
fn select_game(b: State<'_, Backend>, game: GameId) -> Res<()> {
    b.select(game).map_err(|e| format!("{e:#}"))
}

#[tauri::command]
async fn refresh_game(b: State<'_, Backend>, game: GameId, force: bool) -> Res<()> {
    let b = b.inner().clone();
    blocking(move || b.refresh(game, force)).await
}

#[tauri::command]
fn install_game(b: State<'_, Backend>, game: GameId, dir: Option<PathBuf>, runner: Option<String>) -> Res<()> {
    b.install(game, dir, runner).map_err(|e| format!("{e:#}"))
}

#[tauri::command]
fn play_game(app: tauri::AppHandle, b: State<'_, Backend>, game: GameId) -> Res<()> {
    b.play(game).map_err(|e| format!("{e:#}"))?;
    if b.settings().minimize_on_launch {
        // Minimize once the game process is actually up (after any update),
        // and bring the window back when it closes.
        let b = b.inner().clone();
        std::thread::spawn(move || watch_game_window(app, b, game));
    }
    Ok(())
}

/// Minimize while `game` runs, restore afterwards.
fn watch_game_window(app: tauri::AppHandle, b: Backend, game: GameId) {
    use std::time::Duration;
    let Some(window) = app.get_webview_window("main") else { return };
    let mut minimized = false;
    loop {
        std::thread::sleep(Duration::from_millis(500));
        let view = b.view();
        if view.running_game == Some(game) && !minimized {
            let _ = window.minimize();
            minimized = true;
        }
        if !b.is_busy() {
            if minimized {
                let _ = window.unminimize();
                let _ = window.set_focus();
            }
            return;
        }
    }
}

#[tauri::command]
fn pause_job(b: State<'_, Backend>) {
    b.pause();
}

#[tauri::command]
fn dismiss_job(b: State<'_, Backend>) {
    b.dismiss();
}

#[tauri::command]
fn get_job(b: State<'_, Backend>) -> JobView {
    b.view()
}

#[tauri::command]
fn set_install_options(b: State<'_, Backend>, game: GameId, options: InstallOptions) -> Res<()> {
    b.set_install_options(game, options).map_err(|e| format!("{e:#}"))
}

#[tauri::command]
fn runner_families() -> Vec<FamilyView> {
    Family::ALL.iter().map(|&f| FamilyView { id: f, label: f.label(), repo: f.repo() }).collect()
}

#[tauri::command]
async fn runner_releases(b: State<'_, Backend>, family: Family, force: bool) -> Res<Vec<ReleaseView>> {
    let b = b.inner().clone();
    blocking(move || b.runner_releases(family, force)).await
}

#[tauri::command]
fn installed_runners(b: State<'_, Backend>) -> Vec<InstalledRunner> {
    b.installed_runners()
}

#[tauri::command]
async fn install_runner(b: State<'_, Backend>, family: Family, name: String) -> Res<()> {
    let b = b.inner().clone();
    // Looking the release up may hit GitHub, so off the UI thread.
    blocking(move || b.install_runner(family, name)).await
}

#[tauri::command]
fn remove_runner(b: State<'_, Backend>, name: String) -> Res<()> {
    b.remove_runner(&name).map_err(|e| format!("{e:#}"))
}

#[tauri::command]
fn runtimes(b: State<'_, Backend>) -> Vec<RuntimeView> {
    b.runtimes()
}

#[tauri::command]
fn update_runtimes(b: State<'_, Backend>) -> Res<()> {
    b.update_runtimes().map_err(|e| format!("{e:#}"))
}

#[tauri::command]
fn get_settings(b: State<'_, Backend>) -> Settings {
    b.settings()
}

#[tauri::command]
fn set_settings(b: State<'_, Backend>, settings: Settings) -> Res<()> {
    b.set_settings(settings).map_err(|e| format!("{e:#}"))
}

pub fn run() {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            let paths = Paths::new(Paths::default_root());
            log::info!("data folder: {}", paths.root.display());
            let backend = Backend::new(paths).map_err(|e| format!("{e:#}"))?;
            app.manage(backend);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            app_info,
            get_games,
            get_selected,
            select_game,
            refresh_game,
            install_game,
            play_game,
            pause_job,
            dismiss_job,
            get_job,
            set_install_options,
            runner_families,
            runner_releases,
            installed_runners,
            install_runner,
            remove_runner,
            runtimes,
            update_runtimes,
            get_settings,
            set_settings,
        ])
        .run(tauri::generate_context!())
        .expect("error while running Decanter");
}
