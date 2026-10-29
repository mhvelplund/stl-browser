# Usability pass after milestone 6 (before milestone 7)

## Context

The user asked for seven changes after trying milestones 1–6 by hand: cancel buttons for slow tasks, the mouse Back
button, STL renders as folder pictures, rescanning one folder (toolbar and right-click), keeping your place when you
switch views, and opening a file in its Windows app with a double-click. They all touch the same files
(`+page.svelte`, `commands.rs`, the indexer), so they go in as one small milestone ("6.5") on a branch
`usability-pass`. Per CLAUDE.md: write a plan in `docs/superpowers/plans/2026-10-29-usability-pass.md`, then do TDD per
step with a ledger in `.superpowers/sdd/usability-pass/` and one fresh reviewer at the end. Update `docs/index.md`
(status row, decisions, deferred items) as work lands. Commit the pending `docs/index.md` edit first.

## Steps

### 1. Stop buttons (indexing and Extract to…)

- `AppState` (`app/src-tauri/src/state.rs`): add `cancel_index` and `cancel_extract` (`Arc<AtomicBool>`). Clear them
  when a run is claimed (`try_begin_index`, `try_begin_extract`). Add commands `cancel_index` and `cancel_extract` that
  set them.
- `stl-index`: give `scan_root` and `index_all` a `cancel: &AtomicBool` (bundle it with step 4's scope in one
  `ScanRun { scope, cancel }` struct so call sites change once). Check it before each source. On a cancel:
  - stop listing sources;
  - **skip the stale-source deletion** (the `seen` set is incomplete, so it would delete real sources);
  - still resolve the collections already marked dirty, so the DB stays consistent;
  - stop before the next root.
  `ScanStats` and `IndexDone` get `cancelled: bool`. Callers that never cancel (`stlx`, tests) pass
  `&AtomicBool::new(false)`.
- Extract to… already takes a cancel flag: `app/src-tauri/src/export.rs:107` passes `&AtomicBool::new(false)` to
  `write_tree`. Pass the state flag instead. Also stop a single-pass 7z/RAR read early: change `extract_batch`'s
  callback (`crates/stl-index/src/export.rs:121`) so it can stop the pass, if `extract_many_entries` allows it (check
  in `stl-scan`); otherwise note it as deferred. `ExtractDone` gets `cancelled`.
- UI: `StatusBar.svelte` shows a small "Stop" (×) button after each progress bar, with `onStopIndex` and
  `onStopExtract`. `indexrun.ts` `doneHint` and `extractrun.ts` `extractHint` say "Stopped after N of M". Unit tests in
  the `.test.ts` files.
- Known limit: listing one huge archive (Ghamak's RAR) can't be interrupted part-way. The stop takes effect after that
  source. Record this in the deferred list.

### 2. Mouse Back button goes to the parent folder

- `+page.svelte`: add `onmouseup` (and `onauxclick`) on `svelte:window`. For `e.button === 3`, call `preventDefault()`
  and `goUp()`. Also prevent the `mousedown` default for button 3, so WebView2 doesn't do its own history-back. Leave
  button 4 (Forward) alone.
- Update the `usage` hint and the Up button title ("Up (Backspace, mouse Back)").

### 3. STL renders as folder pictures (carousel and right-click)

- `browse.rs` `pics()`: after the nested images, append up to `MAX_MESHES` nested meshes (today meshes are used only
  when there are no images). The default slot stays the hero, or the first image.
- If the node has a saved selection that is not among its pics but is a thumbnail-able leaf below it, put it first.
  This is for a choice made with the right-click entry on a file deeper down than the first six. Pass `selected` into
  `pics` and `shown_pic`; find the key with a DFS under the node, and only when it is missing.
- **A folder's chosen picture stands in for the whole folder in its ancestors.** Today `nested()` (`browse.rs:137`)
  collects pictures from every descendant. Instead, when the DFS reaches a descendant folder that has a saved
  selection which resolves to a picture (an image or mesh leaf below it), it adds just that picture and doesn't go
  into that folder.
  - That picture goes to the image list or the mesh list by its kind.
  - Composite cells already use `shown_pic`, so they stay consistent.
  - A stale selection (the key is no longer below that folder) is ignored, and the DFS goes into the folder as before.
  - A `composite` choice isn't a single picture, so that folder is scanned as before too.
  - Implement this as one DFS that fills both lists and takes `selected`, replacing the two `nested` calls.
- `Listing` gets `key` (the open folder's node key).
- UI: the right-click menu of an image or mesh file tile gets "Use as folder thumbnail". It calls
  `api.selectThumb(listing.key, item.key)` and shows a notice. The folder's own tile is one level up, so it shows there
  on the next listing.
- Rust tests in `browse.rs`: a model with an image now has its mesh in `pics` after the image; a deep selection is put
  first and picked.
  - A collection whose child model has a chosen picture lists only that picture from the model, not its other images.
  - With a stale choice, the collection lists all of that model's images again.

### 4. Rescan the open folder (toolbar) and one folder (right-click)

- **Scope from the view:** add `browse::scope(views, view, path) -> Vec<PathBuf>`. It takes each `real` of the node and
  cuts it at the first `!` (an archive member means "that archive"). It keeps only disk paths and drops paths covered
  by another one. A Models folder merged from several places gives several paths. At the library root the scope is
  `None` (everything).
- **Scoped scan** (`scanstore::scan_root`): still `discover` the whole root (a directory walk; the listing is what's
  slow). Keep only the sources where `src.path` is under a scope path, or a scope path is under `src.path` (a folder
  source covers its collection's loose files). Apply the same rule to the stale-source deletion, so sources outside the
  scope are never removed. In `index_all`, skip roots that don't intersect the scope. The "never resolved" sweep
  stays per root.
- **Command:** `rescan(view, path)` replaces the no-argument `rescan`. A non-empty path computes the scope from the
  cached views; an empty path rescans everything. It reuses `start_index_with`.
- **UI:**
  - The Toolbar button becomes "Rescan folder" at a folder and "Rescan all" at Library. It calls
    `api.rescan(view, nav.path)` with the hint "Rescanning <name>…".
  - The folder-tile context menu gets "Rescan" with `[...nav.path, item.name]`.
  - Both are disabled while busy, as now.
- **Tests:** a `scanstore` test where two collections both change and a scoped scan re-lists only one and deletes
  nothing outside the scope. A `browse::scope` test for a file inside an archive, and for a merged Models folder.

### 5. Keep your place when switching views

- Pure function in `stl-core` (`vtree/locate.rs`):
  `locate(from: &Tree, path: &[String], to: &Tree) -> Vec<String>`.
  - Take the `real` paths of the node at `path`. Split each real path into components on `\`, `/` and `!`, ignoring
    case.
  - In `to`, among non-file nodes, pick the deepest node with a real path equal to, or a component prefix of, one of
    those paths (the folder that contains this spot).
  - If no node contains it, pick the shallowest node below it.
  - Return that node's name path. Build it with a DFS that carries the path, so no parent links are needed.
  - If nothing matches, retry with the ancestors of `from`, one level up at a time, ending at `[]`.
- Command `locate(from: View, path, to: View) -> Vec<String>` in `commands.rs`.
- `switchView` in `+page.svelte` awaits `api.locate(...)`, then calls `load({ path, back: [] })`. It falls back to
  `start()` on an error.
- Tests in `crates/stl-core/tests/` on a small tree pair:
  - a model folder in Models maps to its archive folder in Files, and the reverse;
  - a Files folder with no model maps to its nearest Models ancestor.

### 6. Double-click a file to open it in its Windows app

- Command `open_file(view, path, name)` in `export.rs` (async; refused while indexing because of entry ids, like
  `build_export`).
  - Loose file (`EntryLoc.kind == Folder`): open `source_path\chain[0]` directly.
  - Archive member: write it with `build_export` + `write_tree` into `%TEMP%\stl-browser-open\<n>\<name>`, then open
    it.
  - Open with `tauri_plugin_opener::open_path(path, None::<&str>)`. The plugin is already a dependency and registered
    in `lib.rs:19`.
  - Delete `%TEMP%\stl-browser-open` at app setup, so old extractions don't pile up.
- `Tile.svelte`: `ondblclick` calls `onOpen` for folders and a new `onLaunch` for files. `+page.svelte` sets a notice:
  "Opening <name>…", then the error if there is one.
- The context menu gets "Open" for files too (now folders only).
- Test: `open_file`'s extract half (split out as `stage_for_open(state, view, path, name) -> PathBuf`). A loose file
  returns its real path; an archive member is written to temp with the right bytes (use the zip fixture from
  `testutil`).

## Critical files

- `app/src/routes/+page.svelte`, `app/src/lib/{StatusBar,Toolbar,Tile,TileGrid}.svelte`, `app/src/lib/api.ts`,
  `indexrun.ts`, `extractrun.ts`
- `app/src-tauri/src/{state,commands,indexer,browse,export,lib}.rs`
- `crates/stl-index/src/{scanstore,resolvestore,export}.rs`, `crates/stl-index/src/bin/stlx.rs` (call sites)
- `crates/stl-core/src/vtree/locate.rs` (new) and `vtree/mod.rs`

## Verification

- `cargo test --workspace` (now 242 passed and 2 ignored, plus the new tests), `npm --prefix app test`, and
  `npm --prefix app run check` with 0 errors.
- In `tauri dev` with `STL_BROWSER_HOME` on a scratch index of `D:\STL\Warhammer 40K Collection Vol.1`, driven over CDP
  as in milestone 6:
  - Rescan all, then Stop part-way: the counts don't drop (nothing deleted), and the hint says "Stopped".
  - Extract Ghamak `ancient sarcophagus`, then Stop: the extraction ends quickly with fewer files.
  - Open Ghamak's folder, then "Rescan folder": progress shows only Ghamak's sources.
  - Right-click → Rescan on a collection tile.
  - Switch Models ↔ Files inside Ghamak: you land in the matching folder.
  - A model with images can show an STL render, and "Use as folder thumbnail" sticks after an index run.
- By hand (the user): the mouse Back button goes up; a double-click on an STL inside the RAR opens Bambu Studio, and on
  a JPG opens Photos.
