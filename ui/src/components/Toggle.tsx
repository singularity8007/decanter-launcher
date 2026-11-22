interface Props {
  title: string;
  description?: string;
  checked: boolean;
  onChange: (v: boolean) => void;
}

/** A settings row with a switch on the right. */
export default function Toggle({ title, description, checked, onChange }: Props) {
  return (
    <label className="card flex cursor-pointer items-center justify-between gap-6 px-5 py-4 transition-colors hover:border-ink-600">
      <div>
        <div className="font-semibold">{title}</div>
        {description && <div className="mt-0.5 text-sm text-muted">{description}</div>}
      </div>
      <button
        type="button"
        role="switch"
        aria-checked={checked}
        onClick={() => onChange(!checked)}
        className={`relative h-6 w-11 shrink-0 rounded-full transition-colors ${checked ? "bg-wine-500" : "bg-ink-600"}`}
      >
        <span
          className={`absolute top-0.5 h-5 w-5 rounded-full bg-white shadow transition-all ${checked ? "left-[22px]" : "left-0.5"}`}
        />
      </button>
    </label>
  );
}
