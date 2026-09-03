import { useI18n, type Lang } from "../../../src/i18n";

export type SurfaceId =
  | "dsh"
  | "browser"
  | "terminal"
  | "runtime"
  | "settings"
  | "notifications"
  | "usage";

interface RailItem {
  id: SurfaceId | "browser" | "terminal" | "usage" | "timer";
  label: string;
  enabled: boolean;
}

interface ActivityRailProps {
  active: SurfaceId;
  onSelect(surface: SurfaceId): void;
}

const ICON_ATTRS = {
  viewBox: "0 0 24 24",
  fill: "none",
  stroke: "currentColor",
  strokeWidth: 1.8,
  strokeLinecap: "round",
  strokeLinejoin: "round",
  "aria-hidden": true,
} as const;

function RailIcon({ id }: { id: RailItem["id"] }) {
  switch (id) {
    case "dsh":
      // Monitor: the DSH Surface is a hosted web UI.
      return (
        <svg {...ICON_ATTRS}>
          <rect x="3" y="4" width="18" height="13" rx="2" />
          <path d="M8 21h8M12 17v4" />
        </svg>
      );
    case "browser":
      return (
        <svg {...ICON_ATTRS}>
          <circle cx="12" cy="12" r="9" />
          <path d="M3 12h18M12 3a15 15 0 0 1 0 18M12 3a15 15 0 0 0 0 18" />
        </svg>
      );
    case "terminal":
      return (
        <svg {...ICON_ATTRS}>
          <rect x="3" y="4" width="18" height="16" rx="2" />
          <path d="m7 9 3 3-3 3" />
          <path d="M12 15h5" />
        </svg>
      );
    case "notifications":
      return (
        <svg {...ICON_ATTRS}>
          <path d="M18 8a6 6 0 0 0-12 0c0 7-3 9-3 9h18s-3-2-3-9" />
          <path d="M13.7 21a2 2 0 0 1-3.4 0" />
        </svg>
      );
    case "usage":
      return (
        <svg {...ICON_ATTRS}>
          <path d="M3 21h18" />
          <path d="M6 21v-6M12 21v-11M18 21v-8" />
        </svg>
      );
    case "timer":
      return (
        <svg {...ICON_ATTRS}>
          <circle cx="12" cy="12" r="9" />
          <path d="M12 7v5l3 2" />
        </svg>
      );
    case "runtime":
      return (
        <svg {...ICON_ATTRS}>
          <path d="M3 12h4l2.5-6 5 12 2.5-6h4" />
        </svg>
      );
    case "settings":
      return (
        <svg {...ICON_ATTRS}>
          <circle cx="12" cy="12" r="3.2" />
          <path d="M12 2.5v3M12 18.5v3M2.5 12h3M18.5 12h3M5 5l2.1 2.1M16.9 16.9 19 19M19 5l-2.1 2.1M7.1 16.9 5 19" />
        </svg>
      );
    default:
      return null;
  }
}

export function ActivityRail({ active, onSelect }: ActivityRailProps) {
  const { lang, setLang, t } = useI18n();

  const items: RailItem[] = [
    { id: "dsh", label: t("rail.dsh"), enabled: true },
    { id: "browser", label: t("rail.browser"), enabled: true },
    { id: "terminal", label: t("rail.terminal"), enabled: true },
    { id: "notifications", label: t("rail.notifications"), enabled: true },
    { id: "usage", label: t("rail.usage"), enabled: true },
    { id: "timer", label: t("rail.timer"), enabled: false },
    { id: "runtime", label: t("rail.runtime"), enabled: true },
    { id: "settings", label: t("rail.settings"), enabled: true },
  ];

  return (
    <nav className="activity-rail" aria-label={t("rail.aria.surfaces")}>
      <div className="activity-rail__brand" aria-label={t("rail.aria.brand")}>
        D
      </div>
      <div className="activity-rail__items">
        {items.map((item) => {
          const enabledItem = item.enabled ? (item.id as SurfaceId) : null;
          return (
            <button
              className="activity-rail__button"
              data-active={enabledItem === active}
              disabled={!enabledItem}
              key={item.id}
              onClick={() => enabledItem && onSelect(enabledItem)}
              title={item.label}
              type="button"
            >
              <RailIcon id={item.id} />
              <span className="sr-only">{item.label}</span>
            </button>
          );
        })}
      </div>
      <div className="activity-rail__lang">
        <select
          aria-label={t("lang.label")}
          className="activity-rail__lang-select"
          onChange={(event) => setLang(event.target.value as Lang)}
          value={lang}
        >
          <option value="zh">中文</option>
          <option value="en">EN</option>
        </select>
      </div>
    </nav>
  );
}
