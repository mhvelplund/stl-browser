// Layout math for the virtualized tile grid.

/** How many tiles of at least `minTile` px fit across `width`; at least one. */
export function columns(width: number, minTile = 200, gap = 8): number {
  return Math.max(1, Math.floor((width + gap) / (minTile + gap)));
}

export function tileWidth(width: number, cols: number, gap = 8): number {
  return (width - (cols - 1) * gap) / cols;
}

/** Rows to render for a scroll position: `[first, last)`, with `overscan` extra rows each side. */
export function visibleRows(
  scrollTop: number,
  viewport: number,
  rowHeight: number,
  rows: number,
  overscan = 2,
): { first: number; last: number } {
  const clamp = (n: number) => Math.min(rows, Math.max(0, n));
  const top = Math.min(scrollTop, Math.max(0, rows * rowHeight - viewport));
  return {
    first: clamp(Math.floor(top / rowHeight) - overscan),
    last: clamp(Math.ceil((top + viewport) / rowHeight) + overscan),
  };
}
