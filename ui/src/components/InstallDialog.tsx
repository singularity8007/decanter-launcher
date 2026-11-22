import { useEffect, useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import { AlertTriangle, FolderOpen, HardDrive } from "lucide-react";
import { api, type GameView, type InstalledRunner } from "../api";
import { bytes } from "../format";
import Modal from "./Modal";

interface Props {
  game: GameView;
  onClose: () => void;
  onInstall: (dir: string, runner: string) => void;
}

export default function InstallDialog({ game, onClose, onInstall }: Props) {
  const [dir, setDir] = useState(game.default_dir);
  const [runner, setRunner] = useState(game.default_runner);
  const [runners, setRunners] = useState<InstalledRunner[]>([]);

  useEffect(() => {
    api.installedRunners().then(setRunners).catch(() => setRunners([]));
  }, []);

  const pick = async () => {
    const chosen = await open({ directory: true, multiple: false, defaultPath: dir });
    if (typeof chosen === "string") setDir(chosen);
  };

  const names = Array.from(new Set([game.default_runner, ...runners.filter((r) => r.runtime).map((r) => r.name)]));

  return (
    <Modal
      title={`Install ${game.name}`}
      subtitle={game.latest_version ? `Version ${game.latest_version}` : undefined}
      onClose={onClose}
      footer={
        <>
          <button className="btn btn-ghost" onClick={onClose}>
            Cancel
          </button>
          <button className="btn btn-primary" disabled={!dir.startsWith("/")} onClick={() => onInstall(dir, runner)}>
            Install
          </button>
        </>
      }
    >
      <div className="space-y-5">
        {(game.download_size || game.install_size) && (
          <div className="flex gap-3">
            <Size label="Download" value={bytes(game.download_size)} />
            <Size label="On disk" value={bytes(game.install_size)} />
          </div>
        )}

        <div>
          <div className="label-caps mb-2">Install folder</div>
          <div className="flex gap-2">
            <input className="field font-mono !text-[13px]" value={dir} onChange={(e) => setDir(e.target.value)} />
            <button className="btn btn-ghost shrink-0" onClick={pick} title="Choose a folder">
              <FolderOpen size={16} />
            </button>
          </div>
          <p className="mt-2 text-xs leading-relaxed text-muted">
            Already have it installed? Pick that folder: Decanter checks the files and downloads only what's missing or
            changed.
          </p>
        </div>

        <div>
          <div className="label-caps mb-2">Proton</div>
          <select className="field" value={runner} onChange={(e) => setRunner(e.target.value)}>
            {names.map((n) => (
              <option key={n} value={n}>
                {n === game.default_runner ? `${n} (recommended)` : n}
              </option>
            ))}
          </select>
          <p className="mt-2 text-xs text-muted">
            Downloaded automatically if it isn't installed yet, along with the Steam Linux Runtime it needs.
          </p>
        </div>

        {game.notes && (
          <div className="flex gap-3 rounded-xl border border-warn/30 bg-warn/10 p-3 text-sm text-warn">
            <AlertTriangle size={18} className="mt-0.5 shrink-0" />
            <span>{game.notes}</span>
          </div>
        )}
      </div>
    </Modal>
  );
}

function Size({ label, value }: { label: string; value: string }) {
  return (
    <div className="card flex flex-1 items-center gap-3 px-4 py-3">
      <HardDrive size={18} className="text-wine-300" />
      <div>
        <div className="label-caps">{label}</div>
        <div className="font-semibold">{value}</div>
      </div>
    </div>
  );
}
