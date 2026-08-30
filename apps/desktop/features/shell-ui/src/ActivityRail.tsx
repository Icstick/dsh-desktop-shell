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
  shortLabel: string;
  enabled: boolean;
}

interface ActivityRailProps {
  active: SurfaceId;
  onSelect(surface: SurfaceId): void;
}

export function ActivityRail({ active, onSelect }: ActivityRailProps) {
  const { lang, setLang, t } = useI18n();

  const items: RailItem[] = [
    { id: "dsh", label: t("rail.dsh"), shortLabel: "DS", enabled: true },
    { id: "browser", label: t("rail.browser"), shortLabel: "BR", enabled: true },
    { id: "terminal", label: t("rail.terminal"), shortLabel: "TM", enabled: true },
    { id: "notifications", label: t("rail.notifications"), shortLabel: "NT", enabled: true },
    { id: "usage", label: t("rail.usage"), shortLabel: "US", enabled: true },
    { id: "timer", label: t("rail.timer"), shortLabel: "TI", enabled: false },
    { id: "runtime", label: t("rail.runtime"), shortLabel: "RT", enabled: true },
    { id: "settings", label: t("rail.settings"), shortLabel: "ST", enabled: true },
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
              <span aria-hidden="true">{item.shortLabel}</span>
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
