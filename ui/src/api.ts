// Typed wrappers over the Tauri commands in src-tauri/src/lib.rs. The types
// mirror the Rust structs' serde output in core/src (snake_case fields).

import { invoke } from "@tauri-apps/api/core";

export type GameId = "zzz" | "hsr" | "genshin" | "wuwa" | "endfield";
export type Publisher = "hoyoverse" | "kuro" | "gryphline";
export type GameStatus = "not_installed" | "incomplete" | "ready" | "update_available";
export type Family = "ge" | "cachyos" | "em";
export type Runtime = "sniper" | "steamrt4";
export type JobKind = "install" | "play" | "runner" | "runtime";
export type Phase = "preparing" | "verifying" | "downloading" | "installing" | "running";

export interface Art {
  background: string | null;
  video: string | null;
  icon: string | null;
}

export interface GraphicsOption {
  label: string;
  args: string;
}

export interface Install {
  game: GameId;
  dir: string;
  prefix: string;
  runner: string;
  graphics: string | null;
  voice: string | null;
  extra_args: string;
  env: Record<string, string>;
}

export interface GameView {
  id: GameId;
  name: string;
  short: string;
  publisher: Publisher;
  visible: boolean;
  notes: string;
  status: GameStatus;
  install: Install | null;
  installed_version: string | null;
  latest_version: string | null;
  download_size: number | null;
  install_size: number | null;
  art: Art;
  graphics_options: GraphicsOption[];
  default_graphics: string;
  play_time: string;
  last_played: string | null;
  default_dir: string;
  runner_installed: boolean;
}

export type JobStatus =
  | { state: "idle" }
  | { state: "running"; kind: JobKind; game: GameId | null; title: string }
  | { state: "paused"; kind: JobKind; game: GameId | null; title: string }
  | { state: "done"; kind: JobKind; game: GameId | null; message: string }
  | { state: "failed"; kind: JobKind; game: GameId | null; message: string; details: string };

export interface Snapshot {
  label: string;
  phase: Phase;
  done: number;
  total: number;
  install_done: number;
  install_total: number;
}

export interface JobView {
  status: JobStatus;
  progress: Snapshot;
  net_bps: number;
  disk_bps: number;
  peak_bps: number;
  history: number[];
  eta_secs: number | null;
  running_game: GameId | null;
}

export interface Release {
  family: Family;
  tag: string;
  name: string;
  prerelease: boolean;
  published_at: string;
  url: string;
  archive_name: string;
  size: number;
  sha256: string | null;
  checksum_url: string | null;
  installed: boolean;
}

export interface InstalledRunner {
  name: string;
  family: Family | null;
  runtime: Runtime | null;
  problem: string | null;
  used_by: GameId[];
  newer: string | null;
}

export interface RuntimeView {
  runtime: Runtime;
  label: string;
  ready: boolean;
  version: string | null;
}

export interface FamilyView {
  id: Family;
  label: string;
  repo: string;
}

export interface Settings {
  wayland: boolean;
  hud: boolean;
  performance_mode: boolean;
  fps_cap: number;
  auto_update_games: boolean;
  minimize_on_launch: boolean;
}

export interface InstallOptions {
  runner?: string;
  graphics?: string;
  voice?: string;
  extra_args?: string;
  env?: Record<string, string>;
}

export interface AppInfo {
  version: string;
  repo_url: string;
  data_dir: string;
}

export const DEFAULT_RUNNER = "GE-Proton10-34";

export const api = {
  appInfo: () => invoke<AppInfo>("app_info"),
  games: () => invoke<GameView[]>("get_games"),
  selected: () => invoke<GameId | null>("get_selected"),
  select: (game: GameId) => invoke<void>("select_game", { game }),
  refresh: (game: GameId, force = false) => invoke<void>("refresh_game", { game, force }),
  install: (game: GameId, dir: string | null, runner: string | null) =>
    invoke<void>("install_game", { game, dir, runner }),
  play: (game: GameId) => invoke<void>("play_game", { game }),
  pause: () => invoke<void>("pause_job"),
  dismiss: () => invoke<void>("dismiss_job"),
  job: () => invoke<JobView>("get_job"),
  setInstallOptions: (game: GameId, options: InstallOptions) =>
    invoke<void>("set_install_options", { game, options }),
  families: () => invoke<FamilyView[]>("runner_families"),
  releases: (family: Family, force = false) => invoke<Release[]>("runner_releases", { family, force }),
  installedRunners: () => invoke<InstalledRunner[]>("installed_runners"),
  installRunner: (family: Family, name: string) => invoke<void>("install_runner", { family, name }),
  removeRunner: (name: string) => invoke<void>("remove_runner", { name }),
  runtimes: () => invoke<RuntimeView[]>("runtimes"),
  updateRuntimes: () => invoke<void>("update_runtimes"),
  settings: () => invoke<Settings>("get_settings"),
  setSettings: (settings: Settings) => invoke<void>("set_settings", { settings }),
};

/** Tauri rejects with the Rust error string; normalise anything else. */
export function errorText(e: unknown): string {
  if (typeof e === "string") return e;
  if (e instanceof Error) return e.message;
  return String(e);
}
