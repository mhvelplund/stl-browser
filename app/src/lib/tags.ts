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
