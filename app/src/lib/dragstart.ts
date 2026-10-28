// When a press on a tile turns into a drag out of the window: the primary button went down on
// it and the pointer then moved THRESHOLD px or more with the button still held.

export type Press = { x: number; y: number; id: number } | null;

export const THRESHOLD = 6;

export function press(e: { button: number; x: number; y: number; pointerId: number }): Press {
  return e.button === 0 ? { x: e.x, y: e.y, id: e.pointerId } : null;
}

/** `buttons` is the PointerEvent bitmask; bit 1 is the primary button. */
export function shouldStart(p: Press, x: number, y: number, buttons: number): boolean {
  if (!p || (buttons & 1) === 0) return false;
  return Math.hypot(x - p.x, y - p.y) >= THRESHOLD;
}
