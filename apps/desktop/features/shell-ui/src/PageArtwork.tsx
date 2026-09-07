import type { SurfaceId } from "./ActivityRail";
import studyArtwork from "./assets/deepseek-study.png";
import observatoryArtwork from "./assets/harness-observatory.png";
import waterArtwork from "./assets/deepseek-water.png";

import workshopArtwork from "./assets/deepseek-workshop.png";
import explorerArtwork from "./assets/deepseek-explorer.png";
import correspondenceArtwork from "./assets/deepseek-correspondence.png";

const artwork: Record<SurfaceId, string> = {
  dsh: waterArtwork,
  browser: explorerArtwork,
  terminal: observatoryArtwork,
  runtime: observatoryArtwork,
  settings: workshopArtwork,
  notifications: correspondenceArtwork,
  usage: studyArtwork,
};

/** Decorative only: never represents runtime state or overlaps native surfaces. */
export function PageArtwork({ surface }: { surface: SurfaceId }) {
  return <img className="page-artwork" src={artwork[surface]} alt="" aria-hidden="true" draggable={false} />;
}
