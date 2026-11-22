import { useState } from "react";
import { CalendarDays, Clock, Download, Loader2, Pause, Play, RotateCw, Settings2 } from "lucide-react";
import { api, type GameView, type JobView, type Settings } from "../api";
import { bytes, percent } from "../format";
import GameIcon from "./GameIcon";
import InstallDialog from "./InstallDialog";
import GameSettings from "./GameSettings";

interface Props {
  game: GameView;
  job: JobView;
  settings: Settings | null;
  act: (fn: () => Promise<unknown>) => Promise<void>;
  onShowDownloads: () => void;
  onOpenProtonSettings: () => void;
}

export default function Home({ game, job, settings, act, onShowDownloads, onOpenProtonSettings }: Props) {
  const [installing, setInstalling] = useState(false);
  const [editing, setEditing] = useState(false);
  const [artLoaded, setArtLoaded] = useState(false);

  const status = job.status;
  const mine = status.state !== "idle" && status.game === game.id;
  const running = status.state === "running";
  const busyElsewhere = running && !mine;
  const gameRunning = job.running_game === game.id;
  const autoUpdate = settings?.auto_update_games ?? true;

  return (
    <div className="absolute inset-0 overflow-hidden">
      {/* Art: the publisher's current key art. */}
      <div className="absolute inset-0 bg-gradient-to-br from-ink-800 via-ink-900 to-wine-900/40" />
      {game.art.background && (
        <img
          key={game.art.background}
          src={game.art.background}
          alt=""
          draggable={false}
          onLoad={() => setArtLoaded(true)}
          className={`absolute inset-0 h-full w-full object-cover ${artLoaded ? "animate-art-in" : "opacity-0"}`}
        />
      )}
      <div className="absolute inset-0 bg-gradient-to-t from-ink-950/90 via-ink-950/10 to-transparent" />
      <div className="absolute inset-0 bg-gradient-to-r from-ink-950/50 via-transparent to-transparent" />

      {/* Play time, top right. */}
      {game.install && (
        <div className="glass animate-fade-in absolute right-6 top-6 flex items-center gap-5 rounded-2xl px-5 py-3">
          <Stat icon={<Clock size={16} />} label="Play time" value={game.play_time} />
          <div className="h-8 w-px bg-white/10" />
          <Stat icon={<CalendarDays size={16} />} label="Last played" value={game.last_played ?? "Never"} />
        </div>
      )}

      {/* Bottom row: game card left, actions right. */}
      <div className="absolute inset-x-6 bottom-6 flex items-end justify-between gap-6">
        <div className="glass animate-fade-in flex min-w-0 items-center gap-4 rounded-2xl p-4 pr-6">
          <GameIcon game={game} size={60} />
          <div className="min-w-0">
            <h1 className="truncate text-2xl font-bold tracking-tight">{game.name}</h1>
            <div className="mt-1 flex items-center gap-2 text-sm text-muted">
              {game.installed_version && <span>v{game.installed_version}</span>}
              <StatusPill game={game} />
            </div>
          </div>
        </div>

        <div className="flex shrink-0 flex-col items-end gap-3">
          {mine && (running || status.state === "paused") && !gameRunning && (
            <ProgressStrip job={job} paused={status.state === "paused"} onClick={onShowDownloads} />
          )}
          {!autoUpdate && game.status === "update_available" && !running && (
            <button
              onClick={() => act(() => api.install(game.id, null, null))}
              className="glass rounded-full px-4 py-1.5 text-sm font-semibold text-wine-200 hover:text-white"
            >
              Update to {game.latest_version}
            </button>
          )}
          <div className="flex items-center gap-3">
            {game.install && (
              <button
                title="Game settings"
                aria-label="Game settings"
                onClick={() => setEditing(true)}
                className="glass flex h-14 w-14 items-center justify-center rounded-full text-fg transition-colors hover:bg-white/10"
              >
                <Settings2 size={22} />
              </button>
            )}
            <MainButton
              game={game}
              job={job}
              mine={mine}
              busyElsewhere={busyElsewhere}
              gameRunning={gameRunning}
              autoUpdate={autoUpdate}
              onInstall={() => setInstalling(true)}
              act={act}
            />
          </div>
        </div>
      </div>

      {installing && (
        <InstallDialog
          game={game}
          onClose={() => setInstalling(false)}
          onInstall={(dir, runner) => {
            setInstalling(false);
            void act(() => api.install(game.id, dir, runner));
          }}
        />
      )}
      {editing && game.install && (
        <GameSettings
          game={game}
          onClose={() => setEditing(false)}
          onSaved={() => act(async () => {})}
          onGetMoreProton={() => {
            setEditing(false);
            onOpenProtonSettings();
          }}
        />
      )}
    </div>
  );
}

function Stat({ icon, label, value }: { icon: React.ReactNode; label: string; value: string }) {
  return (
    <div className="flex items-center gap-3">
      <span className="text-wine-300">{icon}</span>
      <div>
        <div className="label-caps !text-[10px] !text-white/50">{label}</div>
        <div className="text-sm font-semibold">{value}</div>
      </div>
    </div>
  );
}

function StatusPill({ game }: { game: GameView }) {
  const map = {
    not_installed: { text: "Not installed", cls: "bg-white/10 text-muted" },
    incomplete: { text: "Download unfinished", cls: "bg-warn/15 text-warn" },
    ready: { text: "Ready", cls: "bg-ok/15 text-ok" },
    update_available: { text: `Update ${game.latest_version ?? ""}`.trim(), cls: "bg-wine-500/25 text-wine-200" },
  } as const;
  const { text, cls } = map[game.status];
  return <span className={`rounded-md px-2 py-0.5 text-xs font-semibold ${cls}`}>{text}</span>;
}

function ProgressStrip({ job, paused, onClick }: { job: JobView; paused: boolean; onClick: () => void }) {
  const p = job.progress;
  const known = p.total > 0;
  const pct = percent(p.done, p.total);
  return (
    <button onClick={onClick} className="glass w-80 rounded-2xl px-4 py-3 text-left transition-colors hover:bg-white/5">
      <div className="flex items-center justify-between gap-3 text-sm">
        <span className="truncate font-medium">{paused ? "Paused" : p.label || "Working…"}</span>
        <span className="shrink-0 tabular-nums text-muted">{known ? `${pct.toFixed(1)}%` : ""}</span>
      </div>
      <div className="mt-2 h-1.5 overflow-hidden rounded-full bg-white/10">
        {known ? (
          <div className="h-full rounded-full bg-wine-400 transition-[width] duration-500" style={{ width: `${pct}%` }} />
        ) : (
          !paused && <div className="animate-indeterminate h-full w-2/5 rounded-full bg-wine-400" />
        )}
      </div>
      {known && (
        <div className="mt-1.5 text-xs text-muted">
          {bytes(p.done)} of {bytes(p.total)}
        </div>
      )}
    </button>
  );
}

interface MainProps {
  game: GameView;
  job: JobView;
  mine: boolean;
  busyElsewhere: boolean;
  gameRunning: boolean;
  autoUpdate: boolean;
  onInstall: () => void;
  act: (fn: () => Promise<unknown>) => Promise<void>;
}

function MainButton({ game, job, mine, busyElsewhere, gameRunning, autoUpdate, onInstall, act }: MainProps) {
  const base =
    "flex h-14 min-w-52 items-center justify-center gap-3 rounded-full px-8 text-lg font-bold tracking-wide shadow-lg transition-all active:scale-[0.98] disabled:opacity-50";
  const primary = `${base} bg-wine-600 text-white shadow-wine-900/60 hover:bg-wine-500`;
  const status = job.status;

  if (gameRunning) {
    return (
      <button disabled className={`${base} bg-ink-700 text-fg`}>
        <Loader2 size={22} className="animate-spin" /> Running
      </button>
    );
  }
  if (mine && status.state === "running") {
    return (
      <button onClick={() => act(api.pause)} className={`${base} bg-ink-700 text-fg hover:bg-ink-600`}>
        <Pause size={22} /> Pause
      </button>
    );
  }
  if (mine && status.state === "paused") {
    const resume = status.kind === "play" ? () => api.play(game.id) : () => api.install(game.id, null, null);
    return (
      <button onClick={() => act(resume)} className={primary}>
        <RotateCw size={20} /> Resume
      </button>
    );
  }
  if (busyElsewhere) {
    const title = job.status.state === "running" ? job.status.title : "";
    return (
      <button disabled title={`Busy: ${title}`} className={primary}>
        <Loader2 size={20} className="animate-spin" /> Busy
      </button>
    );
  }
  switch (game.status) {
    case "not_installed":
      return (
        <button onClick={onInstall} className={primary}>
          <Download size={22} /> Install
        </button>
      );
    case "incomplete":
      return (
        <button onClick={() => act(() => api.install(game.id, null, null))} className={primary}>
          <RotateCw size={20} /> Resume
        </button>
      );
    case "update_available":
    case "ready":
      return (
        <button
          onClick={() => act(() => api.play(game.id))}
          className={primary}
          title={game.status === "update_available" && autoUpdate ? `Updates to ${game.latest_version} first` : undefined}
        >
          <Play size={22} fill="currentColor" /> Play
        </button>
      );
  }
}
