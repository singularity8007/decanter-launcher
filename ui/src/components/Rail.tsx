import { ArrowDownToLine, Plus, Settings } from "lucide-react";
import type { GameId, GameView } from "../api";
import GameIcon from "./GameIcon";

export type View = "home" | "library" | "downloads" | "settings";

interface Props {
  games: GameView[];
  selected: GameId | null;
  view: View;
  busy: boolean;
  onSelect: (id: GameId) => void;
  onView: (v: View) => void;
}

export default function Rail({ games, selected, view, busy, onSelect, onView }: Props) {
  const installed = games.filter((g) => g.install);
  return (
    <nav className="flex w-[68px] shrink-0 flex-col items-center gap-2 border-r border-ink-800 bg-ink-900 py-3">
      <RailButton label="Add a game" active={view === "library"} onClick={() => onView("library")}>
        <Plus size={22} />
      </RailButton>
      <div className="my-1 h-px w-8 bg-ink-700" />
      <div className="flex flex-1 flex-col items-center gap-2 overflow-y-auto overflow-x-hidden px-1.5 pt-1">
        {installed.map((g) => {
          const active = view === "home" && g.id === selected;
          return (
            <button
              key={g.id}
              title={g.name}
              onClick={() => onSelect(g.id)}
              className="group relative rounded-xl"
            >
              <span
                className={`absolute -left-3 top-1/2 h-6 w-1 -translate-y-1/2 rounded-r bg-wine-400 transition-all ${
                  active ? "opacity-100" : "scale-y-0 opacity-0 group-hover:scale-y-50 group-hover:opacity-60"
                }`}
              />
              <GameIcon
                game={g}
                size={44}
                className={`transition-all ${active ? "ring-2 ring-wine-500" : "opacity-80 group-hover:opacity-100"}`}
              />
              {g.status === "update_available" && (
                <span className="absolute -right-0.5 -top-0.5 h-3 w-3 rounded-full border-2 border-ink-900 bg-wine-400" />
              )}
            </button>
          );
        })}
      </div>
      <RailButton label="Downloads" active={view === "downloads"} onClick={() => onView("downloads")}>
        <ArrowDownToLine size={21} />
        {busy && <span className="absolute right-1.5 top-1.5 h-2.5 w-2.5 animate-pulse rounded-full bg-wine-400" />}
      </RailButton>
      <RailButton label="Settings" active={view === "settings"} onClick={() => onView("settings")}>
        <Settings size={21} />
      </RailButton>
    </nav>
  );
}

function RailButton(props: { label: string; active: boolean; onClick: () => void; children: React.ReactNode }) {
  return (
    <button
      title={props.label}
      aria-label={props.label}
      onClick={props.onClick}
      className={`relative flex h-11 w-11 items-center justify-center rounded-xl transition-colors ${
        props.active ? "bg-wine-700/40 text-wine-300" : "text-muted hover:bg-ink-750 hover:text-fg"
      }`}
    >
      {props.children}
    </button>
  );
}
