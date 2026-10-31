// The rubber-band gesture on a scrolling container: press on empty space, drag to draw a box,
// auto-scroll near the top and bottom edges. Hit testing is left to `hits` (see band.ts).
import { THRESHOLD } from "./dragstart";
import { rect, type Rect } from "./band";

export type BandOpts = {
  /** Whether a press on `target` may start a band (not on a tile or a row). */
  startsOn: (target: Element) => boolean;
  /** The ids inside `r` (content coordinates of the container). */
  hits: (r: Rect) => string[];
  /** The band's hits on every move; `fresh` on a band's first call. */
  onBand: (ids: string[], ctrl: boolean, fresh: boolean) => void;
  /** A press and release on empty space without a band. */
  onClear: () => void;
  /** The box to draw, `null` when the band ends. */
  onRect: (r: Rect | null) => void;
};

const EDGE = 32;
const STEP = 16;

export function bandGesture(el: () => HTMLElement | undefined, o: BandOpts) {
  let start: { x: number; y: number; ctrl: boolean } | null = null;
  let drawn = false;
  let pointer = { x: 0, y: 0 };
  let raf = 0;

  const at = (box: HTMLElement, cx: number, cy: number) => {
    const b = box.getBoundingClientRect();
    return { x: cx - b.left + box.scrollLeft, y: cy - b.top + box.scrollTop };
  };

  function update() {
    const box = el();
    if (!start || !box) return;
    const p = at(box, pointer.x, pointer.y);
    if (!drawn && Math.hypot(p.x - start.x, p.y - start.y) < THRESHOLD) return;
    const fresh = !drawn;
    drawn = true;
    const r = rect(start.x, start.y, p.x, p.y);
    o.onRect(r);
    o.onBand(o.hits(r), start.ctrl, fresh);
    cancelAnimationFrame(raf);
    const b = box.getBoundingClientRect();
    const dy = pointer.y < b.top + EDGE ? -STEP : pointer.y > b.bottom - EDGE ? STEP : 0;
    if (dy) {
      raf = requestAnimationFrame(() => {
        box.scrollTop += dy;
        update();
      });
    }
  }

  function end() {
    cancelAnimationFrame(raf);
    start = null;
    drawn = false;
    o.onRect(null);
  }

  return {
    down(e: PointerEvent) {
      const box = el();
      if (!box || e.button !== 0 || !(e.target instanceof Element) || !o.startsOn(e.target)) return;
      if (e.clientX - box.getBoundingClientRect().left >= box.clientWidth) return; // the scrollbar
      const p = at(box, e.clientX, e.clientY);
      start = { ...p, ctrl: e.ctrlKey || e.metaKey };
      drawn = false;
      pointer = { x: e.clientX, y: e.clientY };
      box.setPointerCapture(e.pointerId);
    },
    move(e: PointerEvent) {
      if (!start) return;
      pointer = { x: e.clientX, y: e.clientY };
      update();
    },
    up() {
      if (!start) return;
      const clear = !drawn && !start.ctrl;
      end();
      if (clear) o.onClear();
    },
    cancel: end,
    /** The container scrolled (wheel or auto-scroll): the band follows. */
    scrolled() {
      if (start && drawn) update();
    },
  };
}
