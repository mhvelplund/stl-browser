# STL Browser — Rust/Tauri app (design + implementation plan)

## Context

The Python prototype in `prototype/` (`stlindex/`: scan → resolve → SQLite → vtree) proved that messy STL collections
(RAR/ZIP/7Z, nested archives, loose folders) can be mapped to a browsable tree of models with preview images. It
needs 7-Zip and has no UI. We now build the real app: a standalone Rust + Tauri desktop app that indexes folders,
caches the index and thumbnails in `~/.stl-browser/`, and lets you browse it like Windows Explorer with very large
icons. **The prototype is reference only.** No code, Python, or 7-Zip dependency ships in the app. Its docs
(`archive-patterns-plan.md`, `folder-structure.md`, `HANDOFF.md`) and `tests/` are the spec for the ported heuristics.

The first implementation step writes this design to `docs/superpowers/specs/2026-10-27-stl-browser-design.md` and
commits it. Then `superpowers:writing-plans` turns each milestone below into a task plan.

## Decisions

**You decided:**
- Rust + Tauri 2. Frontend in **Svelte 5 + TypeScript** (Vite).
- Archives are read with **native crates**: `zip`, `sevenz-rust2`, `unrar` (bindings to the official unrar lib). No
  external tools.
- **Two switchable views:** *Models* (the resolver tree from `folder-structure.md`) and *Files* (the real disk +
  archive layout, where archives and nested archives open like folders).
- Each added folder becomes a top-level **volume**. Its subfolders are collections, as in the prototype.
- Drag-out uses **Windows virtual-file drag** (delayed rendering): files are extracted only when Explorer asks for
  them.
- Index, thumbnails and user config (selected thumbnails; later tags) live in `~/.stl-browser/`.
- *(Added after milestone 2; details in `docs/index.md` and `docs/virtual-tree.md`.)*
  - **Password-protected archives are not supported.** Don't scan them; show them as one item with a box-with-padlock
    icon.
  - **All file and folder names in the virtual file system are lowercase.** Overlapping folders merge their contents.
    Colliding files get a numbered postfix (`ORC.STL` and `Orc.stl` become `orc_1.stl` and `orc_2.stl`).
  - **Hovering over any element shows its real path**, with `!` marking each step into an archive
    (`folder/example.zip!orcs/mech.zip!leg.stl`).

**My calls (correct me if they're wrong):**
- Thumbnail content hash: archive entries use the CRC32 + size stored in the archive header (no extraction needed to
  check the cache). Loose files use a blake3 hash of their bytes. Cache folder = first 16 hex chars of
  blake3(`source path` + `\0` + chain joined). Files are `.jpg` (quality 80) within 256×256, aspect kept *(changed after
  milestone 5 from 128×128 WebP padded transparent; JPEG has no alpha, so mesh renders sit on the tile grey)*.
- STL/OBJ thumbnails come from a CPU software rasterizer: z-buffer, fixed 3/4 isometric camera fit to the bounding
  box, flat grey lambert shading. No GPU, so it runs headless on worker threads.
- User data is keyed by stable strings, never by row ids (ids change on every re-resolve). Models view:
  `m:<collection>/<path>`. Files view: `f:<source path>|<inner path>`.
- Max archive nesting depth: 3 (same as the prototype). Nested archives ≤ 256 MB are opened in memory. Larger ones
  are spilled to a temp file. RAR always needs a file on disk.

## Architecture

```
stl-browser/
  Cargo.toml                      workspace
  crates/
    stl-core/      pure, no I/O: names.rs, resolve.rs, vtree.rs, rawtree.rs   (port of names/resolve/vtree.py)
    stl-scan/      discover.rs, archive.rs (Zip/SevenZ/Rar behind one trait), nested.rs, extract.rs
    stl-index/     db.rs (rusqlite, schema + migrations), indexer.rs (add root / rescan / re-resolve)
    stl-thumb/     image.rs (image crate → 256px jpg), mesh.rs (stl_io/tobj load), raster.rs, cache.rs, queue.rs
    stl-drag/      Windows-only COM IDataObject + IStream + IDataObjectAsyncCapability (windows crate)
  app/
    src-tauri/     commands.rs, events.rs, state.rs, thumb:// URI protocol
    src/           Svelte UI
  prototype/       unchanged, not built or shipped
```

### Data (`~/.stl-browser/index.db`, WAL)

- **Cached listings** (as in prototype `db.py`): `roots(id, path, name, added_at)`, `sources`, `entries` (`chain`
  JSON, `size`, `crc`). Adds `root_id` on `sources`.
- **Resolved, rebuilt per collection:** `groups`, `models`, `model_files`, `assets`. Same columns as prototype schema
  v3, so `folder-structure.md` still holds.
- **User data, never dropped:** `settings(key, value)`, `selected_thumb(node_key PK, image_key)` (the image key is a
  stable entry key, or `composite`), and for later `tags(id, name)` / `node_tags(node_key, tag_id)`.
- **Thumbnail bookkeeping:** `thumbs(entry_id, cache_rel, status, error)`. It can always be rebuilt from the cache
  folder layout.

### Indexing

- **Add folder** (Tauri dialog): insert a root, run `discover` (port of `scan.discover`: skip dot-dirs, multi-part
  RAR handling), list each source natively with nested recursion, then resolve each collection and store it.
- **Rescan all:** for every root, rediscover. Sources with the same size + mtime keep their cached listing. Changed
  ones are re-listed. Missing sources are deleted (entries cascade). Collections that changed are re-resolved.
  Orphaned `thumbs` rows are dropped. A root folder that no longer exists is flagged in the UI, not silently deleted.
- Runs on a background task and sends `index-progress` events (source n/N, current name). The UI stays usable.
- `stl-core::resolve` is a faithful port of `resolve.py` + `names.py`. All 47 prototype tests are ported as Rust
  unit tests, with the same inputs and the same expected outputs.

### Browsing

- `list_folder(view, path) -> Vec<Item>`. Item: `name, kind (folder|model|image|mesh|other), key, image_keys[]`
  (all nested images, rank order), `selected`, plus the four child-folder keys for the composite.
- **Models view** = port of `vtree.build` (Images/Docs/variant/part levels, name-clash suffixes, ordering).
  **Files view** = a tree built from `entries.path` per root (`x.zip/` shown as folder `x`).
- Both trees are built in memory per root at startup and after indexing. The full tree builds in ~0.4 s in Python,
  so it will be cheap in Rust.
- Folder contents shown: subfolders, supported images (jpg/png/webp/gif/bmp) and object files (stl/obj/3mf/lys…).
  Other files are hidden.

### Thumbnails & carousel

- Opening a folder enqueues thumbnail jobs for every image and STL/OBJ nested under each visible item. Visible items
  get high priority, the rest of the folder low priority. Jobs are **grouped by source archive**, so solid RAR/7Z is
  decompressed in one pass per open, not once per image.
- A worker pool (`rayon`, ~cores-1) runs the jobs. Each finished job sends a `thumb-ready(key)` event and the tile
  updates in place. A missing cache file is just regenerated.
- Served to the webview via a custom `thumb://<cache_rel>` protocol.
- **Folder tile carousel:** slot 0 is a composite (a CSS 2×2 grid of four child folders' *selected* thumbnails; never
  stored as a file). Slots 1..n are the nested images. Small ‹ › arrows on hover. The chosen slot is saved to
  `selected_thumb`. The default is the resolver hero image if there is one, else the composite.
- Model tiles use the same carousel. Image and STL tiles show their own thumbnail.

### Drag-out (Windows virtual files)

- JS `pointerdown` + move threshold on a tile calls `begin_drag(keys)`. Rust runs `DoDragDrop` on the main
  (OLE STA) thread through `run_on_main_thread`. The webview's own HTML5 drag is disabled.
- The `IDataObject` offers `CFSTR_FILEDESCRIPTORW` (one descriptor per file and per directory, with relative paths
  that mirror the *current view's* virtual tree) and `CFSTR_FILECONTENTS` (an `IStream` per index). Each stream is
  extracted lazily. A per-drag extraction session caches opened outer and nested archives, so files from the same
  archive don't reopen it.
- It implements `IDataObjectAsyncCapability`, so Explorer copies on its own background thread with its own progress
  dialog and doesn't freeze.
- **Fallback:** a right-click **Extract to…** that writes the same tree to a chosen folder. It shares the same
  extraction session code, works on any OS, and is a safety net if the COM path misbehaves.
- **Risk:** WebView2 and OLE drag interplay. Milestone 5 starts with a throwaway spike that proves a 2-file virtual
  drag from Tauri before building the full version.

### UI

Toolbar: Back / Up / breadcrumb · Models | Files toggle · **Add folder** · **Rescan all** (with a progress bar).
Main area: a virtualized grid of large tiles (~200px, 256px thumbnail art, name below, item count).
Double-click opens. Status bar: index stats and the thumbnail queue length. Light/dark follows the OS.

## Milestones

1. **Skeleton:** workspace, Tauri app, `~/.stl-browser/` creation, DB schema + migrations, settings.
2. **Scan:** native archive trait (zip/7z/rar), nested listing, `extract_entry(chain)`, discover. Tests use small
   generated fixture archives (including zip-in-7z-in-rar) under `crates/stl-scan/tests/fixtures/`.
3. **Resolve:** port `names.rs` / `resolve.rs` / `vtree.rs` plus the 47 ported tests. Index Vol.1 + Vol.2 and compare
   against `prototype/stl-index.db`.
4. **Browse UI:** add folder, rescan, both views, grid, breadcrumbs, progress events.
5. **Thumbnails:** cache, queue, image + STL/OBJ rasterizer, carousel, composite, persisted selection.
6. **Drag-out:** spike → `stl-drag` → Extract to… fallback.
7. *(Later, not in this plan)* tags, search and filters. Only the schema is reserved now.

## Verification

- `cargo test --workspace`: ported resolver tests, archive fixture tests (nested chain extraction is byte-identical),
  rasterizer smoke test (a cube STL gives a non-empty, centred image), thumb cache path tests.
- **Parity check:** index both real volumes in the app. A small Rust test binary compares per-collection model counts
  and "without own image" with `prototype/stl-index.db` (target: 447 / 685 models, within a few for documented
  heuristic edge cases), plus the spot checks listed in `archive-patterns-plan.md` (Asgardian Trikers, VOIDWALKERS,
  Primarchs, TYRANIDS poster…).
- **Rescan:** rename one archive and delete another in a scratch copy, rescan, and check that the stale entries are
  gone and the renamed one is re-indexed.
- **Manual in the app** (`cargo tauri dev`): open Vol.2 Necrons STC (no images) and check STL thumbnails appear. Cycle
  a carousel, restart, and the selection persists. Delete the cache folder and thumbnails regenerate. Drag a model
  folder to Explorer and the tree on disk matches the view, including a file from a nested archive. Extract to… gives
  the same result.
