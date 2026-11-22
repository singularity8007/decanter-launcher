// Development-only stand-in for the Rust backend, so the UI can be worked on
// and screenshotted in a plain browser. Loaded only by `vite dev` when not
// inside Tauri; never part of a build.
//
// URL parameters: ?view=home|library|downloads|settings  &section=proton|...
//                 &job=idle|downloading|paused|failed|running  &status=ready|update_available|not_installed

import { mockIPC, mockWindows } from "@tauri-apps/api/mocks";
import type { GameView, InstalledRunner, JobView, Release, RuntimeView, Settings } from "./api";

const q = new URLSearchParams(location.search);
const jobKind = q.get("job") ?? "idle";
const status = (q.get("status") ?? "ready") as GameView["status"];

const zzz: GameView = {
  id: "zzz",
  name: "Zenless Zone Zero",
  short: "ZZZ",
  publisher: "hoyoverse",
  visible: true,
  notes: "",
  status,
  install:
    status === "not_installed"
      ? null
      : {
          game: "zzz",
          dir: "/home/hamza/wine_launcher/game",
          prefix: "/home/hamza/wine_launcher/prefix",
          runner: "GE-Proton10-34",
          graphics: null,
          voice: null,
          extra_args: "",
          env: {},
        },
  installed_version: status === "not_installed" ? null : "3.3.0",
  latest_version: status === "update_available" ? "3.4.0" : "3.3.0",
  download_size: 71_500_000_000,
  install_size: 79_000_000_000,
  art: {
    background:
      "https://fastcdn.hoyoverse.com/static-resource-v2/2026/11/09/ec930c54619b8284230739491e6fdc0a_8103163892211065846.webp",
    video: null,
    icon: "https://launcher-webstatic.hoyoverse.com/launcher-public/2026/09/02/db01dfe29c36a283b774d295b2322b54_5536403786224870636.png",
  },
  graphics_options: [
    { label: "DirectX 11", args: "" },
    { label: "DirectX 12", args: "-use-d3d12" },
  ],
  default_graphics: "",
  play_time: "6 hours 31 minutes",
  last_played: "Nov 20, 2026",
  default_dir: "/home/hamza/.local/share/decanter/games/zzz",
  runner_installed: true,
};

const hidden = (id: GameView["id"], name: string, short: string): GameView => ({
  ...zzz,
  id,
  name,
  short,
  visible: false,
  status: "not_installed",
  install: null,
  installed_version: null,
  art: { background: null, video: null, icon: null },
});

const games: GameView[] = [
  zzz,
  hidden("hsr", "Honkai: Star Rail", "HSR"),
  hidden("genshin", "Genshin Impact", "Genshin"),
  hidden("wuwa", "Wuthering Waves", "WuWa"),
  hidden("endfield", "Arknights: Endfield", "Endfield"),
];

const history = Array.from({ length: 120 }, (_, i) => {
  const base = 21_000_000 + Math.sin(i / 7) * 3_500_000 + Math.sin(i / 2.3) * 1_200_000;
  return i < 8 ? (base * i) / 8 : base;
});

function job(): JobView {
  const progress = { label: "Updating ZZZ to 3.4.0", phase: "downloading" as const, done: 2_310_000_000, total: 6_590_000_000, install_done: 0, install_total: 0 };
  const common = { progress, net_bps: 22_380_000, disk_bps: 61_200_000, peak_bps: 23_960_000, history, eta_secs: 191, running_game: null };
  switch (jobKind) {
    case "downloading":
      return { ...common, status: { state: "running", kind: "play", game: "zzz", title: "Starting Zenless Zone Zero" } };
    case "paused":
      return { ...common, net_bps: 0, status: { state: "paused", kind: "play", game: "zzz", title: "Starting Zenless Zone Zero" } };
    case "failed":
      return {
        ...common,
        history: [],
        status: { state: "failed", kind: "play", game: "zzz", message: "Zenless Zone Zero couldn't start.", details: "the Steam Linux Runtime is missing: /home/hamza/.local/share/decanter/steamrt/steamrt3/_v2-entry-point" },
      };
    case "running":
      return { ...common, progress: { ...progress, phase: "running", label: "Zenless Zone Zero is running" }, running_game: "zzz", status: { state: "running", kind: "play", game: "zzz", title: "Starting Zenless Zone Zero" } };
    default:
      return { ...common, history: [], net_bps: 0, status: { state: "idle" } };
  }
}

const settings: Settings = { wayland: false, hud: false, performance_mode: true, fps_cap: 0, auto_update_games: true, minimize_on_launch: true };

const installed: InstalledRunner[] = [
  { name: "GE-Proton10-34", family: "ge", runtime: "sniper", problem: null, used_by: ["zzz"], newer: "GE-Proton11-12" },
  { name: "proton-cachyos-11.0-20261015-slr", family: "cachyos", runtime: "sniper", problem: null, used_by: [], newer: null },
];

const releases: Release[] = ["GE-Proton11-12", "GE-Proton11-11", "GE-Proton11-10", "GE-Proton10-35", "GE-Proton10-34", "GE-Proton10-33"].map((name, i) => ({
  family: "ge",
  tag: name,
  name,
  prerelease: false,
  published_at: `2026-${String(11 - i).padStart(2, "0")}-0${(i % 9) + 1}T00:00:00Z`,
  url: "",
  archive_name: `${name}.tar.gz`,
  size: 566_000_000 - i * 9_000_000,
  sha256: "x".repeat(64),
  checksum_url: null,
  installed: name === "GE-Proton10-34",
}));

const runtimes: RuntimeView[] = [
  { runtime: "sniper", label: "Steam Linux Runtime 3 (sniper)", ready: true, version: "3.0.20261110.268881" },
  { runtime: "steamrt4", label: "Steam Linux Runtime 4", ready: true, version: "4.0.20261110.268882" },
];

export function installMock() {
  mockWindows("main");
  mockIPC((cmd) => {
    switch (cmd) {
      case "app_info":
        return { version: "0.1.0", repo_url: "https://github.com/singularity8007/wine_launcher", data_dir: "/home/hamza/.local/share/decanter" };
      case "get_games":
        return games;
      case "get_selected":
        return "zzz";
      case "get_job":
        return job();
      case "get_settings":
        return settings;
      case "runner_families":
        return [
          { id: "ge", label: "Proton GE", repo: "" },
          { id: "cachyos", label: "Proton CachyOS", repo: "" },
          { id: "em", label: "Proton EM", repo: "" },
        ];
      case "runner_releases":
        return releases;
      case "installed_runners":
        return installed;
      case "runtimes":
        return runtimes;
      default:
        return null;
    }
  });
}
