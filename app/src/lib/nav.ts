// Navigation history: the open folder's path plus a Back stack that remembers scroll positions.

export type Nav = { path: string[]; back: { path: string[]; scroll: number }[] };

export const start = (): Nav => ({ path: [], back: [] });

/** Go to `path`, remembering where we were and how far it was scrolled. */
export function goTo(nav: Nav, path: string[], scroll: number): Nav {
  return { path, back: [...nav.back, { path: nav.path, scroll }] };
}

export const open = (nav: Nav, name: string, scroll: number): Nav => goTo(nav, [...nav.path, name], scroll);

export const up = (nav: Nav, scroll: number): Nav => (nav.path.length ? goTo(nav, nav.path.slice(0, -1), scroll) : nav);

export function goBack(nav: Nav): { nav: Nav; scroll: number } | null {
  const last = nav.back.at(-1);
  if (!last) return null;
  return { nav: { path: last.path, back: nav.back.slice(0, -1) }, scroll: last.scroll };
}

/** Replace the path with the one the server actually listed (it falls back when a folder vanished). */
export const settle = (nav: Nav, actual: string[]): Nav => ({ ...nav, path: actual });

export function crumbs(path: string[]): { name: string; path: string[] }[] {
  return [{ name: "Library", path: [] }, ...path.map((name, i) => ({ name, path: path.slice(0, i + 1) }))];
}
