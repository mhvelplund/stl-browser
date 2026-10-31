// Tile and folder captions.
import type { Item, Listing, Target, View } from "./api.ts";

const n = (count: number, word: string) => `${count} ${word}${count === 1 ? "" : "s"}`;

function extension(name: string): string | null {
  const dot = name.lastIndexOf(".");
  return dot > 0 ? name.slice(dot + 1).toUpperCase() : null;
}

export function subtitle(item: Item, view: View): string {
  switch (item.kind) {
    case "model":
      return `${n(item.images, "image")} · ${n(item.files, "file")}`;
    case "locked":
      return `password-protected ${item.badge ?? "archive"}`;
    case "file":
    case "image":
      return extension(item.name) ?? item.fileKind ?? item.kind;
  }
  if (view === "models" && item.models > 0) return `${n(item.models, "model")} · ${n(item.images, "image")}`;
  return `${n(item.children, "item")} · ${n(item.files, "file")}`;
}

export function folderSummary(l: Listing, view: View): string {
  if (view === "models") {
    const models = l.models > 0 ? `${n(l.models, "model")} · ` : "";
    return `${models}${n(l.images, "image")} · ${n(l.files, "file")}`;
  }
  return `${n(l.items.length, "item")} · ${n(l.files, "file")}`;
}

/** What a drag or Extract to… of `items` sends: every item, locked ones too (they come back
 * counted as left out), or nothing when all are locked. */
export function exportTargets<T extends { name: string; kind: string }>(items: T[], pathOf: (i: T) => string[]): Target[] {
  if (items.every((i) => i.kind === "locked")) return [];
  return items.map((i) => ({ path: pathOf(i), name: i.name }));
}
