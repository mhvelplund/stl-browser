# STL Browser — design overview

A Windows desktop app (Rust + Tauri 2, Svelte 5 UI) for browsing 3D-print collections: folders of STL/OBJ/LYS files and
images, often inside RAR/ZIP/7Z archives nested inside other archives. You add folders to an index and browse them like
Windows Explorer with very large icons. Each folder shows a carousel of the pictures inside it, and you can drag
anything out to Explorer to extract it.

- Full design: [`superpowers/specs/2026-10-27-stl-browser-design.md`]
- Implementation plans: [`superpowers/plans/2026-10-27-skeleton-and-scan.md`] (milestones 1–2),
  [`superpowers/plans/2026-10-27-resolve.md`] (milestone 3), `superpowers/plans/2026-10-27-browse-ui.md`
  (milestone 4; plans are git-ignored)
- Layout patterns, the model resolver, and the parity baseline: [`archive-patterns.md`]
- The virtual folder tree, the two views, and the naming rules: [`virtual-tree.md`]
- UI mockup (clickable): <https://claude.ai/artifact/Vum7b4yY5TjCBW1Csez2BH>
- The Python prototype was the reference for the resolver port. Its tests live on as Rust tests in `crates/stl-core`.
  After parity held, it was deleted in milestone 3, along with its DB. It is still in git history before that commit.

[`superpowers/specs/2026-10-27-stl-browser-design.md`]: superpowers/specs/2026-10-27-stl-browser-design.md
[`superpowers/plans/2026-10-27-skeleton-and-scan.md`]: superpowers/plans/2026-10-27-skeleton-and-scan.md
[`superpowers/plans/2026-10-27-resolve.md`]: superpowers/plans/2026-10-27-resolve.md
[`archive-patterns.md`]: archive-patterns.md
[`virtual-tree.md`]: virtual-tree.md

## Status

| Milestone                                                                                    | State                                         |
|----------------------------------------------------------------------------------------------|-----------------------------------------------|
| 1. Skeleton: workspace, Tauri app, `~/.stl-browser/index.db`, schema + migrations, settings  | done                                          |
| 2. Scan: native ZIP/7Z/RAR, nested archives, extract by chain, discovery, incremental rescan | done                                          |
| 3. Resolve: port of the prototype's model resolver and virtual tree                          | done                                          |
| 4. Browse UI: add folder, rescan, Models/Files views, grid, breadcrumbs                      | done                                          |
| 5. Thumbnails: cache, queue, image + STL/OBJ renderer, carousel                              | next                                          |
| 6. Drag-out: Windows virtual-file drag, plus "Extract to…"                                   | planned                                       |
| 7. Tags, search, filters                                                                     | later (only the database tables exist so far) |

Milestones 1–2 were checked against the real `D:\Models\Sci-Fi Collection Vol.1`. The scan found 45 sources and
18,050 files (1,243 of them in nested archives) with 0 errors, the same as the prototype. It takes about 35 s, and a
rescan of unchanged folders takes 0 s.

Milestone 3 was checked against Vol.1 + Vol.2 together with `stlx parity` against the prototype's DB:

- 346 sources, 48,942 files and 1,132 models (447 / 685).
- Every per-collection count is equal to the prototype's: models, groups, "without own image" (70 / 135), "no image or
  fallback" (11 / 117), unattached images (277 / 69), and assets by role.
- All 14 spot checks pass.
- Timings:
  - The first index takes 234 s, almost all of it listing archives.
  - Re-indexing unchanged folders takes 0.1 s and resolves nothing.
  - Re-resolving all 11 collections takes 0.7 s.
  - Each view builds in under 0.5 s.

Milestone 4 was smoke-tested in `tauri dev` on Vol.1, driving the WebView over CDP: navigation in both views, a
RAR opened as a folder with correct hover paths, Rescan with progress (unchanged: 0.9 s), Back/Up, the virtualized grid
and the light scheme. The native Add folder dialog and padlock tiles still need a manual check.

## Architecture

```text
Cargo.toml                 workspace
crates/stl-core/           pure logic, no I/O: file kinds and name heuristics (names.rs), the model resolver
                           (resolve/), the virtual tree and both views (vtree/)
crates/stl-scan/           archive readers (ZIP, 7Z, RAR behind one trait), nested listing, extract by chain, discovery
crates/stl-index/          app dir, SQLite schema + migrations, incremental scans (scanstore.rs), per-collection resolve
                           (resolvestore.rs), views from the DB (views.rs), prototype parity (parity.rs), `stlx` dev CLI
app/src-tauri/             Tauri shell: state (DB + cached views + busy flag), commands, browse.rs (listings),
                           indexer.rs (background index runs and their events)
app/src/                   SvelteKit UI (static adapter): routes/+page.svelte plus components and pure logic
                           modules (grid, nav, items) in lib/, tested with `node --test`
```

Planned crates: `stl-thumb` (thumbnails and the STL renderer) and `stl-drag` (Windows OLE drag-out).

### Data on disk: `~/.stl-browser/`

- `index.db` (SQLite, WAL), at schema version 4:
  - **Cached listings:** `roots` (the added folders), `sources` (each archive, or each folder of loose files), `entries`
    (every file found, with `chain`, the archive members to open in order to reach it).
  - **Resolved tables:** `groups`, `models`, `model_files`, `assets`. They are rebuilt per (root, collection) whenever
    one of that collection's sources changes. `resolved` marks the collections that are up to date.
  - **User data, never rebuilt:** `settings`, `selected_thumb`, `tags`, `node_tags`. These are keyed by stable path
    strings, never by row ids.
  - **Later:** `thumbs`.
- `thumbs/<hash of location>/<hash of content>.webp`: 128×128 thumbnails that can be regenerated (milestone 5).
- Tests and `stlx` can point everything elsewhere with `STL_BROWSER_HOME`.

### Scanning

- **Add folder** registers a root, and the root becomes a top-level *volume*. Its subfolders are *collections*.
- **Discovery** walks the root:
  - It skips dot-folders, junk files (`desktop.ini`…) and RAR continuation volumes (`.part2.rar`…).
  - It emits one source per archive, plus one per top-level folder that has loose files.
  - Unreadable folders are skipped with a warning.
- **Listing** reads archives natively (`zip`, `sevenz-rust2`, `unrar`), with no 7-Zip install needed:
  - Nested archives are opened up to 3 levels deep.
  - Nested archives up to 256 MiB stay in memory. Bigger ones, and every archive nested in a RAR, go to a temp file.
- **Rescan** works per source:
  - Same size, mtime and (for folders) file-name fingerprint means the cached listing is kept.
  - Otherwise the source is re-listed. Vanished sources are deleted.
  - A source that failed last time is always retried.
  - A missing root folder is reported and its data is left alone.
- **Password-protected archives** are detected from their headers, with nothing decrypted. Such an archive is stored
  with `locked = 1` and no entries, never opened, and not retried until it changes. A nested one is listed as an entry
  with `locked = 1`, and it isn't opened either. The UI will show both with a box-with-padlock icon.
- **Robustness:** a corrupt archive records an error on its source and the scan continues. A crash (panic) inside an
  archive library is caught and recorded the same way. No format can ever stop to ask for a password.
- **Overlapping roots:** a root inside another root is never scanned on its own, so each source is stored once, under
  the outer root. Adding the same folder twice (in a different case, or with a trailing `\`) returns the existing root.
- **Resolving** (`stlx index` = scan + resolve):
  - After the scan, every collection with a source listed or removed is resolved again, plus any collection never
    resolved.
  - Resolved rows of collections with no sources left are dropped.
  - The resolver is a faithful port of the prototype, with all 47 prototype tests ported. See
    [`archive-patterns.md`].

### Views

Both views are built in memory from the DB (`views.rs` loads the rows, `stl-core::vtree` builds the tree). See
[`virtual-tree.md`] for the layout and the naming rules. `stlx tree models|files [<path>] [--depth N] [--real]`
prints either view.

### The app (milestone 4)

- **Commands:** `status` (roots with a `missing` flag, model and file counts, busy flag), `list_folder(view, path)`,
  `add_folder(path)` (the folder picker is the dialog plugin, called from the UI) and `rescan`.
- **Index runs** happen on a background thread with their own SQLite connection, so browsing keeps working. They send
  `index-progress` (`index`/`total`/`name` per source) and one `index-done` (stats, warnings or error). Only one run at a
  time: a second request is refused. A panic in a run is caught and reported as an error.
- **The tree cache.** Both trees and their per-node counts are built once (on first use) and kept in `AppState`. The
  index worker rebuilds them after each run and swaps them in before it sends `index-done`.
- **`list_folder`** takes the path as name segments (a name may contain `/`), and lists the deepest folder that still
  exists. The returned `path` says where it ended up, so the UI follows when a folder vanished in a rescan.
- **UI:** toolbar (Back, Up, breadcrumbs, Models | Files, Rescan all, Add folder), a title row with counts, a virtualized
  grid of tiles (min 200 px, square art, name, caption) and a status bar (stats, missing folders, the selected item's
  real path, index progress). Placeholder art per kind until thumbnails; a box-with-padlock for locked archives.
  Double-click or Enter opens; Backspace / Alt+Up goes up; Alt+Left goes back (Back restores the scroll position).

## Decisions

### Made by you

- **Rust + Tauri 2** with **Svelte 5 + TypeScript**.
- **Native archive crates, no 7-Zip.**
- **Two switchable views:**
  - *Models*: the resolver's cleaned-up tree (volume → collection → group → model → variant → parts).
  - *Files*: the real disk and archive layout, where archives open like folders.
- **An added folder becomes a top-level volume.**
- **Drag-out uses Windows virtual-file drag**, so files are extracted only when Explorer asks for them. "Extract to…" is
  a fallback.
- **Folder carousel.** Slot 0 is a 2×2 composite of four child folders' thumbnails, and then come the nested images. The
  chosen slot is saved in the DB.
- **Thumbnails are generated when a folder is first opened**, cached as `<hash of path>/<hash of file>`, and regenerated
  if missing. STL files get rendered thumbnails.
- **Tags, search and filtering come later.** User data lives in the SQLite DB in `~/.stl-browser/`.
- **Password-protected archives are not supported** *(added after milestone 2; the scanner side is done)*:
  - Don't try to scan them.
  - Show them as a single item with a **box-with-padlock** icon.
- **All file and folder names are lowercase in the virtual file system** *(added after milestone 2)*:
  - Folders that overlap merge their contents. This includes names differing only in case, like `D:\Models` and `d:\models\`,
    and an added folder that sits inside another added folder.
  - Files whose names collide get a numbered postfix: `ORC.STL` and `Orc.stl` become `orc_1.stl` and `orc_2.stl`.
- **Hovering over any element shows its real path** *(added after milestone 2)*. Each step into an archive is marked
  with `!`, e.g. `folder/example.zip!orcs/mech.zip!leg.stl`.

### Made by me (correct any that are wrong)

- **Thumbnail content hash.** Archive entries use the CRC32 + size stored in the archive, so the cache can be checked
  without extracting. Loose files use blake3 of their bytes. Thumbnails are `.webp`, 128×128.
- **STL/OBJ thumbnails** come from a CPU software renderer: z-buffer, 3/4 isometric view, grey shading. No GPU needed.
- **User data keys.** In the Models view: `m:<collection>/<path>`. In the Files view: `f:<source path>|<inner path>`.
  Both are built from real paths, never row ids, so they survive re-indexing.
- **Nesting limits.** Archives are opened at most 3 levels deep, and nested archives over 256 MiB spill to disk.
- **The SvelteKit template** (the official Tauri Svelte template), so the UI lives in `app/src/routes/`.
- **The Tauri dialog plugin**, for the startup error dialog and later the folder picker.
- **A file-name fingerprint for folder sources** (schema v2), so a renamed loose file is noticed on rescan.
- **`locked` flags on sources and entries** (schema v3) mark password-protected archives.
- **Panics are caught.** The release build uses `panic = "unwind"`, so a crash inside an archive library becomes an
  error on that archive instead of closing the app.
- *Added in milestone 4:*
  - **Trees are cached in `AppState`** and rebuilt by the index worker after every run (about 0.8 s for both views).
  - **The Files view hides other files.** It lists folders, images and object files (`model`, `scene`). The Models
    view shows everything the resolver put in the tree, so its `docs` folders keep their documents.
  - **No Extract to… or thumbnail UI yet**, so no buttons that do nothing.
  - **Offline fonts:** IBM Plex if installed, else Segoe UI; nothing is fetched at runtime.
  - **UI logic tests** use Node's built-in runner on the TypeScript files (no test framework dependency).
- *Added in milestone 3:*
  - **Resolution unit: (root, collection).** Two different roots can't mix their files in one resolve. Two roots with
    the same volume name still show one shared collection folder in the Models view, where clashing models get
    `_N`.
  - **Merge or number, by node kind:**
    - Folders merge only when they are really the same folder: case variants, or an added folder inside another one.
      Model variant and part folders also merge by name, as in the prototype.
    - Every other clash gets `_1`/`_2`, for example `X.zip`, `x.rar` and a folder `x/` side by side.
    - Groups and models are never merged. Models first get the edition or raw name in brackets, then `_N`.
  - **A root inside another root** gets its own volume, which shows the content of its folder in the outer volume. In
    the Models view, if the resolver has collapsed that folder, the volume lists the topmost items below it instead.
  - **Hover for synthetic folders.**
    - A model's hover is the common folder of its files. Pictures attached from outside it are left out, otherwise
      every model with a sidecar picture would hover as its whole collection.
    - Groups, variants, parts, `images` and `docs` hover as the common folder of their children.
  - **Password-protected archives in the Models view** are padlock items directly in their collection folder.
  - **Node keys:**
    - Collections, groups and models use `m:<collection>/<path>`.
    - Variants use `<model key>#<variant>` and parts `<parent key>/<part>`.
    - Leaves use the `f:<source path>|<entry path>` of their file. Disk folders use `f:<folder path>|`.
  - **A drive root** (`D:\`) gets the volume name `D:`.
  - **The `images` folder** of a model appears only if the model has a picture. The prototype also made an empty one for
    models with only documents.

### Details of the new rules (confirmed)

- **Password-protected archives:**
  - An archive counts as password-protected if any of its members is encrypted, or its headers are. It is then stored
    with no entries and a `locked` flag, and it is never retried until its size or mtime changes.
  - This holds both on disk and when nested inside another archive.
  - Detection must not try to decrypt anything:
    - ZIP: the encryption flag in the central directory.
    - 7Z: a password-required error on open, or an AES coder in a block.
    - RAR: encrypted headers, or an encrypted entry in the listing.
- **Lowercasing and merging:**
  - These apply to the *names shown* in both views. The real paths, which are kept for hovering and extraction, keep
    their original case.
  - Merging applies to real folders that coincide. In the Models view, two *different* models that clean up to the same
    name are not merged. They keep the prototype's rule: edition or raw name in brackets, e.g. `archivist zarvos (5e)`.
  - Clashing files are numbered in a stable order (by real path), starting at `_1`, and the postfix goes before the
    extension.
- **Overlapping roots** (e.g. `D:\Models` and `D:\Models\Vol.1`):
  - Each source is indexed only once, under the outermost root.
  - The inner root still appears as its own volume. Its contents merge with the same folders inside the outer volume.
- **Hover paths.**
  - The tooltip uses Windows separators up to the first archive, then `/` inside archives, with `!` at each archive
    boundary.
  - A merged folder lists each of its real paths, one per line.

## Deferred (known, not yet fixed)

From the milestone 2 review:

- **Slow ZIP opening.** Opening a ZIP reads every local file header twice. This can be slow on network drives, and one
  bad header fails the whole archive.
- **Peak memory.** In-memory nested archives can still peak at about 1.5 GiB in the worst case (3 levels × buffer
  growth).
- **Startup error dialog.** The error dialog at startup has never been exercised. `app_dir()` panics if `~/.stl-browser`
  can't be created.
- **Nested-archive warnings** are not stored, so the UI can't show which nested archives were skipped.
- **Edge cases:**
  - A file deleted between discovery and listing fails its folder source.
  - Paths that aren't valid Unicode are stored lossily.
  - A DB from a newer app version opens without a warning.
- **Missing tests:** Cyrillic paths end-to-end, multi-volume and solid RAR, and password-protected RAR (RAR fixtures
  can't be made without WinRAR; password-protected ZIP and 7Z are tested).

From the milestone 3 review:

- **Adding an outer root after an inner one** re-reads every archive, because the cache lookup filters on `root_id`.
  It is correct, but slow on a big volume, and it has no test.
- **Adding an outer root changes the resolution unit and the `m:` keys of the inner content.** All of Vol.1 becomes
  one collection under `STL`. Nothing uses those keys yet, but tags and thumbnail choices will need keys that survive
  this.
- **Duplicate roots already in a v3 DB** (`D:\Models` and `d:\models\`) are never merged. New ones are refused.
- **A source that keeps failing** makes its collection re-resolve on every run (about 0.1 s per collection).
- **Missing indexes:** `sources(root_id, collection)` and `assets(group_id)`.
- **`parse_name` compiles a regex on every call.**
- **ASCII-only digit checks** (`is_digits`, the designer-tag test). Sources with no collection are skipped.
- **The resolver merges same-stem archives** (`X.zip` + `x.rar`) into one model, as the prototype did. The
  `x_1`/`x_2` rule applies only to the Files view.

## Working notes

- Run all tests with `cargo test --workspace`, `npm --prefix app test` and `npm --prefix app run check`.
- To drive the running app from a script, start it with
  `WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS=--remote-debugging-port=9222` and talk CDP to `http://127.0.0.1:9222/json`. Run RAR against a real file with
  `STL_BROWSER_RAR_FIXTURE=<file.rar> cargo test -p stl-scan --test rar -- --ignored`.
- Dev CLI: `cargo run --release -p stl-index --bin stlx -- add <dir> | index | tree models|files [<path>] | parity
  <prototype.db> | extract <entry_id> <out>`. Set `STL_BROWSER_HOME` to keep test runs out of `~/.stl-browser`.
- `unrar_sys` doesn't link `advapi32`, so `stl-scan` links it explicitly.
- unrar can't write to a temp file that is still open. Close the handle first (`NamedTempFile::into_temp_path`).
- Shell heredocs in this environment mangle backslashes, and shell quoting breaks on text with an apostrophe. Write
  source files with an editor.
- `D:\Models\Sci-Fi Collection Vol.1\.3d-model-library\` (`*_DO_NOT_DELETE.json`) belongs to another tool. Discovery
  skips dot-folders, so leave it alone.
