import { AlertTriangle, CheckCircle2, X } from "lucide-react";
import type { JobView } from "../api";

interface Props {
  job: JobView;
  error: string | null;
  onClearError: () => void;
  onDismissJob: () => void;
}

/** Bottom-centre messages: a refused action, or how the last task ended. */
export default function Toast({ job, error, onClearError, onDismissJob }: Props) {
  const s = job.status;
  if (error) {
    return <Shell tone="bad" title={error} onClose={onClearError} />;
  }
  if (s.state === "failed") {
    return <Shell tone="bad" title={s.message} details={s.details} onClose={onDismissJob} />;
  }
  if (s.state === "done") {
    return <Shell tone="good" title={s.message} onClose={onDismissJob} />;
  }
  return null;
}

function Shell(props: { tone: "good" | "bad"; title: string; details?: string; onClose: () => void }) {
  const bad = props.tone === "bad";
  return (
    <div className="pointer-events-none fixed inset-x-0 top-14 z-50 flex justify-center px-6">
      <div
        role={bad ? "alert" : "status"}
        className={`animate-fade-in pointer-events-auto flex max-w-xl items-start gap-3 rounded-2xl border px-5 py-4 shadow-2xl backdrop-blur-xl ${
          bad ? "border-wine-500/40 bg-wine-900/90" : "border-ok/30 bg-ink-850/95"
        }`}
      >
        {bad ? (
          <AlertTriangle size={20} className="mt-0.5 shrink-0 text-wine-300" />
        ) : (
          <CheckCircle2 size={20} className="mt-0.5 shrink-0 text-ok" />
        )}
        <div className="min-w-0">
          <div className="selectable whitespace-pre-line text-sm font-semibold">{props.title}</div>
          {props.details && <div className="selectable mt-1 break-words font-mono text-xs text-white/60">{props.details}</div>}
        </div>
        <button aria-label="Dismiss" onClick={props.onClose} className="shrink-0 rounded-lg p-1 text-white/60 hover:bg-white/10 hover:text-white">
          <X size={16} />
        </button>
      </div>
    </div>
  );
}
