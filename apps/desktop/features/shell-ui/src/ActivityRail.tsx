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
  shortLabel: string;
  enabled: boolean;
}

const items: RailItem[] = [
  { id: "dsh", label: "DSH", shortLabel: "DS", enabled: true },
  { id: "browser", label: "Browser", shortLabel: "BR", enabled: true },
  { id: "terminal", label: "Terminal", shortLabel: "TM", enabled: true },
  { id: "notifications", label: "Notifications", shortLabel: "NT", enabled: true },
  { id: "usage", label: "Usage", shortLabel: "US", enabled: true },
  { id: "timer", label: "Timer（M3）", shortLabel: "TI", enabled: false },
  { id: "runtime", label: "Runtime", shortLabel: "RT", enabled: true },
  { id: "settings", label: "Settings", shortLabel: "ST", enabled: true },
];

interface ActivityRailProps {
  active: SurfaceId;
  onSelect(surface: SurfaceId): void;
}

export function ActivityRail({ active, onSelect }: ActivityRailProps) {
  return (
    <nav className="activity-rail" aria-label="Desktop surfaces">
      <div className="activity-rail__brand" aria-label="DSH Desktop Shell">
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
              <span aria-hidden="true">{item.shortLabel}</span>
              <span className="sr-only">{item.label}</span>
            </button>
          );
        })}
      </div>
    </nav>
  );
}