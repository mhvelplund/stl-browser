# STL Browser — design overview

A Windows desktop app (Rust + Tauri 2, Svelte 5 UI) for browsing 3D-print collections: folders of STL/OBJ/LYS files and
images, often inside RAR/ZIP/7Z archives nested inside other archives. You add folders to an index and browse them like
Windows Explorer with very large icons. Each folder shows a carousel of the pictures inside it, and you can drag
anything out to Explorer to extract it.

- Full design: [`superpowers/specs/2026-10-27-stl-browser-design.md`]
- Implementation plans: [`superpowers/plans/2026-10-27-skeleton-and-scan.md`] (milestones 1–2),
  [`superpowers/plans/2026-10-27-resolve.md`] (milestone 3), `superpowers/plans/2026-10-27-browse-ui.md`
  (milestone 4), `superpowers/plans/2026-10-28-thumbnails.md` (milestone 5) and
  `superpowers/plans/2026-10-28-drag-out.md` (milestone 6). The last three are git-ignored.
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
| 5. Thumbnails: cache, queue, image + STL/OBJ renderer, carousel                              | done                                          |
| 6. Drag-out: Windows virtual-file drag, plus "Extract to…"                                   | done                                          |
| 6.5. Usability pass: stop buttons, folder rescans, view-switch place, open files, STL pictures | done (branch `usability-pass`)              |
| 7. Search: name search with Free/Word/Regexp, scoped to the open folder                      | built (branch `m7-search`), manual check due  |
| 8. Tags and filters                                                                          | later (only the database tables exist so far) |

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

Milestone 5 was smoke-tested the same way on Vol.1 + Vol.2:

- **Collection composites** fill in. The first ~50 thumbnails, from big solid RARs, take about a minute.
- **STL renders:** Robots STC (no images) shows them, and models with pictures default to their hero.
- **Carousel:** a flipped slot is saved and survives a restart.
- **Cache:** deleting `thumbs/` while the app runs regenerates on reopen.
- **Status bar:** the queue counts down to "up to date".

Milestone 6 was checked on Vol.1 without a mouse:

- **The spike** proved the risky part first. A data object living in the MTA, used on an STA through a marshaled proxy
  and dropped on the shell's own drop target for a folder, is copied asynchronously. `GetData` is called on a COM
  worker thread, not on the STA.
- **Extract to…** (driven over CDP in `tauri dev`) wrote two of Grimlax's models: `ancient sarcophagus` (65 files,
  straight from the RAR) and `phalanx of dawn e` (4 files, about 95 MB, from a zip inside the RAR). Both trees match
  `stlx tree models … --real` exactly.
- **The drag pipeline on real data.** `drag_real_item_to_shell_folder` (an opt-in test) drops the same two models on the
  shell's drop target. They copy in 1.3 s and 5.3 s, with every file at its indexed size.
- **`begin_drag` with no button held** returns at once and starts no drag.
- **Checked by hand afterwards** (2026-10-29): a real mouse drag from the window to Explorer works, Escape during a drag
  writes nothing, and the context menu's Extract to… works.

The usability pass (6.5) was checked in `tauri dev` on a scratch index of Vol.1, driven over CDP:

- **Rescan folder in Grimlax**, with every source marked stale, re-listed only `Grimlax.rar` (1 of 45) in 15 s.
- **Stop during Rescan all** ended after 5 of 45 sources and deleted nothing: 45 sources, 18,050 files and 447 models
  stayed. The run ended 9 s after Stop, because the archive being listed has to finish first.
- **Stop during Extract to…** of `ancient sarcophagus` ended 0.19 s after Stop, with 3 of 65 files written.
- **Switching views** in `ancient sarcophagus` lands on `grimlax/731 - ancient sarcophagus` in Files, and back.
- **The carousel** of `ancient sarcophagus` has its JPG and then 6 STL renders, with the JPG as the default.
  - "Use as folder thumbnail" on an STL in `supported` makes it that folder's picture.
  - The model then takes only that one picture from `supported`.
- **A synthetic mouse Back** (button 3) goes up one folder and the webview doesn't navigate.
- **Not checked by script:** double-clicking a file to open it in its Windows app (it would launch apps on the
  desktop), and a real mouse's Back button.

## Architecture

```text
Cargo.toml                 workspace
crates/stl-core/           pure logic, no I/O: file kinds and name heuristics (names.rs), the model resolver
                           (resolve/), the virtual tree and both views (vtree/)
crates/stl-scan/           archive readers (ZIP, 7Z, RAR behind one trait), nested listing, extract by chain, discovery
crates/stl-thumb/          thumbnails: cache names (cache.rs), image scaling + JPEG (image.rs), STL/OBJ loading
                           (mesh.rs), the CPU rasterizer (raster.rs), batches read in one archive pass (generate.rs),
                           the priority job queue (queue.rs); `examples/thumb.rs` renders one local file to PNG
crates/stl-drag/           Windows only: the OLE virtual-file data object (data.rs), streams over delete-on-close temp
                           files (stream.rs), the drop source (source.rs); `prepare` (MTA) and `run_drag` (STA)
crates/stl-index/          app dir, SQLite schema + migrations, incremental scans (scanstore.rs), per-collection resolve
                           (resolvestore.rs), views from the DB (views.rs), thumbnail rows (thumbstore.rs), prototype
                           parity (parity.rs), export plans located and written to a folder (export.rs), `stlx` dev CLI
app/src-tauri/             Tauri shell: state (DB + cached views + busy flag + thumbnail service), commands,
                           browse.rs (listings and carousels), indexer.rs (background index runs), thumbs.rs
                           (thumbnail requests, workers, `thumb-ready`, the `thumb` protocol), export.rs
                           (`begin_drag`, `extract_to`)
app/src/                   SvelteKit UI (static adapter): routes/+page.svelte plus components and pure logic
                           modules (grid, nav, items, indexrun, carousel) in lib/, tested with `node --test`
```

Two smaller modules came with milestone 6:

- `stl-core::vtree::export` builds the export plan: which folders and files a drag writes, under Windows-safe names.
- `stl-scan::session` extracts many files one by one and keeps the nested archives open between them.

### Drag-out and Extract to… (milestone 6)

- **Gesture:** pressing a tile and moving 6 px with the primary button held calls `begin_drag(view, path, [name])`.
  - Rust builds the export plan, locates every file (source, chain, size) and prepares the data object.
  - It then runs `DoDragDrop` on the main thread through `run_on_main_thread`.
  - If the button is already up by then, no drag starts.
- **The data object** offers three formats:
  - `CFSTR_FILEDESCRIPTORW`: one descriptor per folder and per file, with paths that mirror the current view;
  - `CFSTR_FILECONTENTS`: one stream per file, extracted only when Explorer asks for it;
  - `CFSTR_PREFERREDDROPEFFECT`: copy.

  It also implements `IDataObjectAsyncCapability`, so Explorer copies on its own thread, with its own progress dialog.
- **Threads:** the object is created in the MTA, which `CoIncrementMTAUsage` keeps alive. `DoDragDrop` gets a marshaled
  proxy, so Explorer's calls go straight to the MTA and extraction never runs on the UI thread.
- **Streams** are delete-on-close temp files. No file is held in memory, and nothing is left behind after a crash.
- **Extraction** goes through one `ExtractSession` per drag, or per Extract to…:
  - It keeps the stack of open archives, so files from the same nested archive in a row don't reopen it.
  - A panic in an archive reader fails only that one file.
- **Right-click** on a tile offers **Open** and **Extract to…**.
  - Extract to… writes the same tree into a chosen folder, on a background thread (`extract-progress`,
    `extract-done`).
  - A top-level name that already exists there becomes `name (2)`, so nothing is overwritten.
  - Files are written as `.part` and renamed when complete.
  - A file that fails is reported, and the rest are still written.

### Data on disk: `~/.stl-browser/`

- `index.db` (SQLite, WAL), at schema version 5:
  - **Cached listings:** `roots` (the added folders), `sources` (each archive, or each folder of loose files), `entries`
    (every file found, with `chain`, the archive members to open in order to reach it).
  - **Resolved tables:** `groups`, `models`, `model_files`, `assets`. They are rebuilt per (root, collection) whenever
    one of that collection's sources changes. `resolved` marks the collections that are up to date.
  - **User data, never rebuilt:** `settings`, `selected_thumb`, `tags`, `node_tags`. These are keyed by stable path
    strings, never by row ids.
  - **Thumbnail bookkeeping:** `thumbs(entry_id, cache_rel, status, error)`.
    - A row goes with its entry (cascade), so a re-listed source drops its rows.
    - A row only counts while its file exists.
    - `error` rows are not retried until the entry changes.
- `thumbs/<location>/<content>.jpg`: thumbnails within 256×256 (JPEG, quality 80) that can always be regenerated.
  Images keep their shape; STL/OBJ renders are square on the tile grey `#2A2E34`, since JPEG has no transparency.
  - `<location>` is the first 16 hex chars of blake3(source path + `\0` + chain).
  - `<content>` is `<crc>-<size>` from the archive, or blake3 of the bytes for loose files.
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
- **UI:** toolbar (Back, Up, breadcrumbs, Models | Files, Search, Rescan (🗘), Add folder), a title row with counts, a virtualized
  grid of tiles (min 200 px, square art, name, caption) and a status bar (stats, missing folders, the selected item's
  real path, index progress). Placeholder art per kind until thumbnails; a box-with-padlock for locked archives.
  Double-click or Enter opens; Backspace / Alt+Up goes up; Alt+Left goes back (Back restores the scroll position).
- **Search:** the magnifying glass opens a bar under the toolbar that searches the open folder and everything below
  it. Free (letters in order, the default), Word (one piece) or Regexp; Aa (case) only in the Files view, whose names
  keep their case. Typing swaps the grid for a list of matches (kind icon, highlighted name, where it is); a folder
  opens in the browser and closes search, a file opens in its app. Esc closes the bar. Matching runs in Rust on the
  cached view tree (`stl_core::vtree::search`, `search` command), at most 1000 results.

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
- **Thumbnails are 256×256 JPEG files at quality 80** *(added after milestone 5, replacing the 128×128 WebP files)*.
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
- **Usability pass** *(added after milestone 6)*:
  - Slow tasks (indexing, Extract to…) have a Stop button next to their progress bar.
  - The mouse Back button goes to the parent folder.
  - A folder's carousel offers its STL renders too, after its images. A file tile's right-click menu has "Use as folder
    thumbnail".
  - A folder's chosen picture counts as that folder's picture for its parents: their carousels don't look inside it.
  - "Rescan all" is "Rescan folder" inside a folder and rescans only that folder and its subfolders. Right-click →
    Rescan does the same for a folder tile.
  - Switching between Models and Files keeps you at, or near, the same place.
  - Double-clicking a file opens it in its Windows app.
- **Releases are automatic** *(added during milestone 7)*. Every push to `main` runs `.github/workflows/release.yml`:
  - Commitizen (`.cz.toml`) picks the next version from the conventional commits since the last tag. It writes it to
    `Cargo.toml`, `Cargo.lock`, `tauri.conf.json`, `package.json` and `package-lock.json`, updates `CHANGELOG.md`, and
    commits and tags as the `commitizen-ci` GitHub App (client ID in the `CZ_APP_CLIENT_ID` variable, key in the
    `CZ_APP_PRIVATE_KEY` secret).
  - A version set by hand that has no tag yet is released as it is (this is how 0.3.0 ships).
  - A Windows runner builds the exe and the NSIS installer and attaches them to a new GitHub release.
  - Internal path dependencies no longer pin a `version`, so a bump doesn't break them.

### Made by me (correct any that are wrong)

- **Usability pass details:**
  - **A stopped index** deletes no vanished sources, because it hasn't seen them all. It still resolves the
    collections it changed, then skips the remaining roots.
  - **A rescan's scope** comes from the open folder's real paths, cut at the first archive. Discovery still walks the
    whole root, but only sources in scope are listed or deleted. A folder source counts when the scope is inside it,
    except for the root's own one (it holds only the files directly in the root). At Library, everything is rescanned.
  - **Switching views** goes to the deepest folder of the other view whose real path contains this one. If there is
    none, it goes to the shallowest folder below it. If there is still none, it tries the parent folder, up to Library.
  - **The carousel** shows images (up to 11) and then meshes (up to 6). A composite is filled up with images, and with
    meshes only when the folder has no images. That way a model with one image and some STLs doesn't get a 2×2.
  - **A `composite` choice and a stale choice** don't stand in for a folder: its parents look inside it as before.
  - **Opening a file:**
    - A loose file opens in place.
    - An archive member is written to `%TEMP%\stl-browser-open\<n>\` first, and that folder is emptied at app start.
    - Opening is refused during an index run, like Extract to….
    - Only models, scenes, images and documents (not `.html`) are opened, never programs, scripts or shortcuts from an
      archive. A copy written from an archive gets a Mark-of-the-Web (`Zone.Identifier`, internet zone).
    - A second double-click while a file is still being extracted is ignored.

- **Thumbnail content hash.** Archive entries use the CRC32 + size stored in the archive, so the cache can be checked
  without extracting. Loose files use blake3 of their bytes.
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
- *Added in milestone 6:*
  - **A drag writes the tree as the view shows it**, with lowercase names and the `_N` clash names. The Files view
    takes only its visible items (folders, images and mesh files). The Models view also takes `docs`.
  - **Windows-safe names:**
    - `< > : " / \ | ? *` and control characters become `_`.
    - Trailing dots and spaces are dropped.
    - Device names (`con`, `com1.stl`) get a leading `_`.
    - Names that clash after this get `_2`, `_3`.
  - **Locked archives are left out** of drags and extractions, and counted as skipped.
  - **Explorer copies exactly the declared size** (measured). It cuts a file short when the size given is too small, and
    stops at the end of the stream when it is too big. So in a drag, a file without a size, or longer than its indexed
    size (a stale index), fails rather than being truncated.
  - **Paths over 259 characters** don't fit in a file descriptor. They are left out of a drag, and the UI says how many.
    Extract to… still writes them.
  - **One Extract to… at a time.** A second one is refused while the first runs.
  - **No drag or Extract to… during an index run.** A run re-lists sources and reuses entry ids before the views are
    rebuilt, so a plan made then could name the wrong file. Both are refused with a message until the run ends.
  - **7z and RAR levels are read in one pass.** Those readers decode from the start of the archive for every member, so
    file-by-file extraction is quadratic.
    - Extract to… writes all the files of one 7z/RAR level from one pass.
    - A drag reads the level on the first request into it. It keeps the other files from that level in delete-on-close
      temp files, up to 2 GiB, and serves Explorer's later requests from them.
    - Zip members and loose files are read directly.
  - **Results** (an extraction finished, items left out of a drag) stay in the status bar until the next selection.
- *Added in milestone 5:*
  - **The queue replaces, it doesn't append.** Each request sends what is on screen (high) and the rest of the folder
    (low), replacing whatever was queued, so a folder you left doesn't keep the workers busy. Jobs from one archive run
    as one batch (≤ 64), and the archive is read in a single pass.
  - **Std worker threads instead of rayon** (cores − 1). The queue needs priorities, grouping and replace, which a
    rayon pool doesn't give.
  - **Carousel slots:**
    - Pictures are up to 11 nested images (the hero first if it lives outside the folder), else up to 6 nested STL/OBJ
      files.
    - The composite is four child folders' shown pictures, filled up from the folder's own pictures. It needs at least
      2 cells, otherwise there is no composite slot.
    - Default slot: the hero, else slot 0.
    - The saved choice is honoured only while it still exists.
  - **The CRC shortcut.** An archive entry's cache name is known from its stored CRC, so a hit costs no extraction.
  - **Size caps:** images ≤ 64 MiB, meshes ≤ 256 MiB.
  - **Small images are scaled up** to fill the square.
  - **No thumbnail for macOS AppleDouble files** (`__MACOSX/`, `._x.stl`). They borrow real file names, and they were
    being picked as a model's meshes.
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
  one collection under `STL`. Saved thumbnail choices (`selected_thumb`) use these keys since milestone 5, so they are
  lost for that content; tags will need keys that survive this.
- **Duplicate roots already in a v3 DB** (`D:\Models` and `d:\models\`) are never merged. New ones are refused.
- **A source that keeps failing** makes its collection re-resolve on every run (about 0.1 s per collection).
- **Missing indexes:** `sources(root_id, collection)` and `assets(group_id)`.
- **`parse_name` compiles a regex on every call.**
- **ASCII-only digit checks** (`is_digits`, the designer-tag test). Sources with no collection are skipped.
- **The resolver merges same-stem archives** (`X.zip` + `x.rar`) into one model, as the prototype did. The
  `x_1`/`x_2` rule applies only to the Files view.

From the milestone 4 review:

- **Navigation can be lost under rapid input.** `nav` only changes when a listing arrives, so two quick Backspaces go
  up one level, and an `index-done` reload that lands mid-navigation drops the click.
- **`Listing.path` echoes the request's casing.** It's harmless today because the UI always sends tree names.
- **"files" means two things on screen.** The footer counts every indexed entry, while the title and tiles count only
  mesh files.
- **Selecting a tile hides the last index result** in the status bar until the selection clears.
- **UI polish:**
  - Locked tiles are tinted, not dimmed.
  - A view switch draws one frame with the old listing.
  - After a fallback, the old scroll offset is restored.
- **Keyboard and screen readers:**
  - Tab can't reach tiles outside the rendered rows, and there are no arrow keys in the grid.
  - The progress block's `role="status"` announces every source.
- **Files view folders that hold only hidden files** show as empty folders. They are not pruned.

From the milestone 5 review:

- **Broken thumbnail images.** A thumbnail `<img>` that 404s shows the browser's broken-image glyph. It has no
  `onerror`, and an `immutable` URL won't reload.
- **A composite whose cells all failed** shows four blank cells instead of the kind icon.
- **Skeleton flash after an index run.** Every tile shows the "generating" placeholder briefly, because the url map
  is cleared before the reload answers.
- **Image decoding is heavy.** It converts and resizes at full resolution, which costs a lot of memory and CPU for
  8k photos multiplied by the worker count.
- **The `thumb` protocol spawns one OS thread per request.** It should use `spawn_blocking`.
- **The queue builds a group key per queued job on every batch pop.** That is O(N) work under the lock.
- **Solid archives are decompressed once per 64-job batch.** Batches of one archive can also run on several workers
  at once.
- **A NULL entry size becomes 0**, which bypasses the size caps. Archive listings always store sizes.
- **Orphaned cache files are never removed.** The cache grows as archives change, and the `.webp` files from before
  the switch to JPEG stay behind too. Deleting `thumbs/` is always safe.
- **Models without images show their first mesh in tree order**, often a base or an arm. Picking the largest file
  would do better.
- **Not smoke-checked:** thumbnails in the light theme, OBJ files from real data, and padlock tiles next to
  thumbnails.

From the milestone 6 review:

- **Extract to… may act on the wrong folder.** It reads the current folder after the folder picker returns. Backspace
  with the context menu open also navigates. Either way it can use another folder (usually giving "no longer exists").
- **The status line lists every failed file.** A source deleted since indexing can fill it with thousands of entries.
- **Closing the app during Extract to…** leaves `.part` files. (It can be stopped since 6.5.)
- **The app never learns how a drag ended.** A file that fails shows only in Explorer's own error dialog.
- **Stream edge cases:**
  - `IStream::Read` doesn't check for a null buffer.
  - `CopyTo` drops bytes if the target accepts a short write.
- **Large drags:**
  - The descriptor block is rebuilt on every `GetData`, which costs about 60 MB per call at 100k files.
  - Drag streams are staged in `%TEMP%`, so a file bigger than the free space on C: fails.
- **Reserved names:** `conin$`, `conout$` and the superscript `com¹` / `lpt¹` device names are not escaped.
- **A tiny race:** a button release between the button check and `DoDragDrop` drops onto whatever is under the cursor.
- **No drag image:** the drag shows only the cursor, with no ghost thumbnail.
- **Swapped mouse buttons** have not been tested with the OLE key state.

From the usability pass:

- **Stop waits for the current source.** Listing one big archive (Grimlax's RAR, about 15 s) can't be interrupted
  part-way.
- **Opening an archive member blocks its command** until the member is extracted, with only "Opening …" in the status
  bar and no progress.
- **Files opened from archives stay in `%TEMP%`** until the next app start. One still open in its app at that point
  isn't deleted.
- **More thumbnails per folder:** carousels now also ask for up to 6 STL renders, so a first visit renders more.
- **Listings walk further:** collecting a folder's pictures now goes on until both the image and the mesh caps are
  reached, so a big subtree with only images is walked to the end. `locate` lowercases every real path of the target
  view on each switch. Both are fine on Vol.1 + Vol.2 so far.
- **A rescan of a folder renamed on disk** deletes its sources, but its new name is outside the scope and isn't added.
  Its models are gone until a parent folder is rescanned.
- **Opening a file from an archive can't be stopped.**

## Working notes

- Run all tests with `cargo test --workspace`, `npm --prefix app test` and `npm --prefix app run check`.
- To drive the running app from a script, start it with
  `WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS=--remote-debugging-port=9222` and talk CDP to `http://127.0.0.1:9222/json`.
- Check the drag pipeline on real data without a mouse:
  - Point `STL_BROWSER_HOME` at an indexed home and set `STL_BROWSER_DRAG_CHECK="models|<folder path>|<name>"`.
  - Run `cargo test -p app --lib drag_real -- --ignored --nocapture`.
  - It drops the item on the shell's drop target for a temp folder.
- Run RAR against a real file with
  `STL_BROWSER_RAR_FIXTURE=<file.rar> cargo test -p stl-scan --test rar -- --ignored`.
- Dev CLI: `cargo run --release -p stl-index --bin stlx -- add <dir> | index | tree models|files [<path>] | parity
  <prototype.db> | extract <entry_id> <out>`. Set `STL_BROWSER_HOME` to keep test runs out of `~/.stl-browser`.
- `unrar_sys` doesn't link `advapi32`, so `stl-scan` links it explicitly.
- Two COM gotchas from milestone 6:
  - `FILEDESCRIPTORW` is packed: build one, then copy it in with `write_unaligned`.
  - `CoGetInterfaceAndReleaseStream` releases the stream itself, so hold it in `ManuallyDrop`.
- Editing `app/src-tauri/Cargo.toml` while `tauri dev` runs starts a rebuild, which fails because `app.exe` is locked.
- unrar can't write to a temp file that is still open. Close the handle first (`NamedTempFile::into_temp_path`).
- Shell heredocs in this environment mangle backslashes, and shell quoting breaks on text with an apostrophe. Write
  source files with an editor.
- `D:\Models\Sci-Fi Collection Vol.1\.3d-model-library\` (`*_DO_NOT_DELETE.json`) belongs to another tool. Discovery
  skips dot-folders, so leave it alone.
