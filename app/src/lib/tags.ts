import type { Tag, TagKind, TargetTags } from "./api";

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

/** A tag over a selection: `count` of the editable targets have it; `faded` when not all do. */
export type MergedTag = Tag & { count: number; faded: boolean };

/** The union of the editable targets' own tags by (kind, name), sorted by kind then name.
 * `byUser` is true when any target has it user-set. `skipped` counts targets that can't be
 * tagged and aren't covered by their owner. */
export function mergeTags(targets: TargetTags[]): { tags: MergedTag[]; total: number; skipped: number } {
  const editable = targets.filter((t) => t.editable);
  const by = new Map<string, MergedTag>();
  for (const t of editable) {
    const seen = new Set<string>();
    for (const tag of t.own) {
      const key = tag.kind + ":" + tag.name;
      const m = by.get(key) ?? { kind: tag.kind, name: tag.name, byUser: false, count: 0, faded: false };
      m.byUser ||= tag.byUser;
      if (!seen.has(key)) m.count++;
      seen.add(key);
      by.set(key, m);
    }
  }
  const tags = groupByKind([...by.values()]).flatMap(([, ts]) => ts as MergedTag[]);
  for (const m of tags) m.faded = m.count < editable.length;
  return { tags, total: editable.length, skipped: targets.filter((t) => !t.editable && !t.covered).length };
}
