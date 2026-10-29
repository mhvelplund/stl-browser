# Skeleton + Scan (Milestones 1–2) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** A Tauri + Svelte app that starts, creates `~/.stl-browser/index.db`, and a Rust scanner that discovers
sources under a root, lists ZIP/7Z/RAR natively (nested up to depth 3), extracts any entry by its chain, and stores
listings incrementally in SQLite.

**Architecture:** A Cargo workspace. `stl-core` holds pure name helpers. `stl-scan` holds archive reading, discovery
and extraction. `stl-index` holds the app dir, the DB and incremental scan storage. `app/src-tauri` holds the Tauri
shell. A dev CLI (`stl-index` bin `stlx`) runs scans on real data before the UI exists.

**Tech Stack:** Rust 1.99 (MSVC), Tauri 2, Svelte 5 + TypeScript + Vite, rusqlite (bundled), zip, sevenz-rust2,
unrar, blake3, tempfile, thiserror, serde/serde_json.

**Spec:** `docs/superpowers/specs/2026-10-27-stl-browser-design.md`. The prototype reference is
`prototype/stlindex/scan.py`, `db.py`, `names.py`.

## Global Constraints

- No runtime dependency on Python, 7-Zip or any external executable. Nothing in `prototype/` is built, imported or
  shipped.
- App data dir: `~/.stl-browser/` (`dirs::home_dir()`). Tests override it with the env var `STL_BROWSER_HOME`.
- DB file: `<app dir>/index.db`, WAL mode, `PRAGMA foreign_keys = ON`.
- Max archive nesting depth: 3. Nested archives ≤ 256 MiB (`268_435_456` bytes) are read in memory. Larger ones are
  spilled to a `tempfile`. RAR is always read from a file on disk.
- Archive extensions: `.zip`, `.rar`, `.7z`. Secondary RAR volumes (`.partN.rar`, N ≠ 1) are skipped.
- Entry `path` is `/`-separated, with nested archives as path segments (`a/b.zip/c.stl`). `chain` is the list of raw
  member names, outermost first.
- `crc` is 8 uppercase hex characters, or `""` when the format gives none.
- Dot-directories (e.g. `.3d-model-library`) are never descended into.
- An archive must never block on a password prompt. Encrypted or corrupt sources record `sources.error`, and the scan
  continues.

## Review Focus

1. **Unicode paths** (`03. ОРКИ`, member names with Cyrillic or accents) must round-trip through listing, the DB and
   extraction. *Test in Task 5 and Task 8.*
2. **A corrupt or encrypted archive** in a root must not abort the scan. It gets `sources.error` and the other
   sources still index. *Test in Task 9.*
3. **Backslash member names** (Windows-made zips) are normalised to `/` in `path`, but the raw name stays in `chain`
   so extraction still works. *Test in Task 5.*
4. **A nested archive over 256 MiB** spills to disk instead of loading into RAM. *Test in Task 6, using a lowered
   threshold parameter.*
5. **Re-running a scan** on an unchanged root re-lists nothing, and a deleted archive's entries are removed. *Test in
   Task 9.*

---

### Task 1: Workspace and app scaffold

**Files:**
- Create: `Cargo.toml` (workspace: `crates/*`, `app/src-tauri`), `.gitignore` (`target/`, `node_modules/`,
  `app/dist/`)
- Create: `app/` via `create-tauri-app` with the Svelte + TypeScript template (npm). Product name `STL Browser`,
  identifier `dk.hvelplund.stlbrowser`.
- Create: empty lib crates `crates/stl-core`, `crates/stl-scan`, `crates/stl-index` (edition 2024).

- [ ] **Step 1:** Scaffold. Set the window title to `STL Browser` and the size to 1280×800.
- [ ] **Step 2:** Run `cargo build --workspace` and `npm --prefix app run build`. Expected: both succeed.
- [ ] **Step 3:** Run `npm --prefix app run tauri dev`. Expected: the window opens showing the template page. Close
  it.
- [ ] **Step 4:** Commit: `chore: cargo workspace and tauri/svelte scaffold`.

### Task 2: App dir, DB schema, settings (`stl-index`)

**Files:**
- Create: `crates/stl-index/src/{lib.rs, paths.rs, db.rs}`
- Test: `crates/stl-index/tests/db.rs`

**Interfaces:**
- Produces:
  - `paths::app_dir() -> PathBuf` (uses `STL_BROWSER_HOME`, else `~/.stl-browser`; creates the dir);
    `paths::db_path() -> PathBuf`
  - `db::open(path: &Path) -> rusqlite::Result<Connection>` (applies migrations); `db::SCHEMA_VERSION: i32 = 1`
  - `db::get_setting(&Connection, &str) -> Result<Option<String>>`; `db::set_setting(&Connection, &str, &str)`

Schema v1 (all tables are created now, so later milestones only add columns or tables):
`roots(id PK, path TEXT UNIQUE NOT NULL, name TEXT NOT NULL, added_at TEXT)`;
`sources(id PK, root_id INTEGER NOT NULL REFERENCES roots(id) ON DELETE CASCADE, kind TEXT NOT NULL, path TEXT UNIQUE
NOT NULL, rel_path TEXT, name TEXT, collection TEXT, coll_path TEXT, size INTEGER, mtime REAL, scanned_at TEXT, error
TEXT)`;
`entries(id PK, source_id … ON DELETE CASCADE, path TEXT NOT NULL, chain TEXT NOT NULL, size INTEGER, crc TEXT)` with
an index on `source_id`;
`settings(key PK, value)`; `selected_thumb(node_key PK, image_key TEXT NOT NULL)`; `tags(id PK, name UNIQUE)`;
`node_tags(node_key, tag_id REFERENCES tags ON DELETE CASCADE, PRIMARY KEY(node_key, tag_id))`.

Migrations: run step functions while `PRAGMA user_version` < `SCHEMA_VERSION`.

- [ ] **Step 1: Failing tests**
  - `open_creates_schema`: after `open(tmp/index.db)`, `user_version == 1` and all 7 tables exist in
    `sqlite_master`.
  - `open_is_idempotent`: opening twice is fine, and a setting written on the first open survives the second.
  - `settings_roundtrip`: `get_setting("x") == None`; after `set_setting("x","1")` and `set_setting("x","2")`, the
    value is `Some("2")`.
  - `app_dir_respects_env`: with `STL_BROWSER_HOME=<tmp>/h`, `app_dir()` equals that path and the dir exists.
  - `deleting_root_cascades`: a root with one source and one entry. Deleting the root leaves 0 sources and 0 entries.
- [ ] **Step 2:** Run `cargo test -p stl-index`. Expected: FAIL (unresolved items).
- [ ] **Step 3:** Implement. Use rusqlite with the `bundled` feature.
- [ ] **Step 4:** Run `cargo test -p stl-index`. Expected: PASS.
- [ ] **Step 5:** Commit: `feat(index): app dir, sqlite schema v1, settings`.

### Task 3: Open the DB at app start

**Files:**
- Modify: `app/src-tauri/src/lib.rs`
- Create: `app/src-tauri/src/{state.rs, commands.rs}`
- Modify: `app/src/App.svelte` (replace the template with a status bar)

**Interfaces:**
- Consumes: `stl_index::paths::*`, `stl_index::db::open`
- Produces: `AppState { db: Mutex<Connection>, app_dir: PathBuf }` managed by Tauri; command
  `app_info() -> AppInfo { app_dir: String, db_path: String, schema_version: i32 }` (serde, camelCase).

- [ ] **Step 1:** Write a failing unit test `app_info_reports_schema` in `commands.rs`. It builds `AppState` on a temp
  dir and calls the inner fn `app_info_impl(&state)`. Assert `schema_version == 1` and that `db_path` ends with
  `index.db`.
- [ ] **Step 2:** Run `cargo test -p app`. Expected: FAIL.
- [ ] **Step 3:** Implement. Open the DB in `setup`. If it fails to open, show a native error dialog and exit. The
  Svelte status bar shows `Index: <db_path>`.
- [ ] **Step 4:** Run `cargo test -p app`. Expected: PASS. Then `npm --prefix app run tauri dev`: the status bar shows
  the path, and `~/.stl-browser/index.db` exists.
- [ ] **Step 5:** Commit: `feat(app): open index db on startup`.

### Task 4: File kinds (`stl-core::names`, partial port)

**Files:**
- Create: `crates/stl-core/src/{lib.rs, names.rs}`
- Test: in-module `#[cfg(test)]`

**Interfaces:**
- Produces: `enum Kind { Model, Scene, Image, Companion, Doc, Meta, Archive, Junk, Other }`;
  `kind_of(name: &str) -> Kind`; `ext(name) -> String` (lowercase, with dot); `stem(name) -> &str`;
  `normalize(name) -> String`; `archive_stem(file_name) -> String`; `is_secondary_rar_volume(file_name) -> bool`.
  Extension sets, `JUNK_NAMES` and `META_STEMS` are copied exactly from `prototype/stlindex/names.py:5-14`. Milestone
  3 extends this file.

- [ ] **Step 1: Failing tests**
  - `kind_of`: `"a.STL"`→Model, `"x.lys"`→Scene, `"p.JPEG"`→Image, `"m.mtl"`→Companion, `"guide.pdf"`→Doc,
    `"README.txt"`→Meta, `"attribution_card.html"`→Meta, `"notes.txt"`→Doc, `"k.7z"`→Archive,
    `"Desktop.ini"`→Junk, `"f.obv"`→Other.
  - `normalize("Foo_Bar-Baz  (1)") == "foo bar baz 1"`.
  - `archive_stem("Kit.part01.rar") == "Kit"`; `is_secondary_rar_volume("Kit.part2.rar")` is true;
    `("Kit.part1.rar")` is false; `("Kit.rar")` is false.
- [ ] **Step 2:** Run `cargo test -p stl-core`. Expected: FAIL.
- [ ] **Step 3:** Implement. Use the `regex` crate. `normalize` follows Python `[\W_]+` with Unicode semantics.
- [ ] **Step 4:** Run `cargo test -p stl-core`. Expected: PASS.
- [ ] **Step 5:** Commit: `feat(core): file kind and name helpers`.

### Task 5: Archive readers for ZIP and 7Z (`stl-scan::archive`)

**Files:**
- Create: `crates/stl-scan/src/{lib.rs, archive.rs, error.rs}`
- Create: `crates/stl-scan/tests/common/mod.rs` (fixture builders that write zip/7z into a tempdir at test time)
- Test: `crates/stl-scan/tests/archive.rs`

**Interfaces:**
- Produces:
  - `enum ArchiveInput { Path(PathBuf), Bytes(Arc<Vec<u8>>, String /* name for ext */) }`
  - `struct Member { raw_name: String, path: String /* '/'-normalised */, size: u64, crc: String }` (directories are
    excluded)
  - `trait Archive { fn list(&mut self) -> Result<Vec<Member>, ScanError>; fn extract(&mut self, raw_name: &str, out:
    &mut dyn Write) -> Result<(), ScanError>; fn extract_many(&mut self, raw_names: &HashSet<String>, f: &mut dyn
    FnMut(&str, &[u8]) -> Result<(), ScanError>) -> Result<(), ScanError>; }`. `extract_many` does one sequential pass,
    for solid archives. Thumbnails need it later.
  - `open(input: ArchiveInput) -> Result<Box<dyn Archive>, ScanError>`, dispatched on the lowercase extension.
  - `enum ScanError { Io, Format(String), Encrypted, Unsupported(String) }` (thiserror)
- 7z: use the `sevenz-rust2` crate. Pass an empty password, and map a password error to `Encrypted`.

- [ ] **Step 1: Failing tests** (fixtures built by `common::zip(&[(name, bytes)])` / `common::sevenz(...)`)
  - `zip_lists_files_not_dirs`: a zip with `Kit/`, `Kit/a.stl` (5 bytes), `Kit/b.jpg`. `list()` returns exactly 2
    members, with `path == "Kit/a.stl"`, `size == 5`, and `crc` 8 uppercase hex chars equal to `crc32fast` of the
    bytes.
  - `zip_backslash_names`: raw name `Kit\\a.stl` gives `path == "Kit/a.stl"` and `raw_name == "Kit\\a.stl"`, and
    `extract(raw_name)` returns the original bytes.
  - `zip_unicode_names`: `ОРКИ/бой.stl` round-trips through list and extract.
  - `sevenz_list_and_extract`: same as the first test, for 7z.
  - `bytes_input_equals_path_input`: listing a zip from `Bytes` equals listing it from `Path`.
  - `extract_many_single_pass`: a 7z with 3 files. Asking for 2 calls the callback exactly twice, with the right
    bytes.
  - `corrupt_archive_is_format_error`: random bytes named `x.zip` give `Err(ScanError::Format(_))`, not a panic.
- [ ] **Step 2:** Run `cargo test -p stl-scan --test archive`. Expected: FAIL.
- [ ] **Step 3:** Implement `archive.rs` with `ZipArchive` and `SevenZArchive`.
- [ ] **Step 4:** Run `cargo test -p stl-scan --test archive`. Expected: PASS.
- [ ] **Step 5:** Commit: `feat(scan): zip and 7z readers`.

### Task 6: RAR reader and nested listing

**Files:**
- Modify: `crates/stl-scan/src/archive.rs` (add `RarArchive`)
- Create: `crates/stl-scan/src/nested.rs`
- Test: `crates/stl-scan/tests/nested.rs`; `crates/stl-scan/tests/rar.rs`

**Interfaces:**
- Consumes: Task 5 `open`, `Member`, `ScanError`.
- Produces:
  - `struct ScannedEntry { path: String, chain: Vec<String>, size: u64, crc: String }`
  - `struct ListOptions { max_depth: usize /* 3 */, mem_limit: u64 /* 256 MiB */ }` with `Default`
  - `list_archive(path: &Path, opts: &ListOptions) -> Result<(Vec<ScannedEntry>, Vec<String> /* warnings */),
    ScanError>`. `Err` only when the outer archive fails. Nested failures become warnings.
- Nesting: the nested archive member itself stays in the output (kind Archive, as the prototype does), plus its
  children as `"<member path>/<child path>"` with `chain = outer.chain + child.chain`. Recurse while `depth <
  max_depth`. A nested failure adds the warning `"nested <path>: <err>"` and continues. A nested RAR, or any nested
  archive over `mem_limit`, is written to a `tempfile::NamedTempFile` (suffix = its extension) and opened by path.
- RAR: use the `unrar` crate. Encrypted headers or entries map to `Encrypted`. Never prompt (pass no password). `crc`
  comes from the file header, formatted `{:08X}`.

- [ ] **Step 1: Failing tests (`nested.rs`)**
  - `zip_in_7z_in_zip`: outer `o.zip` holds `Pack/inner.7z`, which holds `Model/deep.zip`, which holds `part.stl`.
    The entries include `Pack/inner.7z/Model/deep.zip/part.stl` with `chain == ["Pack/inner.7z", "Model/deep.zip",
    "part.stl"]`.
  - `depth_limit`: the same fixture with `max_depth: 1` has no entry containing `deep.zip/`.
  - `corrupt_nested_warns`: `o.zip` holds a garbage `bad.7z` and a good `a.stl`. The result is `Ok`. Entries include
    `a.stl` and `bad.7z`, and there is exactly 1 warning, starting with `nested bad.7z`.
  - `spills_above_mem_limit`: with `mem_limit: 10`, the nested fixture lists the same entries as with the default.
    Check through a test-only counter `nested::SPILL_COUNT` (an `AtomicUsize`, behind `#[cfg(test)]` or a `testing`
    feature) that is > 0.
- [ ] **Step 2: Failing tests (`rar.rs`)**. RAR files can't be created without WinRAR, so these tests read
  `STL_BROWSER_RAR_FIXTURE` (the path to any `.rar`) and are `#[ignore]` by default:
  - `rar_lists_and_extracts`: `list()` is non-empty. For the first member, `extract` gives `member.size` bytes whose
    CRC32 equals `member.crc`.
- [ ] **Step 3:** Run `cargo test -p stl-scan`. Expected: FAIL.
- [ ] **Step 4:** Implement `RarArchive` and `nested.rs`.
- [ ] **Step 5:** Run `cargo test -p stl-scan`. Expected: PASS. Then run `$env:STL_BROWSER_RAR_FIXTURE="D:\STL\Warhammer
  40K Collection Vol.1\<any>.rar"; cargo test -p stl-scan --test rar -- --ignored`. Expected: PASS.
- [ ] **Step 6:** Commit: `feat(scan): rar reader and nested archive listing`.

### Task 7: Extract an entry by chain

**Files:**
- Create: `crates/stl-scan/src/extract.rs`
- Test: `crates/stl-scan/tests/extract.rs`

**Interfaces:**
- Consumes: Task 5 `open`; Task 6 spill logic (reuse the helper `nested::open_member(parent: &mut dyn Archive,
  raw_name, opts) -> Result<(Box<dyn Archive>, Option<NamedTempFile>), ScanError>`, made `pub(crate)` in Task 6).
- Produces: `enum SourceKind { Archive, Folder }`; `extract_entry(kind: SourceKind, source_path: &Path, chain:
  &[String], out: &mut dyn Write) -> Result<u64 /* bytes */, ScanError>`. For a folder, `chain[0]` is the path relative
  to `source_path`.

- [ ] **Step 1: Failing tests**
  - `extract_deep_chain_byte_identical`: using the `zip_in_7z_in_zip` fixture, extracting `part.stl` gives exactly the
    original bytes.
  - `extract_from_folder`: in a tempdir with `sub/x.stl`, `extract_entry(Folder, dir, ["sub/x.stl"])` gives its bytes.
  - `missing_member_errors`: a chain with an unknown name gives `Err`, not a panic or empty output.
- [ ] **Step 2:** Run `cargo test -p stl-scan --test extract`. Expected: FAIL.
- [ ] **Step 3:** Implement.
- [ ] **Step 4:** Run `cargo test -p stl-scan --test extract`. Expected: PASS.
- [ ] **Step 5:** Commit: `feat(scan): extract entry by nested chain`.

### Task 8: Discover sources under a root

**Files:**
- Create: `crates/stl-scan/src/discover.rs`
- Test: `crates/stl-scan/tests/discover.rs`

**Interfaces:**
- Consumes: `stl_core::names::{kind_of, archive_stem, is_secondary_rar_volume}`
- Produces: `struct SourceInfo { kind: SourceKind, path: PathBuf, rel_path: String, name: String, collection: String,
  coll_path: String, size: u64, mtime: f64, loose_files: Vec<String> }`; `discover(root: &Path) ->
  io::Result<Vec<SourceInfo>>`. This is a faithful port of `prototype/stlindex/scan.py:89-136`: the volume is the root
  folder name, `collection = "<vol>/<top>"` (or `<vol>`), there is one folder source per top-level dir with loose
  non-archive, non-junk files, and the output is sorted.

- [ ] **Step 1: Failing tests** (tempdir root `Vol`)
  - `archives_and_loose`: files `A/x.zip`, `A/img.jpg`, `B/sub/y.7z`, `top.stl`. Sources: archive `A/x.zip`
    (collection `Vol/A`, coll_path `x.zip`, name `x`); archive `B/sub/y.7z` (collection `Vol/B`, coll_path
    `sub/y.7z`); folder `A` (loose `["img.jpg"]`); folder `Vol` (collection `Vol`, loose `["top.stl"]`).
  - `skips_dot_dirs_and_junk`: `.3d-model-library/z.zip` and `A/desktop.ini` produce nothing.
  - `multipart_rar`: `A/k.part1.rar` and `A/k.part2.rar` give one source named `k`.
  - `unicode_dirs`: `03. ОРКИ/a.zip` gives collection `Vol/03. ОРКИ`.
- [ ] **Step 2:** Run `cargo test -p stl-scan --test discover`. Expected: FAIL.
- [ ] **Step 3:** Implement with `walkdir` (sorted, with an `filter_entry` that skips dot-dirs). `mtime` is seconds
  since the epoch as `f64`.
- [ ] **Step 4:** Run `cargo test -p stl-scan --test discover`. Expected: PASS.
- [ ] **Step 5:** Commit: `feat(scan): discover sources under a root`.

### Task 9: Incremental root scan into the DB, plus dev CLI

**Files:**
- Create: `crates/stl-index/src/scanstore.rs`; `crates/stl-index/src/bin/stlx.rs`
- Test: `crates/stl-index/tests/scanstore.rs`

**Interfaces:**
- Consumes: `stl_scan::{discover, list_archive, ListOptions, SourceInfo, ScannedEntry}`; Task 2 `db`.
- Produces:
  - `add_root(&Connection, path: &Path) -> Result<i64>` (name = the folder name; idempotent on path)
  - `struct ScanStats { sources: usize, listed: usize, reused: usize, removed: usize, errors: usize }`
  - `enum Progress<'a> { Source { index: usize, total: usize, name: &'a str }, Warning(&'a str) }`
  - `scan_root(conn: &mut Connection, root_id: i64, opts: &ListOptions, progress: &mut dyn FnMut(Progress)) ->
    Result<ScanStats>`. A source whose `size` and `mtime` match the stored row is reused. Others are re-listed, with
    entries replaced in one transaction per source. Sources of this root not seen on disk are deleted. A listing error
    stores `error` and zero entries, and counts in `errors`. If the root dir is missing, return `Err(RootMissing)`
    without deleting anything.
  - `stlx` CLI: `stlx add <dir>`, `stlx scan` (all roots, prints progress and stats), `stlx extract <entry_id> <out>`.
    It uses `paths::db_path()`.

- [ ] **Step 1: Failing tests** (`STL_BROWSER_HOME` = tempdir, root built with Task 5's fixture helpers)
  - `first_scan_lists_all`: root with 2 zips and 1 loose file. Stats: `sources 3, listed 3, reused 0`. `entries`
    count equals the sum of the listings.
  - `rescan_unchanged_reuses`: the second `scan_root` gives `listed 0, reused 3`, and entry ids are unchanged.
  - `modified_archive_relisted`: rewrite one zip with an extra file (bump the mtime with `filetime`). Then `listed 1`,
    and its entry count goes up by 1.
  - `deleted_archive_removed`: delete one zip. Then `removed 1`, and its entries are gone.
  - `corrupt_archive_recorded`: a garbage `bad.zip` next to a good one. The scan is `Ok` with `errors 1`, the
    `bad.zip` row has a non-null `error`, and the good zip has entries.
  - `missing_root_is_error_and_keeps_data`: rename the root dir. `scan_root` is `Err`, and the source count is
    unchanged.
- [ ] **Step 2:** Run `cargo test -p stl-index --test scanstore`. Expected: FAIL.
- [ ] **Step 3:** Implement `scanstore.rs` and `stlx`.
- [ ] **Step 4:** Run `cargo test --workspace`. Expected: PASS.
- [ ] **Step 5: Real-data check.** Run `cargo run --release -p stl-index --bin stlx -- add "D:\STL\Warhammer 40K
  Collection Vol.1"`, then `stlx scan`. Expected: 45 archive sources, about 18,050 entries (prototype: 18,050 incl.
  1,243 nested) and 0 errors. The second `stlx scan` prints `listed 0`. Then `stlx extract <id of a nested-chain
  entry> out.stl` writes a file of the listed size.
- [ ] **Step 6:** Commit: `feat(index): incremental root scan and stlx dev cli`.
