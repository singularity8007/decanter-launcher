import { Activity, ArrowDownToLine, Gauge, HardDrive, Pause, RotateCw } from "lucide-react";
import { api, type GameView, type JobView, type Phase } from "../api";
import { bytes, duration, percent, speed } from "../format";
import SpeedGraph from "./SpeedGraph";

interface Props {
  job: JobView;
  games: GameView[];
  act: (fn: () => Promise<unknown>) => Promise<void>;
}

const PHASE: Record<Phase, string> = {
  preparing: "Preparing",
  verifying: "Verifying",
  downloading: "Downloading",
  installing: "Installing",
  running: "Running",
};

/** Plain grey by design: no game art here, just the numbers. */
export default function Downloads({ job, games, act }: Props) {
  const s = job.status;
  const active = s.state === "running" || s.state === "paused";
  const transfer = active && job.progress.phase !== "running";
  const game = active && s.game ? games.find((g) => g.id === s.game) : undefined;

  return (
    <div className="h-full overflow-y-auto px-10 py-9">
      <div className="flex items-end justify-between">
        <div>
          <h1 className="text-3xl font-bold tracking-tight">Downloads</h1>
          <p className="mt-1 text-muted">
            {transfer ? `${speed(job.net_bps)} • 1 active` : "Nothing is downloading right now."}
          </p>
        </div>
      </div>

      {transfer ? (
        <div className="card animate-fade-in mt-8 overflow-hidden">
          <div className="flex items-start justify-between gap-6 p-6">
            <div className="min-w-0">
              <div className="label-caps">{s.state === "paused" ? "Paused" : PHASE[job.progress.phase]}</div>
              <h2 className="mt-1 truncate text-2xl font-bold">{game?.name ?? s.title}</h2>
              <p className="mt-1 truncate text-sm text-muted">{job.progress.label}</p>
            </div>
            {s.state === "running" ? (
              <button className="btn btn-ghost shrink-0" onClick={() => act(api.pause)}>
                <Pause size={16} /> Pause
              </button>
            ) : (
              <button
                className="btn btn-primary shrink-0"
                onClick={() =>
                  act(() => {
                    if (s.state !== "paused" || !s.game) return Promise.resolve();
                    return s.kind === "play" ? api.play(s.game) : api.install(s.game, null, null);
                  })
                }
              >
                <RotateCw size={16} /> Resume
              </button>
            )}
          </div>

          <div className="grid grid-cols-4 gap-px border-y border-ink-700 bg-ink-700">
            <Metric icon={<Activity size={16} />} label="Network" value={speed(job.net_bps)} accent />
            <Metric icon={<Gauge size={16} />} label="Peak" value={speed(job.peak_bps)} />
            <Metric icon={<HardDrive size={16} />} label="Disk" value={speed(job.disk_bps)} />
            <Metric icon={<ArrowDownToLine size={16} />} label="Time left" value={duration(job.eta_secs)} />
          </div>

          <div className="space-y-4 p-6">
            <Bar label="Download" done={job.progress.done} total={job.progress.total} color="bg-wine-400" />
            {job.progress.install_total > 0 && (
              <Bar label="Install" done={job.progress.install_done} total={job.progress.install_total} color="bg-ok" />
            )}
          </div>

          <div className="border-t border-ink-700 bg-ink-900/60 px-6 pb-4 pt-5">
            <div className="label-caps mb-3">Network speed, last minute</div>
            <SpeedGraph samples={job.history} />
          </div>
        </div>
      ) : (
        <div className="card mt-8 flex flex-col items-center justify-center px-6 py-20 text-center">
          <ArrowDownToLine size={34} className="text-ink-500" />
          <div className="mt-4 font-semibold">No active downloads</div>
          <div className="mt-1 text-sm text-muted">Installs and updates show up here while they run.</div>
        </div>
      )}
    </div>
  );
}

function Metric({ icon, label, value, accent }: { icon: React.ReactNode; label: string; value: string; accent?: boolean }) {
  return (
    <div className="bg-ink-850 px-6 py-4">
      <div className="flex items-center gap-2 text-faint">
        {icon}
        <span className="label-caps">{label}</span>
      </div>
      <div className={`mt-1 text-lg font-bold tabular-nums ${accent ? "text-wine-300" : ""}`}>{value}</div>
    </div>
  );
}

function Bar({ label, done, total, color }: { label: string; done: number; total: number; color: string }) {
  const known = total > 0;
  const pct = percent(done, total);
  return (
    <div>
      <div className="mb-2 flex items-baseline justify-between text-sm">
        <span className="font-semibold">{label}</span>
        <span className="tabular-nums text-muted">
          {known ? `${bytes(done)} / ${bytes(total)} • ${pct.toFixed(1)}%` : "Working…"}
        </span>
      </div>
      <div className="h-2 overflow-hidden rounded-full bg-ink-700">
        {known ? (
          <div className={`h-full rounded-full ${color} transition-[width] duration-500`} style={{ width: `${pct}%` }} />
        ) : (
          <div className={`animate-indeterminate h-full w-2/5 rounded-full ${color}`} />
        )}
      </div>
    </div>
  );
}
