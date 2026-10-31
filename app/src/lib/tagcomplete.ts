// Pure helpers for completing `tag:` terms in the search bar.
import type { Tag } from "./api";

/** The `tag:` term under the caret: its span in the query and the (normalised) value typed before the caret. */
export type TagTerm = { start: number; end: number; prefix: string };

const isSpace = (c: string) => /\s/.test(c);

/** Like the Rust `normalize`: lowercase, whitespace runs collapsed, trimmed. */
const normalize = (s: string) => s.toLowerCase().split(/\s+/).filter(Boolean).join(" ");

/** A prefix keeps a trailing space ("big " should not match "bigger"). */
const normalizePrefix = (s: string) => s.toLowerCase().replace(/\s+/g, " ").trimStart();

function nextSpace(q: string, from: number): number {
  let i = from;
  while (i < q.length && !isSpace(q[i])) i++;
  return i;
}

/** The `tag:` term the caret is in, split the way `parse_query` splits it; `null` without one or with nothing typed. */
export function tagTermAt(query: string, caret: number): TagTerm | null {
  let i = 0;
  while (i < query.length) {
    while (i < query.length && isSpace(query[i])) i++;
    if (i >= query.length) break;
    const start = i;
    if (query.slice(i, i + 4).toLowerCase() !== "tag:") {
      i = nextSpace(query, i);
      continue;
    }
    const after = i + 4;
    let valueStart: number, valueEnd: number, end: number;
    if (query[after] === '"') {
      const close = query.indexOf('"', after + 1);
      valueStart = after + 1;
      valueEnd = close < 0 ? query.length : close;
      end = close < 0 ? query.length : close + 1;
    } else {
      valueStart = after;
      end = valueEnd = nextSpace(query, after);
    }
    if (caret >= valueStart && caret <= valueEnd) {
      const prefix = normalizePrefix(query.slice(valueStart, caret));
      return prefix ? { start, end, prefix } : null;
    }
    i = end;
  }
  return null;
}

/** The distinct tag names (any kind) starting with `prefix`, sorted. */
export function matchTags(tags: Tag[], prefix: string): string[] {
  const p = normalizePrefix(prefix);
  const names = new Set<string>();
  for (const t of tags) {
    const n = normalize(t.name);
    if (n && n.startsWith(p)) names.add(n);
  }
  return [...names].sort();
}

/** Replace `term` with `tag:name` (quoted when it has a space), followed by one space; the caret goes after it. */
export function applyCompletion(query: string, term: TagTerm, name: string): { query: string; caret: number } {
  const text = /\s/.test(name) ? `tag:"${name}"` : `tag:${name}`;
  const rest = query.slice(term.end);
  const space = rest !== "" && isSpace(rest[0]) ? "" : " ";
  return { query: query.slice(0, term.start) + text + space + rest, caret: term.start + text.length + 1 };
}
