import { useEffect, useState } from "react";
import { openPath, openUrl } from "@tauri-apps/plugin-opener";
import { FolderOpen } from "lucide-react";
import { BottleIcon, DecanterIcon, DialIcon, LayersIcon, LaunchIcon } from "./SectionIcons";
import { api, type AppInfo, type JobView, type RuntimeView, type Settings } from "../api";
import Toggle from "./Toggle";
import ProtonManager from "./ProtonManager";

type Section = "general" | "launch" | "proton" | "runtime" | "about";

const SECTIONS: { id: Section; label: string; Icon: (p: { active?: boolean }) => React.ReactElement }[] = [
  { id: "general", label: "General", Icon: DialIcon },
  { id: "launch", label: "Game launch", Icon: LaunchIcon },
  { id: "proton", label: "Proton", Icon: BottleIcon },
  { id: "runtime", label: "Steam Runtime", Icon: LayersIcon },
  { id: "about", label: "About", Icon: DecanterIcon },
];

interface Props {
  settings: Settings;
  job: JobView;
  act: (fn: () => Promise<unknown>) => Promise<void>;
  onSettings: (s: Settings) => void;
}

export default function SettingsPage({ settings, job, act, onSettings }: Props) {
  const [section, setSection] = useState<Section>(
    (import.meta.env.DEV ? new URLSearchParams(location.search).get("section") : null) as Section | null ?? "general",
  );
  const set = <K extends keyof Settings>(key: K, value: Settings[K]) => onSettings({ ...settings, [key]: value });

  return (
    <div className="flex h-full">
      <aside className="w-60 shrink-0 border-r border-ink-800 bg-ink-900/60 px-4 py-9">
        <h1 className="mb-6 px-3 text-2xl font-bold tracking-tight">Settings</h1>
        <div className="space-y-1">
          {SECTIONS.map((s) => (
            <button
              key={s.id}
              onClick={() => setSection(s.id)}
              className={`relative flex w-full items-center gap-3 rounded-xl px-3 py-2.5 text-sm font-medium transition-colors ${
                section === s.id ? "bg-ink-750 text-fg" : "text-muted hover:bg-ink-800 hover:text-fg"
              }`}
            >
              {section === s.id && <span className="absolute left-0 top-1/2 h-5 w-1 -translate-y-1/2 rounded-r bg-wine-400" />}
              <span className={section === s.id ? "text-wine-200" : ""}>
                <s.Icon active={section === s.id} />
              </span>
              {s.label}
            </button>
          ))}
        </div>
      </aside>

      <div className="min-w-0 flex-1 overflow-y-auto px-10 py-9">
        <div className="mx-auto max-w-3xl">
          {section === "general" && (
            <Page title="General">
              <Toggle
                title="Update games automatically"
                description="When you press Play, Decanter installs a new game version first. Proton is never updated on its own."
                checked={settings.auto_update_games}
                onChange={(v) => set("auto_update_games", v)}
              />
              <Toggle
                title="Minimize while playing"
                description="Tuck Decanter away while a game runs, and bring it back when the game closes."
                checked={settings.minimize_on_launch}
                onChange={(v) => set("minimize_on_launch", v)}
              />
            </Page>
          )}

          {section === "launch" && (
            <Page title="Game launch" subtitle="Applies to every game. Per-game options are behind the gear on the home screen.">
              <Toggle
                title="Native Wayland"
                description="PROTON_ENABLE_WAYLAND. Off uses XWayland, which paces frames more smoothly on KDE + NVIDIA."
                checked={settings.wayland}
                onChange={(v) => set("wayland", v)}
              />
              <Toggle
                title="Performance power profile"
                description="Hold the 'performance' profile while a game runs (power-profiles-daemon)."
                checked={settings.performance_mode}
                onChange={(v) => set("performance_mode", v)}
              />
              <Toggle
                title="DXVK HUD"
                description="FPS, frame times, GPU load and shader compiler activity in the corner."
                checked={settings.hud}
                onChange={(v) => set("hud", v)}
              />
              <FpsCap value={settings.fps_cap} onChange={(v) => set("fps_cap", v)} />
            </Page>
          )}

          {section === "proton" && (
            <Page title="Proton" subtitle="Install Proton builds, and choose whether they update themselves. Each game picks its own in its settings.">
              <Toggle
                title="Update Proton automatically"
                description="When you press Play, the game moves to the newest stable build of its Proton family first (the old build stays installed). Games set to keep their Proton are skipped. ZZZ ran choppy on GE-Proton11, so keep ZZZ on GE-Proton10-34 if that matters to you."
                checked={settings.auto_update_proton}
                onChange={(v) => set("auto_update_proton", v)}
              />
              <div className="pt-4">
                <ProtonManager job={job} act={act} />
              </div>
            </Page>
          )}

          {section === "runtime" && (
            <Page title="Steam Linux Runtime" subtitle="The container every Proton build runs in. Each build says which runtime it needs.">
              <Runtimes job={job} act={act} />
            </Page>
          )}

          {section === "about" && <About />}
        </div>
      </div>
    </div>
  );
}

function Page({ title, subtitle, children }: { title: string; subtitle?: string; children: React.ReactNode }) {
  return (
    <div className="animate-fade-in">
      <h2 className="text-xl font-bold">{title}</h2>
      {subtitle && <p className="mt-1 text-sm text-muted">{subtitle}</p>}
      <div className="mt-6 space-y-3">{children}</div>
    </div>
  );
}

function FpsCap({ value, onChange }: { value: number; onChange: (v: number) => void }) {
  const [text, setText] = useState(value === 0 ? "" : String(value));
  const commit = () => {
    const n = text.trim() === "" ? 0 : Number.parseInt(text, 10);
    if (Number.isFinite(n) && n >= 0 && n <= 1000) onChange(n);
    else setText(value === 0 ? "" : String(value));
  };
  return (
    <div className="card flex items-center justify-between gap-6 px-5 py-4">
      <div>
        <div className="font-semibold">FPS cap</div>
        <div className="mt-0.5 text-sm text-muted">Empty for no cap. DX11 via dxvk.conf, DX12 via VKD3D.</div>
      </div>
      <input
        className="field !w-28 text-right tabular-nums"
        inputMode="numeric"
        placeholder="None"
        value={text}
        onChange={(e) => setText(e.target.value.replace(/[^0-9]/g, ""))}
        onBlur={commit}
        onKeyDown={(e) => e.key === "Enter" && (e.target as HTMLInputElement).blur()}
      />
    </div>
  );
}

function Runtimes({ job, act }: { job: JobView; act: (fn: () => Promise<unknown>) => Promise<void> }) {
  const [list, setList] = useState<RuntimeView[]>([]);
  const running = job.status.state === "running";
  useEffect(() => {
    api.runtimes().then(setList).catch(() => setList([]));
  }, [running]);

  return (
    <>
      {list.map((rt) => (
        <div key={rt.runtime} className="card flex items-center justify-between px-5 py-4">
          <div>
            <div className="font-semibold">{rt.label}</div>
            <div className="mt-0.5 font-mono text-xs text-muted">{rt.ready ? rt.version ?? "installed" : "Not installed — downloaded when a Proton build needs it"}</div>
          </div>
          <span className={`rounded-md px-2 py-0.5 text-xs font-semibold ${rt.ready ? "bg-ok/15 text-ok" : "bg-white/5 text-muted"}`}>
            {rt.ready ? "Installed" : "Not installed"}
          </span>
        </div>
      ))}
      <button className="btn btn-ghost" disabled={running || !list.some((r) => r.ready)} onClick={() => act(api.updateRuntimes)}>
        Check for runtime updates
      </button>
    </>
  );
}

function About() {
  const [info, setInfo] = useState<AppInfo | null>(null);
  useEffect(() => {
    api.appInfo().then(setInfo).catch(() => setInfo(null));
  }, []);
  return (
    <div className="animate-fade-in flex flex-col items-center pt-10 text-center">
      <img src="/decanter.svg" alt="" className="h-24 w-24 drop-shadow-[0_10px_30px_rgba(194,48,90,0.35)]" />
      <h2 className="mt-5 text-3xl font-bold tracking-tight">Decanter</h2>
      <p className="mt-1 text-muted">Pick a game, pick a Proton, play.</p>
      {info && <p className="mt-3 font-mono text-sm text-faint">v{info.version}</p>}
      <div className="mt-8 flex gap-3">
        {info && (
          <button className="btn btn-primary" onClick={() => void openUrl(info.repo_url)}>
            <GitHubMark /> GitHub
          </button>
        )}
        {info && (
          <button className="btn btn-ghost" onClick={() => void openPath(info.data_dir)}>
            <FolderOpen size={16} /> Data folder
          </button>
        )}
      </div>
      <p className="mt-10 text-xs text-faint">Free software under the GNU GPL v3.</p>
    </div>
  );
}

function GitHubMark() {
  return (
    <svg viewBox="0 0 16 16" width="16" height="16" fill="currentColor" aria-hidden="true">
      <path d="M8 0C3.58 0 0 3.58 0 8c0 3.54 2.29 6.53 5.47 7.59.4.07.55-.17.55-.38 0-.19-.01-.82-.01-1.49-2.01.37-2.53-.49-2.69-.94-.09-.23-.48-.94-.82-1.13-.28-.15-.68-.52-.01-.53.63-.01 1.08.58 1.23.82.72 1.21 1.87.87 2.33.66.07-.52.28-.87.51-1.07-1.78-.2-3.64-.89-3.64-3.95 0-.87.31-1.59.82-2.15-.08-.2-.36-1.02.08-2.12 0 0 .67-.21 2.2.82.64-.18 1.32-.27 2-.27.68 0 1.36.09 2 .27 1.53-1.04 2.2-.82 2.2-.82.44 1.1.16 1.92.08 2.12.51.56.82 1.27.82 2.15 0 3.07-1.87 3.75-3.65 3.95.29.25.54.73.54 1.48 0 1.07-.01 1.93-.01 2.2 0 .21.15.46.55.38A8.013 8.013 0 0016 8c0-4.42-3.58-8-8-8z" />
    </svg>
  );
}
