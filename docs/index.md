# STL Browser: developer guide

A Windows desktop app (Rust + Tauri 2, Svelte 5 + TypeScript UI) for browsing 3D-print collections: folders of
STL/OBJ/LYS files and images, often inside RAR/ZIP/7Z archives nested inside other archives. You add folders to an
index and browse them like Windows Explorer with very large icons, in two views:

- *Models*: the resolver's cleaned-up tree (volume → collection → group → model → variant → parts).
- *Files*: the real disk and archive layout, where archives open like folders.

Each folder shows a carousel of the pictures inside it. Items can be tagged and searched, and anything can be dragged
out to Explorer to extract it.

## Where to read what

| Doc | Content |
|---|---|
| This page | Architecture, data on disk, invariants, testing, working notes |
| [`features.md`](features.md) | How every feature behaves and why: indexing, views, thumbnails, drag/extract, open, search, tags, multi-select |
| [`ui.md`](ui.md) | Layout, components, input, the pure TS modules, theme tokens, fixed UI text |
| [`virtual-tree.md`](virtual-tree.md) | Resolved tables, the Models and Files trees, naming rules, hover paths, node and tag keys |
| [`archive-patterns.md`](archive-patterns.md) | Collection layout patterns, the model resolver algorithm, the parity baseline |
| [`known-issues.md`](known-issues.md) | Open problems by area, and ideas not built yet |

UI mockup (clickable): <https://claude.ai/artifact/Vum7b4yY5TjCBW1Csez2BH>

When a feature lands, update these docs, not a plan: behaviour in `features.md`/`ui.md`, deferred items in
`known-issues.md`, structure here.

## Architecture

The repo is a Cargo workspace (`crates/*` + `app/src-tauri`), layered so I/O stays out of the core:

```text
crates/stl-core/    pure logic, no I/O
  names.rs            file kinds and name heuristics (variants, parts, image roles, parse_name, volume_name)
  resolve/            the model resolver: physical/logical trees, model detection, asset attachment, unit collapse
  vtree/              the tree type, naming rules and hover paths (mod.rs), models.rs and files.rs view builders,
                      search.rs, filter.rs (shown endings), tags.rs, export.rs (export plans), locate.rs (view switch)
crates/stl-scan/    archive access: archive.rs (ZIP/7Z/RAR behind one `Archive` trait), nested.rs, extract.rs,
                    session.rs (ExtractSession), discover.rs
crates/stl-thumb/   cache.rs, image.rs, mesh.rs (STL/OBJ), raster.rs (CPU renderer), generate.rs, queue.rs
crates/stl-drag/    Windows only: data.rs (IDataObject), stream.rs, source.rs; `prepare` (MTA), `run_drag` (STA)
crates/stl-index/   paths, db.rs (schema + migrations), config.rs, scanstore.rs, resolvestore.rs, views.rs,
                    thumbstore.rs, tagstore.rs, export.rs, parity.rs; bin/stlx.rs (dev CLI)
app/src-tauri/      Tauri shell: lib.rs (setup, `thumb` protocol), state.rs (DB, cached views, busy flags, thumbnail
                    service), commands.rs, browse.rs (listings, carousels, search, tags), indexer.rs, thumbs.rs,
                    export.rs (begin_drag, extract_to, open_file)
app/src/            SvelteKit UI (static adapter): routes/+page.svelte, components and pure modules in lib/ (see ui.md)
```

- **Native archive crates** (`zip`, `sevenz-rust2`, `unrar`), with no 7-Zip or other external tool at runtime.
  `unrar_sys` doesn't link `advapi32`, so `stl-scan/src/lib.rs` links it explicitly.
- **Commands** follow the `*_impl(state, …)` pattern: the `#[tauri::command]` wrapper calls a plain function that
  tests can call on an `AppState`. Slow commands are `#[tauri::command(async)]`. DTOs are serde `camelCase`, and the
  same shapes are mirrored in `app/src/lib/api.ts`.
- **Events:** `index-progress`, `index-done`, `thumb-ready`, `extract-progress`, `extract-done`.
- **The tree cache:** both view trees and their per-node counts are built once (on first use) into `AppState` and
  swapped in by the index worker after each run. Per-call user data (`selected_thumb`, tags, hides) is read from the
  DB on each command, so edits need no rebuild.

## Data on disk: `~/.stl-browser/`

`STL_BROWSER_HOME` points everything elsewhere (tests, `stlx`, scratch runs).

- **`index.db`** (SQLite, WAL, `foreign_keys = ON`, 5 s busy timeout), schema version 6:
  - **Cached listings, rebuildable:** `roots` (added folders), `sources` (one per archive or folder of loose files, with
    `size`, `mtime`, a file-name fingerprint, `locked`, `error`), `entries` (every file, with `path`, `chain`, `size`,
    `crc`, `locked`).
  - **Resolved tables, rebuilt per (root, collection):** `groups`, `models`, `model_files`, `assets`, plus `resolved`
    (collections that are up to date). They never hold user data.
  - **User data, never rebuilt:** `settings`, `selected_thumb`, `tags`, `node_tags`, `hidden_tags`. Keyed by stable
    path strings, never by row ids (ids change on every re-index). Rows for vanished paths are kept.
  - **Thumbnail bookkeeping:** `thumbs(entry_id, cache_rel, status, error)`, cascading with its entry.
- **`thumbs/<location>/<content>.jpg`**: the regenerable thumbnail cache (see [`features.md`](features.md#thumbnails-and-the-carousel)).
- **`config.yaml`**: `shown_extensions`, read once at startup.
- **`spot-checks.json`** (optional): `stlx parity` spot checks, kept out of the repo because they name real data.

**Schema changes** go through `MIGRATIONS` in `crates/stl-index/src/db.rs`: append a step, bump `SCHEMA_VERSION`, and
update `crates/stl-index/tests/db.rs` (and the app's schema-version test).

### Keys

| Key | Used for | Shape |
|---|---|---|
| Node key, Models | `selected_thumb`, carousel choices | `m:<collection>/<path>`; variants `<model key>#<variant>`, parts `<parent key>/<part>`, `#images`, `#docs` |
| Node key, Files | the same, and every leaf in both views | `f:<source path>|<entry path>`; disk folders `f:<folder path>|` |
| Tag key | `node_tags`, `hidden_tags` | `r:` + real path, lowercased, `/` → `\`, trailing `\` trimmed |

Tag keys use real paths so a tag means the same thing in both views and survives adding an outer root, which changes
`m:` keys.

## Invariants

- **Password-protected archives are never opened or decrypted.** They are detected from headers, stored with
  `locked = 1` and no entries, and shown as one padlock item. They are left out of drags and extractions.
- **No archive may block on a password prompt or crash the app.** The release profile uses `panic = "unwind"` on
  purpose, so a panic inside an archive library becomes an error on that source or file. Don't switch to `abort`.
- **Names shown are lowercase in both views**; real paths keep their case for hovering and extraction. Merge and
  numbering rules are in [`virtual-tree.md`](virtual-tree.md).
- **No entry id is trusted across an index run.** Drags, Extract to… and opening files are refused while a run is
  busy, and an export locates `(source, chain)` up front.
- **Only one index run and one extraction at a time** (guards on `AppState`).
- **The UI thread never extracts.** Index runs, extraction and thumbnails run on their own threads with their own DB
  connections. Drag data is served from the MTA.
- **Nothing is fetched at runtime** (offline fonts, no CDN).
- **Discovery skips dot-folders.** `.3d-model-library\` folders belong to another tool; leave them alone.
- **No real names in the repo.** Tests and docs use invented collection, studio and model names.

## Testing

- `cargo test --workspace`, `npm --prefix app test` (`node --test` on `src/lib/*.test.ts`), and
  `npm --prefix app run check` (svelte-check). All three must pass at every commit.
- **App fixtures:** `app/src-tauri/src/testutil.rs` `library(dir)` writes loose files under `lib/Vol/Coll/`
  (`Orc/orc.stl`, `Orc/orc.jpg`, `Orc/notes.txt`, `Elf/elf.stl`), indexes them and opens an `AppState`. Most
  `browse.rs`, `commands.rs` and `export.rs` tests use it, through `vol/coll/orc` and `vol/coll/elf`.
- **Archive fixtures** are built at test time (`crates/stl-scan/tests/common`: zip, 7z, nested, AES zip). There are no
  RAR fixtures, because RARs can't be made without WinRAR. RAR tests read a real file:
  `STL_BROWSER_RAR_FIXTURE=<file.rar> cargo test -p stl-scan --test rar -- --ignored`. Verify RAR changes against a
  real RAR holding many nested zips.
- **Real-data drag without a mouse:** point `STL_BROWSER_HOME` at an indexed home, set
  `STL_BROWSER_DRAG_CHECK="models|<folder path>|<name>"`, and run `cargo test -p app --lib drag_real -- --ignored
  --nocapture`. It drops the item on the shell's drop target for a temp folder.
- **Resolver regressions:** `stlx parity <other.db>` compares the resolved tables of two indexes per collection, plus
  the spot checks. See [`archive-patterns.md`](archive-patterns.md).
- **Driving the running app:** start it with `WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS=--remote-debugging-port=9222`
  and talk CDP to `http://127.0.0.1:9222/json` (`invoke` for commands, the DOM for the UI). Native dialogs, real mouse
  drags and launching Windows apps still need a human.

## Working notes

- Dev CLI: `cargo run --release -p stl-index --bin stlx -- add <dir> | index | tree models|files [<path>] [--depth N]
  [--real] | parity <db> | extract <entry_id> <out>`.
- Two COM gotchas: `FILEDESCRIPTORW` is packed (build one, then copy it in with `write_unaligned`), and
  `CoGetInterfaceAndReleaseStream` releases the stream itself (hold it in `ManuallyDrop`).
- unrar can't write to a temp file that is still open. Close the handle first (`NamedTempFile::into_temp_path`), or
  nested archives inside RARs silently fail.
- Editing `app/src-tauri/Cargo.toml` while `tauri dev` runs starts a rebuild that fails, because `app.exe` is locked.
- Shell heredocs in this environment mangle backslashes, and shell quoting breaks on text with an apostrophe. Write
  source files with an editor.
- **Releases are automatic.** Every push to `main` runs `.github/workflows/release.yml`. Commitizen (`.cz.toml`) picks
  the next version from the conventional commits, writes it to `Cargo.toml`, `Cargo.lock`, `tauri.conf.json`,
  `package.json` and `package-lock.json`, updates `CHANGELOG.md`, and commits and tags as the `commitizen-ci` GitHub App
  (`CZ_APP_CLIENT_ID` variable, `CZ_APP_PRIVATE_KEY` secret). A Windows runner builds the exe and the NSIS installer
  and attaches them to the release. A version set by hand that has no tag yet is released as it is. Internal path
  dependencies don't pin a `version`, so a bump doesn't break them.
