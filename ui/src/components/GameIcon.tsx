import { useState } from "react";
import type { GameView } from "../api";
import { initials } from "../format";

/** The publisher's icon, or a wine-tinted monogram when there is none. */
export default function GameIcon({ game, size = 44, className = "" }: { game: GameView; size?: number; className?: string }) {
  const [broken, setBroken] = useState(false);
  const style = { width: size, height: size };
  if (game.art.icon && !broken) {
    return (
      <img
        src={game.art.icon}
        alt=""
        style={style}
        draggable={false}
        onError={() => setBroken(true)}
        className={`rounded-xl object-cover ${className}`}
      />
    );
  }
  return (
    <div
      style={style}
      className={`flex items-center justify-center rounded-xl bg-gradient-to-br from-wine-600 to-wine-900 text-sm font-bold text-wine-200 ${className}`}
    >
      <span style={{ fontSize: size * 0.32 }}>{initials(game.name)}</span>
    </div>
  );
}
