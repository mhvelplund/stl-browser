// Explorer-style multi-select over items with string ids. Every function returns a new value.

export type Selection = { ids: ReadonlySet<string>; anchor: string | null };

export const empty: Selection = { ids: new Set(), anchor: null };

export type Mods = { ctrl: boolean; shift: boolean };
export const NONE: Mods = { ctrl: false, shift: false };
/** The selection modifiers of a mouse event (Cmd counts as Ctrl). */
export const mods = (e: { ctrlKey: boolean; metaKey: boolean; shiftKey: boolean }): Mods => ({ ctrl: e.ctrlKey || e.metaKey, shift: e.shiftKey });

const one = (id: string): Selection => ({ ids: new Set([id]), anchor: id });

/** `order` is the display order of ids. Plain: just `id`. ctrl: toggle `id`. shift: anchor..id
 * (replacing, or added with ctrl). The anchor moves to `id` except on shift. */
export function click(s: Selection, id: string, order: readonly string[], mods: Mods): Selection {
  if (mods.shift && s.anchor !== null) {
    const a = order.indexOf(s.anchor);
    const b = order.indexOf(id);
    if (a >= 0 && b >= 0) {
      const range = order.slice(Math.min(a, b), Math.max(a, b) + 1);
      const ids = new Set(mods.ctrl ? [...s.ids, ...range] : range);
      return { ids, anchor: s.anchor };
    }
  }
  if (mods.ctrl && !mods.shift) {
    const ids = new Set(s.ids);
    if (ids.has(id)) ids.delete(id);
    else ids.add(id);
    return { ids, anchor: id };
  }
  return one(id);
}

/** The band's ids, or `base` ∪ them with ctrl; anchor unchanged. */
export function band(base: Selection, hit: readonly string[], ctrl: boolean): Selection {
  return { ids: new Set(ctrl ? [...base.ids, ...hit] : hit), anchor: base.anchor };
}

export function all(order: readonly string[]): Selection {
  return { ids: new Set(order), anchor: order[0] ?? null };
}

/** Drop ids not in `order`; the anchor too if gone. */
export function prune(s: Selection, order: readonly string[]): Selection {
  const keep = new Set(order);
  return {
    ids: new Set([...s.ids].filter((id) => keep.has(id))),
    anchor: s.anchor !== null && keep.has(s.anchor) ? s.anchor : null,
  };
}

/** Right-click / drag start on `id`: keep `s` if `id` is in it, else just `id`. */
export function focus(s: Selection, id: string): Selection {
  return s.ids.has(id) ? s : one(id);
}
