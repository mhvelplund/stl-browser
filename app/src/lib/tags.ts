import type { Tag, TagKind } from "./api";

/** Dot and pill order. */
export const KIND_ORDER: TagKind[] = ["user", "support", "scale", "creator", "edition"];

export const KIND_LABEL: Record<TagKind, string> = {
  user: "Tag",
  support: "Support",
  scale: "Scale",
  creator: "Creator",
  edition: "Edition",
};

/** The distinct kinds among `tags`, in `KIND_ORDER`. */
export function dotKinds(tags: Tag[]): TagKind[] {
  const have = new Set(tags.map((t) => t.kind));
  return KIND_ORDER.filter((k) => have.has(k));
}

/** The tags per kind, in `KIND_ORDER` (kinds without tags left out), names sorted. */
export function groupByKind(tags: Tag[]): [TagKind, Tag[]][] {
  const out: [TagKind, Tag[]][] = [];
  for (const k of KIND_ORDER) {
    const of = tags.filter((t) => t.kind === k).sort((a, b) => (a.name < b.name ? -1 : a.name > b.name ? 1 : 0));
    if (of.length) out.push([k, of]);
  }
  return out;
}

/** What × does on a pill: a user-set tag is removed, a generated one is hidden. */
export function removalAction(tag: Tag): "remove" | "hide" {
  return tag.byUser ? "remove" : "hide";
}

function normalize(s: string): string {
  return s.trim().toLowerCase().replace(/\s+/g, " ");
}

/** The tag names of a search text, like the backend's `parse_query`: `tag:x` or `tag:"x y"` at the start of a term. */
export function parseTagTerms(text: string): string[] {
  const tags: string[] = [];
  let rest = text;
  for (;;) {
    rest = rest.trimStart();
    if (!rest) break;
    if (rest.slice(0, 4).toLowerCase() === "tag:") {
      const after = rest.slice(4);
      let value: string;
      if (after.startsWith('"')) {
        const q = after.slice(1);
        const end = q.indexOf('"');
        value = end < 0 ? q : q.slice(0, end);
        rest = end < 0 ? "" : q.slice(end + 1);
      } else {
        const m = after.search(/\s/);
        value = m < 0 ? after : after.slice(0, m);
        rest = m < 0 ? "" : after.slice(m);
      }
      const n = normalize(value);
      if (n) tags.push(n);
    } else {
      const m = rest.search(/\s/);
      rest = m < 0 ? "" : rest.slice(m);
    }
  }
  return tags;
}
