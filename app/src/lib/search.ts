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

export const scopeLabel = (scope: string[]): string => (scope.length ? `in ${scope.join(" › ")}` : "in Library");
