import { useCallback, useEffect, useState } from "react";
import { ArrowUpCircle, Check, Download, Loader2, RefreshCw, Trash2 } from "lucide-react";
import { api, DEFAULT_RUNNER, errorText, type Family, type FamilyView, type InstalledRunner, type JobView, type Release } from "../api";
import { bytes } from "../format";

const RUNTIME_LABEL = { sniper: "SLR 3", steamrt4: "SLR 4" } as const;
/** Builds the game profiles default to. */
const RECOMMENDED: Record<string, string> = {
  [DEFAULT_RUNNER]: "Recommended for ZZZ, HSR, Genshin, WuWa",
  "proton-twintail-11.0-20261016-slr": "Recommended for Endfield",
};
const GAME_SHORT: Record<string, string> = { zzz: "ZZZ", hsr: "HSR", genshin: "Genshin", wuwa: "WuWa", endfield: "Endfield" };

interface Props {
  job: JobView;
  act: (fn: () => Promise<unknown>) => Promise<void>;
}

export default function ProtonManager({ job, act }: Props) {
  const [families, setFamilies] = useState<FamilyView[]>([]);
  const [family, setFamily] = useState<Family>("ge");
  const [installed, setInstalled] = useState<InstalledRunner[]>([]);
  const [releases, setReleases] = useState<Release[] | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [loading, setLoading] = useState(false);
  const running = job.status.state === "running";
  const installingName = job.status.state === "running" && job.status.kind === "runner" ? job.status.title : null;

  const loadReleases = useCallback(async (f: Family, force: boolean) => {
    setLoading(true);
    setError(null);
    try {
      setReleases(await api.releases(f, force));
    } catch (e) {
      setError(errorText(e));
      setReleases([]);
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    api.families().then(setFamilies).catch(() => setFamilies([]));
  }, []);

  // Reload the lists when a task finishes (an install or removal changes them).
  useEffect(() => {
    api.installedRunners().then(setInstalled).catch(() => setInstalled([]));
    if (!running) void loadReleases(family, false);
  }, [family, running, loadReleases]);

  const remove = async (name: string) => {
    try {
      await api.removeRunner(name);
      setInstalled(await api.installedRunners());
      setReleases(await api.releases(family, false));
    } catch (e) {
      setError(errorText(e));
    }
  };

  return (
    <div className="space-y-8">
      <section>
        <div className="label-caps mb-3">Installed</div>
        {installed.length === 0 && <div className="card px-5 py-4 text-sm text-muted">No Proton builds yet. Install one below.</div>}
        <div className="space-y-2">
          {installed.map((r) => (
            <div key={r.name} className="card flex items-center gap-4 px-5 py-3.5">
              <div className="min-w-0 flex-1">
                <div className="flex flex-wrap items-center gap-2">
                  <span className="font-semibold">{r.name}</span>
                  {r.runtime && <Tag>{RUNTIME_LABEL[r.runtime]}</Tag>}
                  {r.used_by.map((g) => (
                    <Tag key={g} tone="wine">
                      {GAME_SHORT[g] ?? g}
                    </Tag>
                  ))}
                </div>
                {r.problem && <div className="mt-1 text-xs text-warn">{r.problem}</div>}
                {r.newer && (
                  <div className="mt-1 flex items-center gap-1.5 text-xs text-wine-300">
                    <ArrowUpCircle size={13} /> {r.newer} is available
                  </div>
                )}
              </div>
              {r.newer && r.family && (
                <button className="btn btn-ghost shrink-0" disabled={running} onClick={() => act(() => api.installRunner(r.family!, r.newer!))}>
                  <Download size={15} /> Install {r.newer}
                </button>
              )}
              <button
                className="btn btn-danger shrink-0 !px-2.5"
                title={r.used_by.length ? "In use by a game" : "Remove"}
                aria-label={`Remove ${r.name}`}
                disabled={running || r.used_by.length > 0}
                onClick={() => void remove(r.name)}
              >
                <Trash2 size={15} />
              </button>
            </div>
          ))}
        </div>
      </section>

      <section>
        <div className="mb-3 flex items-center justify-between">
          <div className="label-caps">Available</div>
          <button className="btn btn-ghost !py-1.5 !text-xs" disabled={loading} onClick={() => void loadReleases(family, true)}>
            <RefreshCw size={13} className={loading ? "animate-spin" : ""} /> Refresh
          </button>
        </div>
        <div className="mb-4 flex flex-wrap gap-2">
          {families.map((f) => (
            <button
              key={f.id}
              onClick={() => setFamily(f.id)}
              className={`rounded-full px-4 py-1.5 text-sm font-semibold transition-colors ${
                family === f.id ? "bg-wine-600 text-white" : "bg-ink-800 text-muted hover:text-fg"
              }`}
            >
              {f.label}
            </button>
          ))}
        </div>
        {error && <div className="mb-3 rounded-xl border border-wine-500/40 bg-wine-900/40 p-3 text-sm text-wine-200">{error}</div>}
        {releases === null ? (
          <div className="flex items-center gap-2 text-sm text-muted">
            <Loader2 size={16} className="animate-spin" /> Loading releases…
          </div>
        ) : (
          <div className="space-y-2">
            {releases.slice(0, 30).map((r) => {
              const busyHere = installingName?.includes(r.name);
              const verifiable = r.sha256 !== null || r.checksum_url !== null;
              return (
                <div key={r.name} className="card flex items-center gap-4 px-5 py-3">
                  <div className="min-w-0 flex-1">
                    <div className="flex flex-wrap items-center gap-2">
                      <span className="font-medium">{r.name}</span>
                      {RECOMMENDED[r.name] && <Tag tone="wine">{RECOMMENDED[r.name]}</Tag>}
                      {r.prerelease && <Tag tone="warn">Pre-release</Tag>}
                    </div>
                    <div className="mt-0.5 text-xs text-faint">
                      {r.published_at.slice(0, 10)} • {bytes(r.size)}
                    </div>
                  </div>
                  {r.installed ? (
                    <span className="flex items-center gap-1 text-sm font-semibold text-ok">
                      <Check size={15} /> Installed
                    </span>
                  ) : (
                    <button
                      className="btn btn-ghost shrink-0"
                      disabled={running || !verifiable}
                      title={verifiable ? undefined : "No published checksum, so it can't be verified"}
                      onClick={() => act(() => api.installRunner(r.family, r.name))}
                    >
                      {busyHere ? <Loader2 size={15} className="animate-spin" /> : <Download size={15} />} Install
                    </button>
                  )}
                </div>
              );
            })}
          </div>
        )}
      </section>
    </div>
  );
}

function Tag({ children, tone }: { children: React.ReactNode; tone?: "wine" | "warn" }) {
  const cls =
    tone === "wine" ? "bg-wine-500/20 text-wine-200" : tone === "warn" ? "bg-warn/15 text-warn" : "bg-ink-700 text-muted";
  return <span className={`rounded-md px-2 py-0.5 text-[11px] font-semibold ${cls}`}>{children}</span>;
}
