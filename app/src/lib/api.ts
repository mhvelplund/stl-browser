// Types and calls for the Tauri commands and events (see app/src-tauri/src/{browse,commands,indexer}.rs).
import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";

export type View = "models" | "files";

export type TagKind = "user" | "support" | "scale" | "creator" | "edition";

/** `byUser`: set (or confirmed) by the user rather than generated. */
export type Tag = { kind: TagKind; name: string; byUser: boolean };

export type Item = {
  name: string;
  kind: string;
  key: string;
  real: string[];
  badge: string | null;
  fileKind: string | null;
  openable: boolean;
  children: number;
  models: number;
  files: number;
  images: number;
  /** The composite's cells; empty when there is no composite slot. */
  composite: Thumb[];
  pics: Thumb[];
  /** The saved (or default) slot in `[composite?, ...pics]`. */
  slot: number;
  /** The item's own tags (inherited ones count in search only). */
  tags: Tag[];
};

export type Thumb = { entryId: number; key: string };

export type ThumbReady = { entryId: number; url: string | null; pending: number };

export type Listing = {
  view: View;
  path: string[];
  name: string;
  /** The folder's node key: what its saved thumbnail choice is stored under. */
  key: string;
  real: string[];
  models: number;
  files: number;
  images: number;
  items: Item[];
};

export type SearchMode = "free" | "word" | "regexp";

/** A search result: the tile its folder would show, and `parent`, the folder that lists it.
 * `marks` are code-point ranges into `name`. */
export type Hit = Item & {
  parent: string[];
  marks: [number, number][];
};

/** `path` is the scope actually searched (it falls back when a folder vanished). */
export type Found = { path: string[]; hits: Hit[]; truncated: boolean };

export type RootInfo = { id: number; name: string; path: string; missing: boolean };

export type Status = {
  roots: RootInfo[];
  models: number;
  files: number;
  indexing: boolean;
  /** Why `config.yaml` was ignored (the defaults apply), if it was. */
  configWarning: string | null;
};

export type IndexProgress = { index: number; total: number; name: string };

export type IndexDone = {
  ok: boolean;
  error: string | null;
  sources: number;
  listed: number;
  removed: number;
  errors: number;
  locked: number;
  resolved: number;
  models: number;
  warnings: string[];
  seconds: number;
  /** Stopped before every source was looked at. */
  cancelled: boolean;
};

export type ExtractProgress = { index: number; total: number; name: string };

export type ExtractDone = {
  ok: boolean;
  error: string | null;
  files: number;
  dirs: number;
  /** Locked archives left out. */
  skipped: number;
  /** `[rel, error]` per file that could not be written. */
  failed: [string, string][];
  /** Where the first item went. */
  dest: string;
  seconds: number;
  cancelled: boolean;
};

/** What a drag carries; `skipped` counts locked archives and paths too long for Explorer. */
export type DragInfo = { files: number; skipped: number };

export const listFolder = (view: View, path: string[]) => invoke<Listing>("list_folder", { view, path });
/** The folder of view `to` matching the folder at `path` of view `from`. */
export const locate = (from: View, path: string[], to: View) => invoke<string[]>("locate", { from, path, to });
/** Open a file in its Windows app (extracted to a temp folder first if it is in an archive). */
export const openFile = (view: View, path: string[], name: string) => invoke<void>("open_file", { view, path, name });
/** The folders and files below `path` whose names match `query`. */
export const search = (view: View, path: string[], query: string, mode: SearchMode, caseSensitive: boolean) =>
  invoke<Found>("search", { view, path, query, mode, caseSensitive });
export const status = () => invoke<Status>("status");
export const addFolder = (path: string) => invoke<RootInfo>("add_folder", { path });
/** Rescan the folder at `path` (everything at the library root). */
export const rescan = (view: View, path: string[]) => invoke<void>("rescan", { view, path });
export const cancelIndex = () => invoke<void>("cancel_index");
export const cancelExtract = () => invoke<void>("cancel_extract");
export const requestThumbs = (high: number[], low: number[]) =>
  invoke<{ ready: ThumbReady[]; pending: number }>("request_thumbs", { high, low });
export const selectThumb = (nodeKey: string, imageKey: string) => invoke<void>("select_thumb", { nodeKey, imageKey });

export type InheritedTag = { tag: Tag; from: string[] };
/** A node by its name and the path of the folder that lists it. */
export type Owner = { name: string; path: string[] };
/** `owner` is set when the node shares its folder with the node above: tag that one instead. */
export type NodeTags = { own: Tag[]; inherited: InheritedTag[]; hidden: Tag[]; owner: Owner | null };

export const nodeTags = (view: View, path: string[], name: string) => invoke<NodeTags>("node_tags", { view, path, name });
export const addTag = (view: View, path: string[], name: string, kind: TagKind, tag: string) =>
  invoke<void>("add_tag", { view, path, name, kind, tag });
export const removeTag = (view: View, path: string[], name: string, kind: TagKind, tag: string) =>
  invoke<void>("remove_tag", { view, path, name, kind, tag });
export const hideTag = (view: View, path: string[], name: string, kind: TagKind, tag: string) =>
  invoke<void>("hide_tag", { view, path, name, kind, tag });
export const unhideTag = (view: View, path: string[], name: string, kind: TagKind, tag: string) =>
  invoke<void>("unhide_tag", { view, path, name, kind, tag });
export const allTags = () => invoke<Tag[]>("all_tags");

/** An item: `name` in the folder at `path` (`name === ""` is that folder). */
export type Target = { path: string[]; name: string };

/** One target's tags for the multi-item panel; `covered`: not editable, but its owner is a target too. */
export type TargetTags = { own: Tag[]; hidden: Tag[]; editable: boolean; covered: boolean };
export type EditOp = "add" | "remove" | "fill";
/** `changed`: targets written; `skipped`: targets that can't be tagged (not counting covered ones). */
export type EditResult = { changed: number; skipped: number };

export const nodesTags = (view: View, targets: Target[]) => invoke<TargetTags[]>("nodes_tags", { view, targets });
export const editTags = (view: View, targets: Target[], op: EditOp, kind: TagKind, tag: string) =>
  invoke<EditResult>("edit_tags", { view, targets, op, kind, tag });

export const beginDrag = (view: View, targets: Target[]) => invoke<DragInfo>("begin_drag", { view, targets });
export const extractTo = (view: View, targets: Target[], dest: string) => invoke<void>("extract_to", { view, targets, dest });

export const onIndexProgress = (cb: (p: IndexProgress) => void): Promise<UnlistenFn> =>
  listen<IndexProgress>("index-progress", (e) => cb(e.payload));
export const onThumbReady = (cb: (r: ThumbReady) => void): Promise<UnlistenFn> =>
  listen<ThumbReady>("thumb-ready", (e) => cb(e.payload));
export const onExtractProgress = (cb: (p: ExtractProgress) => void): Promise<UnlistenFn> =>
  listen<ExtractProgress>("extract-progress", (e) => cb(e.payload));
export const onExtractDone = (cb: (d: ExtractDone) => void): Promise<UnlistenFn> =>
  listen<ExtractDone>("extract-done", (e) => cb(e.payload));
export const onIndexDone = (cb: (d: IndexDone) => void): Promise<UnlistenFn> =>
  listen<IndexDone>("index-done", (e) => cb(e.payload));
