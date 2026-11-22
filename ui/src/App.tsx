import { useCallback, useEffect, useRef, useState } from "react";
import { api, errorText, type GameId, type GameView, type JobView, type Settings } from "./api";
import TitleBar from "./components/TitleBar";
import Rail, { type View } from "./components/Rail";
import Home from "./components/Home";
import Library from "./components/Library";
import Downloads from "./components/Downloads";
import SettingsPage from "./components/SettingsPage";
import Toast from "./components/Toast";

const IDLE_JOB: JobView = {
  status: { state: "idle" },
  progress: { label: "", phase: "preparing", done: 0, total: 0, install_done: 0, install_total: 0 },
  net_bps: 0,
  disk_bps: 0,
  peak_bps: 0,
  history: [],
  eta_secs: null,
  running_game: null,
};

/** Dev-only: open a given screen straight from the URL (?view=settings). */
function devParam(name: string): string | null {
  return import.meta.env.DEV ? new URLSearchParams(location.search).get(name) : null;
}

export default function App() {
  const [games, setGames] = useState<GameView[]>([]);
  const [selected, setSelected] = useState<GameId | null>(null);
  const [view, setView] = useState<View>(devParam("view") as View | null ?? "home");
  const [job, setJob] = useState<JobView>(IDLE_JOB);
  const [settings, setSettings] = useState<Settings | null>(null);
  const [error, setError] = useState<string | null>(null);
  const wasRunning = useRef(false);

  const reloadGames = useCallback(async () => {
    try {
      setGames(await api.games());
    } catch (e) {
      setError(errorText(e));
    }
  }, []);

  // First load: games, selection, settings; then ask publishers what's new.
  useEffect(() => {
    (async () => {
      try {
        const [list, sel, s] = await Promise.all([api.games(), api.selected(), api.settings()]);
        setGames(list);
        setSettings(s);
        const shown = list.filter((g) => g.visible || g.install);
        const pick = sel ?? shown.find((g) => g.install)?.id ?? shown[0]?.id ?? null;
        setSelected(pick);
        if (!list.some((g) => g.install) && !devParam("view")) setView("library");
        // Refresh in the background, one game at a time; the screen updates as each lands.
        for (const g of shown) {
          try {
            await api.refresh(g.id);
            setGames(await api.games());
          } catch {
            // Offline or a publisher hiccup: the cached art/version stays.
          }
        }
      } catch (e) {
        setError(errorText(e));
      }
    })();
  }, []);

  // Poll the running job. Fast while something runs, slow otherwise.
  useEffect(() => {
    let alive = true;
    let timer: number | undefined;
    const tick = async () => {
      try {
        const j = await api.job();
        if (!alive) return;
        setJob(j);
        const running = j.status.state === "running";
        if (wasRunning.current && !running) void reloadGames();
        wasRunning.current = running;
        timer = window.setTimeout(tick, running ? 500 : 1500);
      } catch {
        timer = window.setTimeout(tick, 2000);
      }
    };
    void tick();
    return () => {
      alive = false;
      window.clearTimeout(timer);
    };
  }, [reloadGames]);

  const selectGame = useCallback(async (id: GameId) => {
    setSelected(id);
    setView("home");
    try {
      await api.select(id);
    } catch (e) {
      setError(errorText(e));
    }
  }, []);

  /** Run an action, surfacing a refusal (e.g. "another task is running"). */
  const act = useCallback(
    async (fn: () => Promise<unknown>) => {
      try {
        await fn();
        setJob(await api.job());
        await reloadGames();
      } catch (e) {
        setError(errorText(e));
      }
    },
    [reloadGames],
  );

  const game = games.find((g) => g.id === selected) ?? null;
  const busy = job.status.state === "running";

  return (
    <div className="flex h-full flex-col bg-ink-950">
      <TitleBar />
      <div className="flex min-h-0 flex-1">
        <Rail
          games={games}
          selected={selected}
          view={view}
          busy={busy}
          onSelect={selectGame}
          onView={setView}
        />
        <main className="relative min-w-0 flex-1 overflow-hidden">
          {view === "home" && game && (
            <Home
              key={game.id}
              game={game}
              job={job}
              settings={settings}
              act={act}
              onShowDownloads={() => setView("downloads")}
              onOpenProtonSettings={() => setView("settings")}
            />
          )}
          {view === "home" && !game && <Library games={games} onPick={selectGame} />}
          {view === "library" && <Library games={games} onPick={selectGame} />}
          {view === "downloads" && <Downloads job={job} games={games} act={act} />}
          {view === "settings" && settings && (
            <SettingsPage
              settings={settings}
              job={job}
              act={act}
              onSettings={async (s) => {
                setSettings(s);
                try {
                  await api.setSettings(s);
                } catch (e) {
                  setError(errorText(e));
                }
              }}
            />
          )}
        </main>
      </div>
      <Toast job={job} error={error} onClearError={() => setError(null)} onDismissJob={() => act(api.dismiss)} />
    </div>
  );
}
