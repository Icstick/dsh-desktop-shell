import type { SurfaceId } from "./ActivityRail";
import studyArtwork from "./assets/deepseek-study.webp";
import observatoryArtwork from "./assets/harness-observatory.webp";
import waterArtwork from "./assets/deepseek-water.webp";

import workshopArtwork from "./assets/deepseek-workshop.webp";
import explorerArtwork from "./assets/deepseek-explorer.webp";
import correspondenceArtwork from "./assets/deepseek-correspondence.webp";

const artwork: Record<SurfaceId, string> = {
  dsh: waterArtwork,
  browser: explorerArtwork,
  terminal: observatoryArtwork,
  runtime: observatoryArtwork,
  settings: workshopArtwork,
  workbench: workshopArtwork,
  notifications: correspondenceArtwork,
  usage: studyArtwork,
};

/** Decorative only: never represents runtime state or overlaps native surfaces. */
export function PageArtwork({ surface }: { surface: SurfaceId }) {
  return <img className="page-artwork" src={artwork[surface]} alt="" aria-hidden="true" draggable={false} />;
}
