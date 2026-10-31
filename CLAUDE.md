# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project

Windows desktop app (Rust + Tauri 2, Svelte 5 + TypeScript UI) for browsing 3D-print collections: STL/OBJ/LYS files and
images, often inside RAR/ZIP/7Z archives nested in other archives. Folders are added to a SQLite index, rescanned
incrementally, and browsed Explorer-style.

**Read `docs/index.md` first.** It holds the milestone status, architecture, every design decision and the deferred
review items. Other binding docs:

- `docs/superpowers/specs/2026-10-27-stl-browser-design.md`: the design spec, the final authority.
- `docs/archive-patterns.md`: layout patterns, the model resolver algorithm, the parity baseline and spot checks.
- `docs/virtual-tree.md`: resolved tables, Models/Files views, naming rules (lowercase, merge, `_1` postfixes, `!` hover
  paths, padlock items).
- `HANDOFF.md` (if present): the current session handoff and the next step.

## Commands

```sh
cargo test --workspace                                   # all Rust tests
cargo test -p stl-scan --test nested                     # one integration test file
cargo test -p stl-index --test db <name_filter>          # one test by name
STL_BROWSER_RAR_FIXTURE=<some.rar> cargo test -p stl-scan --test rar -- --ignored   # RAR test, needs a real file
STL_BROWSER_DRAG_CHECK="models|<folder>|<name>" cargo test -p app --lib drag_real -- --ignored   # real-data drop; needs an indexed STL_BROWSER_HOME
cargo run --release -p stl-index --bin stlx -- add <dir> | index | extract <entry_id> <out>   # dev CLI
cargo run --release -p stl-index --bin stlx -- tree models|files [<path>] [--depth N] [--real]  # print a view
cargo run --release -p stl-index --bin stlx -- parity <prototype.db>   # compare resolved tables with the prototype (+ spot-checks.json in the app dir)
npm --prefix app run tauri dev                           # launch the app
npm --prefix app run build:exe                           # release exe only -> target/release/stl-browser-<version>.exe
npm --prefix app run build:installer                     # NSIS installer -> target/release/bundle/nsis/
npm --prefix app run check                               # svelte-check (UI type check)
npm --prefix app test                                    # UI logic tests (node --test on src/lib/*.test.ts)
```

Set `STL_BROWSER_HOME=<scratch dir>` for `stlx` and test runs so they don't touch the real `~/.stl-browser/`.

## Architecture

Cargo workspace (`crates/*` + `app/src-tauri`), layered so I/O stays out of the core:

- `crates/stl-core`: pure logic, no I/O. `names.rs` (file kinds, name heuristics); `resolve/` (the model resolver, a
  faithful port of the prototype: physical/logical trees, model detection, asset attachment, unit collapse);
  `vtree/` (the tree type, naming rules and hover paths, `models.rs` and `files.rs` view builders fed by row structs).
- `crates/stl-scan`: archive access. `archive.rs` puts ZIP/7Z/RAR behind one `Archive` trait (`open`); `nested.rs` lists
  archives recursively (max 3 deep, >256 MiB or anything nested in a RAR spills to a temp file); `extract.rs` re-opens a
  file by its `chain` of archive members; `discover.rs` walks a root and emits one source per archive plus one per
  top-level folder with loose files; `extract_many_entries` reads many files from one archive in a single pass.
- `crates/stl-thumb`: thumbnails. `cache.rs` (cache names), `image.rs` (scale within 256×256 + JPEG q80), `mesh.rs` (STL/OBJ),
  `raster.rs` (CPU z-buffer renderer), `generate.rs` (one archive pass per batch), `queue.rs` (two-level priority queue
  with replace semantics). Render one file by hand: `cargo run --release -p stl-thumb --example thumb -- <in> <out.png>`.
- `crates/stl-drag`: Windows-only OLE drag-out.
  - `data.rs` is the `IDataObject`: file descriptors, file contents written on request, async copy.
  - `stream.rs` is an `IStream` over a delete-on-close temp file, and `source.rs` is the drop source.
  - `prepare` creates the object in the MTA. `run_drag` runs `DoDragDrop` on the STA with a marshaled proxy, so
    Explorer's calls never block the UI thread.
  - `tests/drop.rs` drops on the shell's own drop target for a temp folder, so no mouse is needed.
- `crates/stl-index`: `~/.stl-browser/index.db` (SQLite, WAL). `db.rs` holds schema + migrations; `scanstore.rs` holds
  `add_root` / `scan_root` (incremental rescan by size, mtime and folder file-name fingerprint; errored sources retried;
  vanished sources deleted; a root inside another root is skipped); `resolvestore.rs` holds `index_all` (scan, then
  re-resolve changed collections per (root, collection)); `views.rs` loads rows and builds both views (inner roots
  become alias volumes; heroes and posters get their entry); `thumbstore.rs` sorts thumbnail requests into ready,
  failed and jobs; `export.rs` locates an export plan's files and writes them to a folder (Extract to…); `parity.rs` backs `stlx parity`. `bin/stlx.rs` is the dev CLI.
- `app/src-tauri`: Tauri shell (`lib.rs` setup and the `thumb` protocol, `state.rs` with the cached view trees, the
  index busy flag and the thumbnail service, `commands.rs`, `browse.rs` for `list_folder` and carousel slots,
  `indexer.rs` for background index runs, `thumbs.rs` for thumbnail requests, workers and `thumb-ready`, `export.rs` for `begin_drag` and `extract_to`).
  `app/src/routes/+page.svelte` plus `app/src/lib/` (components, and pure `grid`/`nav`/`items`/`indexrun`/`carousel`
  modules with tests):
  SvelteKit UI with the static adapter.
- The Python prototype (the resolver's porting reference) was deleted after parity held in milestone 3. It is still in
  git history, and its tests live on as `crates/stl-core/tests/{names,resolve,vtree_models}.rs`.

DB tables split into **cached listings** (`roots`, `sources`, `entries`; rebuildable) and **user data** (`settings`,
`selected_thumb`, `tags`, `node_tags`; never rebuilt, keyed by stable path strings, never row ids). Resolved tables
(`groups`, `models`, `model_files`, `assets`) are rebuilt per collection and must never hold user data. `thumbs` is
bookkeeping for the regenerable `~/.stl-browser/thumbs/` cache; its rows cascade with their entries.

## Rules and gotchas

- **Schema changes go through `MIGRATIONS` in `crates/stl-index/src/db.rs`**: append a step, bump `SCHEMA_VERSION`,
  update `crates/stl-index/tests/db.rs`.
- **Commitizen owns the version.** Every push to `main` bumps it from the conventional commits, updates `CHANGELOG.md`
  and publishes a release (`.github/workflows/release.yml`, `.cz.toml`). Don't bump versions or edit `CHANGELOG.md` by
  hand, keep commit messages conventional, and don't pin a `version` on internal path dependencies.
  `uvx --from commitizen cz bump --dry-run` shows the next bump.
- **Commit types: only `feat!`, `feat`, `fix` and `chore`.** Anything that shouldn't change the version is a `chore`
  with a scope, e.g. `chore(docs): ...` for documentation, `chore(ci): ...` for the pipeline. No `docs:`, `ci:`,
  `refactor:`, `test:` etc.
- **Release profile uses `panic = "unwind"` on purpose** so archive-parser panics become source errors. Don't switch to
  `abort`.
- **Password-protected archives are never opened or decrypted**: detect from headers, store `locked = 1` with no
  entries.
- **`unrar_sys` doesn't link `advapi32`**; `crates/stl-scan/src/lib.rs` links it explicitly.
- **unrar can't write to a still-open file**: close the handle first (`NamedTempFile::into_temp_path`), or nested
  archives inside RARs silently fail.
- **No RAR fixtures** (can't create RARs without WinRAR). Verify RAR paths against a real collection with a RAR
  holding many nested zips. Where that data lives on this machine goes in the git-ignored `CLAUDE.local.md`.
- **No real names in the repo.** Tests and docs use invented collection, studio and model names; keep it that way
  (no franchise, studio or product names, no machine-specific paths).
- **Leave `.3d-model-library\` folders alone**: they belong to another tool. Discovery skips dot-folders.
- **Shell heredocs here mangle backslashes.** Write source files with the editor tools.
- **Don't kill processes by image name** (e.g. `taskkill /IM node.exe`); find a dev server by its port.

## Workflow

**Never commit directly to `main`** unless explicitly asked. To start a new feature, branch from a fresh `main`:
`git checkout main && git pull` first, then `git checkout -b <feature-branch>`. Land work through a pull request.

Milestones are planned with `superpowers:writing-plans` into `docs/superpowers/plans/` and executed inline with TDD per
step, a ledger in `.superpowers/sdd/<plan>/`, and one fresh reviewer at the end. Keep `docs/index.md` (status,
decisions, deferred items) up to date as work lands.
