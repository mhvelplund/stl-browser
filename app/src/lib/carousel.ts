// A tile's carousel: slot 0 is the composite when the item has one, then its pictures. A file
// with pictures of its own (a 3MF's plates and photos) shows its still, then those (`sub`).
import type { Item, Thumb } from "./api.ts";

export type Slot =
  | { kind: "composite"; cells: Thumb[] }
  | { kind: "pic"; thumb: Thumb }
  /** Picture `n` (from 1) of the file `thumb` after its still. */
  | { kind: "sub"; thumb: Thumb; n: number };

/** A file's extra picture urls by entry id, as thumbnails arrive. */
export type Extras = ReadonlyMap<number, string[]>;

const NONE: Extras = new Map();

export function slots(item: Item, extras: Extras = NONE): Slot[] {
  const pics: Slot[] = item.pics.map((thumb) => ({ kind: "pic", thumb }));
  if (item.composite.length) return [{ kind: "composite", cells: item.composite }, ...pics];
  const own = item.fileKind !== null && item.pics.length === 1 ? (extras.get(item.pics[0].entryId) ?? []) : [];
  return [...pics, ...own.map((_, i): Slot => ({ kind: "sub", thumb: item.pics[0], n: i + 1 }))];
}

/** The next slot in `dir`, wrapping around. */
export const step = (i: number, n: number, dir: 1 | -1): number => (n ? (i + dir + n) % n : 0);

/** What `select_thumb` stores for a slot. */
export const slotKey = (s: Slot): string =>
  s.kind === "composite" ? "composite" : s.kind === "sub" ? `${s.thumb.key}#${s.n}` : s.thumb.key;

/** The entry ids a slot needs on screen (a file's own pictures come with its thumbnail). */
export const shown = (s: Slot): number[] => (s.kind === "composite" ? s.cells.map((c) => c.entryId) : [s.thumb.entryId]);

/**
 * The slot an item shows: the one picked this session, else the saved or default one. A saved
 * picture of a file's own that hasn't arrived yet shows slot 0 meanwhile.
 */
export function current(item: Item, chosen: Map<string, number>, extras: Extras = NONE): number {
  const n = slots(item, extras).length;
  const c = chosen.get(item.key);
  if (c !== undefined && c < n) return c;
  return item.slot < n ? item.slot : 0;
}

/**
 * Thumbnails to ask for: `high` = what the items in `[first, last)` show right now, `low` =
 * every other picture of every item (so flipping a carousel is instant). No id twice.
 */
export function requests(
  items: Item[],
  chosen: Map<string, number>,
  visible: { first: number; last: number },
  extras: Extras = NONE,
): { high: number[]; low: number[] } {
  const seen = new Set<number>();
  const take = (ids: number[], out: number[]) => {
    for (const id of ids) {
      if (!seen.has(id)) {
        seen.add(id);
        out.push(id);
      }
    }
  };
  const high: number[] = [];
  const low: number[] = [];
  for (let i = visible.first; i < Math.min(visible.last, items.length); i++) {
    const s = slots(items[i], extras)[current(items[i], chosen, extras)];
    if (s) take(shown(s), high);
  }
  for (const item of items) for (const s of slots(item, extras)) take(shown(s), low);
  return { high, low };
}
