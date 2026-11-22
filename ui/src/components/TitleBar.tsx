import { getCurrentWindow } from "@tauri-apps/api/window";
import { Minus, Square, X } from "lucide-react";

export default function TitleBar() {
  return (
    <header
      data-tauri-drag-region
      onDoubleClick={() => void getCurrentWindow().toggleMaximize()}
      className="flex h-9 shrink-0 items-center justify-between border-b border-ink-800 bg-ink-950 pl-3"
    >
      <div data-tauri-drag-region className="pointer-events-none flex items-center gap-2">
        <img src="/decanter.svg" alt="" className="h-5 w-5" />
        <span className="text-[13px] font-semibold tracking-wide text-muted">Decanter</span>
      </div>
      <div className="flex h-full">
        <WinButton label="Minimize" onClick={() => void getCurrentWindow().minimize()}>
          <Minus size={15} />
        </WinButton>
        <WinButton label="Maximize" onClick={() => void getCurrentWindow().toggleMaximize()}>
          <Square size={12} />
        </WinButton>
        <WinButton label="Close" danger onClick={() => void getCurrentWindow().close()}>
          <X size={16} />
        </WinButton>
      </div>
    </header>
  );
}

function WinButton(props: { label: string; danger?: boolean; onClick: () => void; children: React.ReactNode }) {
  return (
    <button
      aria-label={props.label}
      title={props.label}
      onClick={props.onClick}
      onDoubleClick={(e) => e.stopPropagation()}
      className={`flex h-full w-11 items-center justify-center text-muted transition-colors hover:text-fg ${
        props.danger ? "hover:bg-wine-600" : "hover:bg-ink-750"
      }`}
    >
      {props.children}
    </button>
  );
}
