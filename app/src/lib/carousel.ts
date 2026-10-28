// A tile's carousel: slot 0 is the composite when the item has one, then its pictures.
import type { Item, Thumb } from "./api.ts";

export type Slot = { kind: "composite"; cells: Thumb[] } | { kind: "pic"; thumb: Thumb };

export function slots(item: Item): Slot[] {
  const pics: Slot[] = item.pics.map((thumb) => ({ kind: "pic", thumb }));
  return item.composite.length ? [{ kind: "composite", cells: item.composite }, ...pics] : pics;
}

/** The next slot in `dir`, wrapping around. */
export const step = (i: number, n: number, dir: 1 | -1): number => (n ? (i + dir + n) % n : 0);

/** What `select_thumb` stores for a slot. */
export const slotKey = (s: Slot): string => (s.kind === "composite" ? "composite" : s.thumb.key);

/** The entry ids a slot needs on screen. */
export const shown = (s: Slot): number[] => (s.kind === "composite" ? s.cells.map((c) => c.entryId) : [s.thumb.entryId]);

/** The slot an item shows: the one picked this session, else the saved or default one. */
export function current(item: Item, chosen: Map<string, number>): number {
  const n = slots(item).length;
  const c = chosen.get(item.key);
  return c !== undefined && c < n ? c : item.slot;
}

/**
 * Thumbnails to ask for: `high` = what the items in `[first, last)` show right now, `low` =
 * every other picture of every item (so flipping a carousel is instant). No id twice.
 */
export function requests(
  items: Item[],
  chosen: Map<string, number>,
  visible: { first: number; last: number },
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
    const s = slots(items[i])[current(items[i], chosen)];
    if (s) take(shown(s), high);
  }
  for (const item of items) for (const s of slots(item)) take(shown(s), low);
  return { high, low };
}
