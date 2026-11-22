import { Check } from "lucide-react";
import type { GameId, GameView } from "../api";
import GameIcon from "./GameIcon";

/** "Add a game": every game Decanter shows, with its current key art. */
export default function Library({ games, onPick }: { games: GameView[]; onPick: (id: GameId) => void }) {
  const shown = games.filter((g) => g.visible);
  return (
    <div className="h-full overflow-y-auto px-10 py-9">
      <h1 className="text-3xl font-bold tracking-tight">Games</h1>
      <p className="mt-1 text-muted">Pick a game to install it, or open one you already have.</p>
      <div className="mt-8 grid grid-cols-[repeat(auto-fill,minmax(300px,1fr))] gap-5">
        {shown.map((g) => (
          <button
            key={g.id}
            onClick={() => onPick(g.id)}
            className="group animate-fade-in relative aspect-[16/9] overflow-hidden rounded-2xl border border-ink-700 bg-ink-850 text-left transition-all hover:-translate-y-0.5 hover:border-wine-500/60 hover:shadow-xl hover:shadow-wine-900/30"
          >
            {g.art.background && (
              <img
                src={g.art.background}
                alt=""
                draggable={false}
                className="absolute inset-0 h-full w-full object-cover transition-transform duration-500 group-hover:scale-105"
              />
            )}
            <div className="absolute inset-0 bg-gradient-to-t from-ink-950/95 via-ink-950/30 to-transparent" />
            <div className="absolute inset-x-4 bottom-4 flex items-center gap-3">
              <GameIcon game={g} size={42} />
              <div className="min-w-0 flex-1">
                <div className="truncate font-bold">{g.name}</div>
                <div className="text-xs text-muted">{g.latest_version ? `Version ${g.latest_version}` : " "}</div>
              </div>
              {g.install && (
                <span className="flex items-center gap-1 rounded-full bg-ok/15 px-2.5 py-1 text-xs font-semibold text-ok">
                  <Check size={13} /> Installed
                </span>
              )}
            </div>
          </button>
        ))}
      </div>
    </div>
  );
}
