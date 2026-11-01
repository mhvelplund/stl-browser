// Turntable on hover: a mesh's thumbnail has a strip of frames next to it in the cache
// (`<content>.spin.jpg`, SPIN_FRAMES squares side by side) that plays while the pointer rests.
import type { Slot } from "./carousel.ts";

/** Frames in a strip (`stl_thumb::SPIN_FRAMES`). */
export const SPIN_FRAMES = 12;

/** How long the pointer rests on a tile before it starts turning. */
export const SPIN_DELAY_MS = 250;

const MESH = /\.(stl|obj)$/i;

/** The strip's URL for a slot showing the still at `url`, or null when it has none. */
export function spinUrl(slot: Slot | null, url: string | null | undefined): string | null {
  if (slot?.kind !== "pic" || !url || !MESH.test(slot.thumb.key) || !url.endsWith(".jpg")) return null;
  return url.slice(0, -".jpg".length) + ".spin.jpg";
}
