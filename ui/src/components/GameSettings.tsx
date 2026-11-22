import { useEffect, useState } from "react";
import { openPath } from "@tauri-apps/plugin-opener";
import { FolderOpen, Plus, Trash2 } from "lucide-react";
import { api, errorText, type GameView, type InstalledRunner } from "../api";
import Modal from "./Modal";

interface Props {
  game: GameView;
  onClose: () => void;
  onSaved: () => void;
  onGetMoreProton: () => void;
}

const VOICES = [
  { id: "en-us", label: "English" },
  { id: "ja-jp", label: "Japanese" },
  { id: "ko-kr", label: "Korean" },
  { id: "zh-cn", label: "Chinese" },
];

const RUNTIME_LABEL = { sniper: "SLR 3 (sniper)", steamrt4: "SLR 4" } as const;

export default function GameSettings({ game, onClose, onSaved, onGetMoreProton }: Props) {
  const install = game.install!;
  const [runners, setRunners] = useState<InstalledRunner[]>([]);
  const [runner, setRunner] = useState(install.runner);
  const [graphics, setGraphics] = useState(install.graphics ?? game.default_graphics);
  const [voice, setVoice] = useState(install.voice ?? "en-us");
  const [args, setArgs] = useState(install.extra_args);
  const [env, setEnv] = useState<[string, string][]>(Object.entries(install.env));
  const [pinned, setPinned] = useState(install.pin_runner);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    api.installedRunners().then(setRunners).catch(() => setRunners([]));
  }, []);

  const save = async () => {
    const cleaned = env.map(([k, v]) => [k.trim(), v] as const).filter(([k]) => k.length > 0);
    try {
      await api.setInstallOptions(game.id, {
        runner,
        graphics: graphics === game.default_graphics ? "" : graphics,
        voice: game.publisher === "hoyoverse" ? voice : undefined,
        extra_args: args,
        env: Object.fromEntries(cleaned),
        pin_runner: pinned,
      });
      onSaved();
      onClose();
    } catch (e) {
      setError(errorText(e));
    }
  };

  const usable = runners.filter((r) => r.runtime);
  const options = usable.some((r) => r.name === runner) ? usable : [...usable, { name: runner } as InstalledRunner];

  return (
    <Modal
      title={game.name}
      subtitle="Launch settings"
      onClose={onClose}
      wide
      footer={
        <>
          <button className="btn btn-ghost" onClick={onClose}>
            Cancel
          </button>
          <button className="btn btn-primary" onClick={save}>
            Save
          </button>
        </>
      }
    >
      <div className="space-y-6">
        <Section title="Folders">
          <PathRow label="Game" path={install.dir} />
          <PathRow label="Proton data (prefix)" path={install.prefix} />
        </Section>

        <Section title="Proton">
          <select className="field" value={runner} onChange={(e) => setRunner(e.target.value)}>
            {options.map((r) => (
              <option key={r.name} value={r.name}>
                {r.name}
                {r.runtime ? ` — ${RUNTIME_LABEL[r.runtime]}` : " — not installed, downloads on Play"}
              </option>
            ))}
          </select>
          <label className="mt-3 flex cursor-pointer items-center gap-3 text-sm">
            <input type="checkbox" className="h-4 w-4 accent-wine-500" checked={pinned} onChange={(e) => setPinned(e.target.checked)} />
            <span>
              Keep this Proton build <span className="text-muted">— skip Proton auto-update for this game</span>
            </span>
          </label>
          <button onClick={onGetMoreProton} className="mt-2 text-sm font-semibold text-wine-300 hover:text-wine-200">
            Get more Proton versions →
          </button>
        </Section>

        {game.graphics_options.length > 0 && (
          <Section title="Graphics API">
            <div className="flex gap-2">
              {game.graphics_options.map((g) => (
                <button
                  key={g.args}
                  onClick={() => setGraphics(g.args)}
                  className={`btn flex-1 ${graphics === g.args ? "btn-primary" : "btn-ghost"}`}
                >
                  {g.label}
                </button>
              ))}
            </div>
          </Section>
        )}

        {game.publisher === "hoyoverse" && (
          <Section title="Voice pack" hint="Changing it downloads the new pack the next time the game updates or you press Play.">
            <select className="field" value={voice} onChange={(e) => setVoice(e.target.value)}>
              {VOICES.map((v) => (
                <option key={v.id} value={v.id}>
                  {v.label}
                </option>
              ))}
            </select>
          </Section>
        )}

        <Section title="Extra launch arguments">
          <input className="field font-mono !text-[13px]" placeholder="-arg1 -arg2" value={args} onChange={(e) => setArgs(e.target.value)} />
        </Section>

        <Section title="Environment variables">
          <div className="space-y-2">
            {env.map(([k, v], i) => (
              <div key={i} className="flex gap-2">
                <input
                  className="field font-mono !text-[13px]"
                  placeholder="NAME"
                  value={k}
                  onChange={(e) => setEnv(env.map((row, j) => (j === i ? [e.target.value, row[1]] : row)))}
                />
                <input
                  className="field font-mono !text-[13px]"
                  placeholder="value"
                  value={v}
                  onChange={(e) => setEnv(env.map((row, j) => (j === i ? [row[0], e.target.value] : row)))}
                />
                <button aria-label="Remove" className="btn btn-ghost shrink-0" onClick={() => setEnv(env.filter((_, j) => j !== i))}>
                  <Trash2 size={15} />
                </button>
              </div>
            ))}
            <button className="btn btn-ghost" onClick={() => setEnv([...env, ["", ""]])}>
              <Plus size={15} /> Add variable
            </button>
          </div>
        </Section>

        {error && <div className="rounded-xl border border-wine-500/40 bg-wine-900/40 p-3 text-sm text-wine-200">{error}</div>}
      </div>
    </Modal>
  );
}

function Section({ title, hint, children }: { title: string; hint?: string; children: React.ReactNode }) {
  return (
    <section>
      <div className="label-caps mb-2">{title}</div>
      {children}
      {hint && <p className="mt-2 text-xs text-muted">{hint}</p>}
    </section>
  );
}

function PathRow({ label, path }: { label: string; path: string }) {
  return (
    <div className="mb-2 flex items-center gap-3 rounded-xl bg-ink-850 px-4 py-2.5">
      <div className="min-w-0 flex-1">
        <div className="text-xs text-muted">{label}</div>
        <div className="selectable truncate font-mono text-[13px]">{path}</div>
      </div>
      <button title="Open folder" aria-label={`Open ${label} folder`} className="btn btn-ghost shrink-0 !px-2.5" onClick={() => void openPath(path)}>
        <FolderOpen size={15} />
      </button>
    </div>
  );
}
