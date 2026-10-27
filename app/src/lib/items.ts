// Tile and folder captions.
import type { Item, Listing, View } from "./api.ts";

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
  if (view === "models") return `${n(l.models, "model")} · ${n(l.images, "image")} · ${n(l.files, "file")}`;
  return `${n(l.items.length, "item")} · ${n(l.files, "file")}`;
}
