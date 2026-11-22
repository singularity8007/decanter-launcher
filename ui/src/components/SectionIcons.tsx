// Decanter's own duotone icons for the settings sections: a line drawing in
// the text colour, with a wine-tinted fill that deepens when the section is
// active.

interface Props {
  active?: boolean;
  size?: number;
}

function Frame({ active, size = 20, children }: Props & { children: React.ReactNode }) {
  return (
    <svg
      width={size}
      height={size}
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth={1.7}
      strokeLinecap="round"
      strokeLinejoin="round"
      aria-hidden="true"
      style={{ ["--tint" as string]: active ? "rgb(194 48 90 / 0.55)" : "rgb(194 48 90 / 0.18)" }}
    >
      {children}
    </svg>
  );
}

/** General: a tuning dial. */
export function DialIcon(p: Props) {
  return (
    <Frame {...p}>
      <circle cx="12" cy="13.5" r="7.5" fill="var(--tint)" />
      <path d="M12 13.5 15.6 9.9" />
      <circle cx="12" cy="13.5" r="1.1" fill="currentColor" stroke="none" />
      <path d="M12 2.5v1.8M4.2 5.6l1.3 1.3M19.8 5.6l-1.3 1.3" />
    </Frame>
  );
}

/** Game launch: a controller with a play mark. */
export function LaunchIcon(p: Props) {
  return (
    <Frame {...p}>
      <path
        d="M7.2 7.5h9.6a4.2 4.2 0 0 1 4.1 5.1l-.9 4.2a2.4 2.4 0 0 1-4.2 1L14 15.6h-4l-1.8 2.2a2.4 2.4 0 0 1-4.2-1l-.9-4.2a4.2 4.2 0 0 1 4.1-5.1Z"
        fill="var(--tint)"
      />
      <path d="M10.6 9.9v3.4l2.9-1.7Z" fill="currentColor" stroke="none" />
    </Frame>
  );
}

/** Proton: a bottle with rising bubbles. */
export function BottleIcon(p: Props) {
  return (
    <Frame {...p}>
      <path d="M8.6 14.2h6.8v5.6a1.4 1.4 0 0 1-1.4 1.4h-4a1.4 1.4 0 0 1-1.4-1.4Z" fill="var(--tint)" stroke="none" />
      <path d="M10.4 2.8h3.2M10.8 2.8v3.9c0 .8-.4 1.4-1 1.9-1 .8-1.6 1.9-1.6 3.2v8a1.4 1.4 0 0 0 1.4 1.4h4.8a1.4 1.4 0 0 0 1.4-1.4v-8c0-1.3-.6-2.4-1.6-3.2-.6-.5-1-1.1-1-1.9V2.8" />
      <circle cx="11" cy="17.4" r=".9" fill="currentColor" stroke="none" />
      <circle cx="13.2" cy="15.6" r=".6" fill="currentColor" stroke="none" />
    </Frame>
  );
}

/** Steam Runtime: the layers every Proton build runs on. */
export function LayersIcon(p: Props) {
  return (
    <Frame {...p}>
      <path d="M12 3.2 20 7.4l-8 4.2-8-4.2Z" fill="var(--tint)" />
      <path d="m4 11.8 8 4.2 8-4.2M4 16.2l8 4.2 8-4.2" />
    </Frame>
  );
}

/** About: the app's own mark, a D whose stem stands apart from its bowl. */
export function DecanterIcon(p: Props) {
  return (
    <Frame {...p}>
      <path d="M5.5 5v14" strokeWidth={2.4} />
      <path d="M10.2 5.2h2.2a6.8 6.8 0 0 1 0 13.6h-2.2" fill="var(--tint)" strokeWidth={2.4} />
    </Frame>
  );
}
