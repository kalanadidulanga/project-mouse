// Small line icons for the rail (spec 006 FR-024). Inline SVG: no assets, no dependency, and
// they follow the text colour in light, dark and High Contrast.
export type IconName = "home" | "movement" | "behaviour" | "schedules" | "blackouts" | "appearance" | "about";

const PATHS: Record<IconName, string> = {
  home: "M2.5 7.5 8 3l5.5 4.5M4 6.5V13h3V9.5h2V13h3V6.5",
  movement: "M8 1.5v13M1.5 8h13M6 3.5l2-2 2 2M6 12.5l2 2 2-2M3.5 6l-2 2 2 2M12.5 6l2 2-2 2",
  behaviour: "M2 4.5h6M11 4.5h3M9.5 3v3M2 11.5h2M7 11.5h7M5.5 10v3",
  schedules: "M2.5 3.5h11v10h-11zM2.5 6.5h11M5 2v3M11 2v3",
  blackouts: "M12.5 10.5A5 5 0 0 1 5.5 3.5a5 5 0 1 0 7 7z",
  appearance: "M2 3h12v10H2zM2 5.5h12",
  about: "M8 14.5A6.5 6.5 0 1 0 8 1.5a6.5 6.5 0 0 0 0 13zM8 7v4.5M8 4.75v.5",
};

export function Icon({ name }: { name: IconName }) {
  return (
    <svg
      className="icon"
      width="16"
      height="16"
      viewBox="0 0 16 16"
      fill="none"
      stroke="currentColor"
      strokeWidth="1.4"
      strokeLinecap="round"
      strokeLinejoin="round"
      aria-hidden="true"
    >
      <path d={PATHS[name]} />
    </svg>
  );
}
