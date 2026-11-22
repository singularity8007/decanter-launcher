import { useId } from "react";

/** Network speed over the last minute as a smooth area chart. */
export default function SpeedGraph({ samples, height = 120 }: { samples: number[]; height?: number }) {
  // useId() contains ':' (or '«»'), which breaks url(#...) references.
  const gradient = `speed-${useId().replace(/[^a-zA-Z0-9_-]/g, "")}`;
  const width = 600;
  const slots = 120; // matches the backend's 60 s history at 2 samples/s
  const data = samples.slice(-slots);
  const max = Math.max(1, ...data) * 1.15;
  const step = width / (slots - 1);
  const offset = (slots - data.length) * step; // new samples enter from the right

  const points = data.map((v, i) => [offset + i * step, height - (v / max) * height] as const);
  const line = points.map(([x, y], i) => `${i === 0 ? "M" : "L"}${x.toFixed(1)},${y.toFixed(1)}`).join(" ");
  const area = points.length > 1 ? `${line} L${points[points.length - 1]![0].toFixed(1)},${height} L${points[0]![0].toFixed(1)},${height} Z` : "";

  return (
    <svg viewBox={`0 0 ${width} ${height}`} preserveAspectRatio="none" className="block w-full" style={{ height }}>
      <defs>
        <linearGradient id={gradient} x1="0" y1="0" x2="0" y2="1">
          <stop offset="0" stopColor="var(--color-wine-400)" stopOpacity="0.45" />
          <stop offset="1" stopColor="var(--color-wine-400)" stopOpacity="0" />
        </linearGradient>
      </defs>
      {[0.25, 0.5, 0.75].map((f) => (
        <line key={f} x1="0" x2={width} y1={height * f} y2={height * f} stroke="var(--color-ink-700)" strokeDasharray="4 6" vectorEffect="non-scaling-stroke" />
      ))}
      {area && <path d={area} fill={`url(#${gradient})`} />}
      {points.length > 1 && (
        <path d={line} fill="none" stroke="var(--color-wine-400)" strokeWidth="2" strokeLinejoin="round" vectorEffect="non-scaling-stroke" />
      )}
    </svg>
  );
}
