// Types and calls for the Tauri commands and events (see app/src-tauri/src/{browse,commands,indexer}.rs).
import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";

export type View = "models" | "files";

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

export type RootInfo = { id: number; name: string; path: string; missing: boolean };

export type Status = { roots: RootInfo[]; models: number; files: number; indexing: boolean };

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
export const status = () => invoke<Status>("status");
export const addFolder = (path: string) => invoke<RootInfo>("add_folder", { path });
/** Rescan the folder at `path` (everything at the library root). */
export const rescan = (view: View, path: string[]) => invoke<void>("rescan", { view, path });
export const cancelIndex = () => invoke<void>("cancel_index");
export const cancelExtract = () => invoke<void>("cancel_extract");
export const requestThumbs = (high: number[], low: number[]) =>
  invoke<{ ready: ThumbReady[]; pending: number }>("request_thumbs", { high, low });
export const selectThumb = (nodeKey: string, imageKey: string) => invoke<void>("select_thumb", { nodeKey, imageKey });

export const beginDrag = (view: View, path: string[], names: string[]) => invoke<DragInfo>("begin_drag", { view, path, names });
export const extractTo = (view: View, path: string[], names: string[], dest: string) =>
  invoke<void>("extract_to", { view, path, names, dest });

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
