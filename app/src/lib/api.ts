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
};

export type Listing = {
  view: View;
  path: string[];
  name: string;
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
};

export const listFolder = (view: View, path: string[]) => invoke<Listing>("list_folder", { view, path });
export const status = () => invoke<Status>("status");
export const addFolder = (path: string) => invoke<RootInfo>("add_folder", { path });
export const rescan = () => invoke<void>("rescan");

export const onIndexProgress = (cb: (p: IndexProgress) => void): Promise<UnlistenFn> =>
  listen<IndexProgress>("index-progress", (e) => cb(e.payload));
export const onIndexDone = (cb: (d: IndexDone) => void): Promise<UnlistenFn> =>
  listen<IndexDone>("index-done", (e) => cb(e.payload));
