# STL Browser — design overview

A Windows desktop app (Rust + Tauri 2, Svelte 5 UI) for browsing 3D-print collections: folders of STL/OBJ/LYS files and
images, often inside RAR/ZIP/7Z archives nested inside other archives. You add folders to an index and browse them
like Windows Explorer with very large icons. Each folder shows a carousel of the pictures inside it, and you can drag
anything out to Explorer to extract it.

- Full design: [`superpowers/specs/2026-10-27-stl-browser-design.md`](superpowers/specs/2026-10-27-stl-browser-design.md)
- Current implementation plan: [`superpowers/plans/2026-10-27-skeleton-and-scan.md`](superpowers/plans/2026-10-27-skeleton-and-scan.md)
- UI mockup (clickable): https://claude.ai/artifact/Vum7b4yY5TjCBW1Csez2BH
- `prototype/` holds the Python prototype and its notes (`archive-patterns-plan.md`, `folder-structure.md`,
  `HANDOFF.md`). It is the reference for the heuristics. It is **not** part of the app, and nothing in it is built or
  shipped.

## Status

| Milestone                                                                                   | State                                           |
|---------------------------------------------------------------------------------------------|-------------------------------------------------|
| 1. Skeleton: workspace, Tauri app, `~/.stl-browser/index.db`, schema + migrations, settings | done                                            |
| 2. Scan: native ZIP/7Z/RAR, nested archives, extract by chain, discovery, incremental rescan | done                                            |
| 3. Resolve: port of the prototype's model resolver and virtual tree                         | next                                            |
| 4. Browse UI: add folder, rescan, Models/Files views, grid, breadcrumbs                     | planned                                         |
| 5. Thumbnails: cache, queue, image + STL/OBJ renderer, carousel                             | planned                                         |
| 6. Drag-out: Windows virtual-file drag, plus "Extract to…"                                  | planned                                         |
| 7. Tags, search, filters                                                                    | later (only the database tables exist so far)   |

Milestones 1–2 were checked against the real `D:\STL\Warhammer 40K Collection Vol.1`. The scan found 45 sources and
18,050 files (1,243 of them in nested archives) with 0 errors, the same as the prototype. It takes about 35 s, and a
rescan of unchanged folders takes 0 s.

## Architecture

```
Cargo.toml                 workspace
crates/stl-core/           pure logic, no I/O: file kinds, name helpers (later: resolver, virtual tree)
crates/stl-scan/           archive readers (ZIP, 7Z, RAR behind one trait), nested listing, extract by chain, discovery
crates/stl-index/          app dir, SQLite schema + migrations, incremental scan storage, `stlx` dev CLI
app/src-tauri/             Tauri shell: state, commands
app/src/                   SvelteKit UI (static adapter)
prototype/                 reference only
```

Planned crates: `stl-thumb` (thumbnails and the STL renderer) and `stl-drag` (Windows OLE drag-out).

### Data on disk: `~/.stl-browser/`

- `index.db` (SQLite, WAL), at schema version 2:
  - **Cached listings:** `roots` (the added folders), `sources` (each archive, or each folder of loose files),
    `entries` (every file found, with `chain`, the archive members to open in order to reach it).
  - **User data, never rebuilt:** `settings`, `selected_thumb`, `tags`, `node_tags`. These are keyed by stable path
    strings, never by row ids.
  - **Later:** resolved tables (`groups`, `models`, `model_files`, `assets`, rebuilt on each index) and `thumbs`.
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
- **Robustness:** a corrupt archive records an error on its source and the scan continues. A crash (panic) inside an
  archive library is caught and recorded the same way. No format can ever stop to ask for a password.

## Decisions

### Made by you

- **Rust + Tauri 2** with **Svelte 5 + TypeScript**.
- **Native archive crates, no 7-Zip.**
- **Two switchable views:**
  - *Models*: the resolver's cleaned-up tree (volume → collection → group → model → variant → parts).
  - *Files*: the real disk and archive layout, where archives open like folders.
- **An added folder becomes a top-level volume.**
- **Drag-out uses Windows virtual-file drag**, so files are extracted only when Explorer asks for them. "Extract to…"
  is a fallback.
- **Folder carousel.** Slot 0 is a 2×2 composite of four child folders' thumbnails, and then come the nested images.
  The chosen slot is saved in the DB.
- **Thumbnails are generated when a folder is first opened**, cached as `<hash of path>/<hash of file>`, and
  regenerated if missing. STL files get rendered thumbnails.
- **Tags, search and filtering come later.** User data lives in the SQLite DB in `~/.stl-browser/`.
- **Password-protected archives are not supported** *(added after milestone 2)*:
  - Don't try to scan them.
  - Show them as a single item with a **box-with-padlock** icon.
- **All file and folder names are lowercase in the virtual file system** *(added after milestone 2)*:
  - Folders that overlap merge their contents. This includes names differing only in case, like `D:\STL` and
    `d:\stl\`, and an added folder that sits inside another added folder.
  - Files whose names collide get a numbered postfix: `ORC.STL` and `Orc.stl` become `orc_1.stl` and `orc_2.stl`.
- **Hovering over any element shows its real path** *(added after milestone 2)*. Each step into an archive is
  marked with `!`, e.g. `folder/example.zip!orcs/mech.zip!leg.stl`.

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
- **Panics are caught.** The release build uses `panic = "unwind"`, so a crash inside an archive library becomes an
  error on that archive instead of closing the app.

### How I'm reading your new rules (my interpretation; confirm or correct)

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
  - Merging applies to real folders that coincide. In the Models view, two *different* models that clean up to the
    same name are not merged. They keep the prototype's rule: edition or raw name in brackets, e.g.
    `illuminor szeras (5e)`.
  - Clashing files are numbered in a stable order (by real path), starting at `_1`, and the postfix goes before the
    extension.
- **Overlapping roots** (e.g. `D:\STL` and `D:\STL\Vol.1`):
  - Each source is indexed only once, under the outermost root.
  - The inner root still appears as its own volume. Its contents merge with the same folders inside the outer
    volume.
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
- **Startup error dialog.** The error dialog at startup has never been exercised. `app_dir()` panics if
  `~/.stl-browser` can't be created.
- **Nested-archive warnings** are not stored, so the UI can't show which nested archives were skipped.
- **Edge cases:**
  - A file deleted between discovery and listing fails its folder source.
  - Paths that aren't valid Unicode are stored lossily.
  - A DB from a newer app version opens without a warning.
- **Missing tests:** Cyrillic paths end-to-end, and multi-volume and solid RAR (RAR fixtures can't be made without
  WinRAR).
- **Overlapping roots and drive-root volume names** (`D:\`). These are now covered by the new merge rule and get
  fixed with it.

## Working notes

- Run all tests with `cargo test --workspace`. Run RAR against a real file with
  `STL_BROWSER_RAR_FIXTURE=<file.rar> cargo test -p stl-scan --test rar -- --ignored`.
- Dev CLI: `cargo run --release -p stl-index --bin stlx -- add <dir> | scan | extract <entry_id> <out>`. Set
  `STL_BROWSER_HOME` to keep test runs out of `~/.stl-browser`.
- `unrar_sys` doesn't link `advapi32`, so `stl-scan` links it explicitly.
- unrar can't write to a temp file that is still open. Close the handle first (`NamedTempFile::into_temp_path`).
- Shell heredocs in this environment mangle backslashes. Write source files with an editor.
