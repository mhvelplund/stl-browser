// Pure helpers for the search result list.

/** Split `name` into highlighted and plain runs; `marks` are code-point ranges `[start, end)`. */
export function runs(name: string, marks: [number, number][]): { text: string; hit: boolean }[] {
  const chars = Array.from(name);
  const out: { text: string; hit: boolean }[] = [];
  let at = 0;
  const push = (from: number, to: number, hit: boolean) => {
    if (to > from) out.push({ text: chars.slice(from, to).join(""), hit });
  };
  for (const [start, end] of marks) {
    push(at, start, false);
    push(start, end, true);
    at = end;
  }
  push(at, chars.length, false);
  return out;
}

/** Where a hit lives, relative to the folder the search started in. */
export const where = (parent: string[], scope: string[]): string => parent.slice(scope.length).join(" › ");

/** `where` cut to the last two folders (a tile caption has room for little more). */
export function whereShort(parent: string[], scope: string[]): string {
  const rel = parent.slice(scope.length);
  if (!rel.length) return "here";
  return (rel.length > 2 ? "… › " : "") + rel.slice(-2).join(" › ");
}

export const scopeLabel = (scope: string[]): string => (scope.length ? `in ${scope.join(" › ")}` : "in Library");

/** A hit's identity: its place in the tree (keys repeat, e.g. a volume's empty key or a shared picture). */
export const hitId = (h: { name: string; parent: string[] }): string => JSON.stringify([...h.parent, h.name]);
