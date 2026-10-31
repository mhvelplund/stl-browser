// Rubber-band hit testing from layout geometry, so tiles outside the virtualized DOM count too.

/** In content coordinates (scrollTop already added). */
export type Rect = { x0: number; y0: number; x1: number; y1: number };

export function rect(ax: number, ay: number, bx: number, by: number): Rect {
  return { x0: Math.min(ax, bx), y0: Math.min(ay, by), x1: Math.max(ax, bx), y1: Math.max(ay, by) };
}

const overlaps = (a0: number, a1: number, b0: number, b1: number) => a1 > b0 && a0 < b1;

/** Grid tiles: tile i is at col i % cols, row i / cols; left = pad + col*(tile+gap),
 * top = pad + row*rowHeight, size tile × (rowHeight - gap). The indexes of the tiles the rect
 * intersects, ascending. */
export function gridHits(r: Rect, n: number, g: { cols: number; tile: number; gap: number; pad: number; rowHeight: number }): number[] {
  const out: number[] = [];
  const rows = Math.ceil(n / g.cols);
  const first = Math.max(0, Math.floor((r.y0 - g.pad) / g.rowHeight));
  const last = Math.min(rows - 1, Math.floor((r.y1 - g.pad) / g.rowHeight));
  for (let row = first; row <= last; row++) {
    const top = g.pad + row * g.rowHeight;
    if (!overlaps(r.y0, r.y1, top, top + g.rowHeight - g.gap)) continue;
    for (let col = 0; col < g.cols; col++) {
      const i = row * g.cols + col;
      if (i >= n) break;
      const left = g.pad + col * (g.tile + g.gap);
      if (overlaps(r.x0, r.x1, left, left + g.tile)) out.push(i);
    }
  }
  return out;
}

/** List rows of equal `rowHeight` starting at `top`, full width: indexes intersecting [y0, y1]. */
export function listHits(r: Rect, n: number, rowHeight: number, top: number): number[] {
  const out: number[] = [];
  for (let i = Math.max(0, Math.floor((r.y0 - top) / rowHeight)); i < n; i++) {
    const y = top + i * rowHeight;
    if (y >= r.y1) break;
    if (overlaps(r.y0, r.y1, y, y + rowHeight)) out.push(i);
  }
  return out;
}
