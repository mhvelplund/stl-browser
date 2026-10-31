# Thumbnails (milestone 5) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Tiles show real thumbnails: images are scaled, STL/OBJ files are rendered on the CPU, folders and models get
a carousel (slot 0 = a 2×2 composite of child folders, then nested images) whose chosen slot is saved, all generated in
the background and cached on disk.

**Architecture:** A new `stl-thumb` crate holds the pure-ish parts: cache naming, image scaling, mesh loading, a z-buffer
rasterizer, batch generation (one archive pass per batch via a new `stl_scan::extract::extract_many_entries`) and a
two-level priority queue with worker threads. `stl-index` gets schema v5 (`thumbs`) and a `thumbstore` (job rows,
cache hits, results). The app computes carousel slots in `browse::list` (from the tree plus `selected_thumb`), serves
`thumb://` files, and exposes `request_thumbs` / `select_thumb`; workers emit `thumb-ready`. The UI asks for the
thumbnails of visible tiles first and the rest of the folder second.

**Tech Stack:** Rust: `image` (jpeg/png/webp/gif/bmp decode, lossless WebP encode), `stl_io`, `tobj`, `blake3`,
std threads; Tauri 2 custom URI scheme; Svelte 5 (`SvelteMap`).

**Spec:** `docs/superpowers/specs/2026-10-27-stl-browser-design.md` (Decisions "My calls", Data, Thumbnails & carousel,
Verification) and `docs/index.md` (decisions on thumbnails and user-data keys).

## Global Constraints

- Cache: `~/.stl-browser/thumbs/<location>/<content>.webp`. `<location>` = first 16 hex chars of
  blake3(`source path` + `\0` + chain joined with `\0`). `<content>`: archive entries with a stored CRC use
  `<crc lowercase>-<size>` (no extraction needed to check the cache); loose files and entries without a CRC use the
  first 32 hex chars of blake3 of their bytes.
- Files are `.webp`, 128×128, aspect kept, padded transparent (`THUMB_SIZE = 128`).
- STL/OBJ: CPU rasterizer, z-buffer, fixed 3/4 isometric camera fit to the bounding box, flat grey lambert shading,
  no GPU, runs on worker threads.
- Thumbnail sources: images `jpg jpeg png webp gif bmp`; meshes `stl obj`. Anything else has no thumbnail and keeps its
  `KindIcon`.
- Jobs grouped by source archive (same outer archive chain), so a solid RAR/7Z is decompressed once per batch.
- Workers: `max(1, cores - 1)` threads. A missing cache file is regenerated. A failed thumbnail is recorded and not
  retried until its entry changes (entries cascade to `thumbs`).
- Carousel: slot 0 = composite (CSS 2×2 of four child folders' selected thumbnails; never stored as a file), slots 1..n
  = nested images (≤ 11, rank/tree order), or nested meshes (≤ 6) when there are no images. ‹ › arrows on hover, a
  `k / n` counter. The chosen slot is saved in `selected_thumb(node_key, image_key)`; `image_key` is the entry's
  `f:<source path>|<entry path>` key or `composite`. Default: the resolver hero image if there is one, else the
  composite, else the first image.
- Model tiles use the same carousel. Image and mesh tiles show their own thumbnail.
- Served to the webview via the `thumb` custom protocol (`http://thumb.localhost/<rel>` on Windows); only paths
  matching `<16 hex>/<name>.webp` are served.
- Status bar shows the thumbnail queue length.
- Size caps: images ≤ 64 MiB, meshes ≤ 256 MiB; bigger files get an error row ("too large").
- Schema changes go through `MIGRATIONS` (v5) and `crates/stl-index/tests/db.rs`.
- Tests: `cargo test --workspace`, `npm --prefix app test`, `npm --prefix app run check` pass at every commit.

## Review Focus

1. **Corrupt or unsupported image / mesh inside an archive** (truncated JPEG, ASCII STL with junk, OBJ with no faces):
   the job records an error, the tile keeps its icon, and the other jobs in the batch still finish. → Task 4
   `bad_file_does_not_stop_the_batch`.
2. **Cache folder deleted while the app runs** (or one `.webp` removed): the next request regenerates it instead of
   serving a dead URL. → Task 6 `missing_file_is_not_a_hit`.
3. **The user navigates away mid-generation**: queued jobs for the old folder are dropped when a new request replaces
   them, so the new folder isn't stuck behind thousands of stale jobs. → Task 5 `replace_drops_stale_jobs`.
4. **A saved selection that no longer exists** (image removed in a rescan, or a composite that has no cells any more):
   the tile falls back to its default slot. → Task 7 `stale_selection_falls_back_to_default`.
5. **Path traversal through the `thumb` protocol** (`../index.db`, absolute paths, backslashes): refused. → Task 2
   `cache_rel_is_validated`.

Also pinned: a degenerate mesh (all triangles flat or one point) renders without panicking (Task 3
`degenerate_mesh_renders`).

---

### Task 1: Schema v5 `thumbs`

**Files:**
- Modify: `crates/stl-index/src/db.rs` (V5, `SCHEMA_VERSION = 5`), `crates/stl-index/tests/db.rs`

**Interfaces:**
- Produces: table `thumbs(entry_id INTEGER PRIMARY KEY REFERENCES entries(id) ON DELETE CASCADE, cache_rel TEXT,
  status TEXT NOT NULL, error TEXT)`; `status` is `ok` or `error`.

- [ ] **Step 1: Write the failing test** in `tests/db.rs`: `thumbs_rows_cascade_with_entries` — insert a root, a
  source, an entry, a `thumbs` row; delete the entry; `count(thumbs) == 0`. Update the existing version assertion to 5.
- [ ] **Step 2: Run** `cargo test -p stl-index --test db` → FAIL (no table `thumbs`).
- [ ] **Step 3: Implement** V5 and bump the version.
- [ ] **Step 4: Run** `cargo test -p stl-index` → PASS.
- [ ] **Step 5: Commit** `feat(db): schema v5 thumbs table`.

---

### Task 2: `stl-thumb` crate: cache names and image thumbnails

**Files:**
- Create: `crates/stl-thumb/Cargo.toml` (deps: `image` with default features off and `jpeg png webp gif bmp`, `blake3`;
  later tasks add `stl_io`, `tobj`, `stl-scan`), `crates/stl-thumb/src/lib.rs`, `src/cache.rs`, `src/image.rs`
- Test: unit tests in each module

**Interfaces:**
- Produces:
  - `pub const THUMB_SIZE: u32 = 128;`
  - `#[derive(Debug, Clone, Copy, PartialEq, Eq)] pub enum ThumbKind { Image, Mesh }`;
    `pub fn thumb_kind(name: &str) -> Option<ThumbKind>` (by lowercase extension, Global Constraints list).
  - `cache.rs`: `pub fn location_dir(source_path: &str, chain: &[String]) -> String`;
    `pub fn content_name(crc: &str, size: u64) -> Option<String>` (`None` when `crc` is empty);
    `pub fn content_hash(bytes: &[u8]) -> String`; `pub fn cache_rel(location: &str, content: &str) -> String`
    (`"<location>/<content>.webp"`); `pub fn is_valid_rel(rel: &str) -> bool`.
  - `image.rs`: `pub fn image_thumb(bytes: &[u8]) -> Result<image::RgbaImage, String>` (decode with a 512 MiB alloc
    limit, fit inside 128×128 keeping aspect, centre on a transparent 128×128 canvas);
    `pub fn encode_webp(img: &image::RgbaImage) -> Result<Vec<u8>, String>` (lossless).

- [ ] **Step 1: Write the failing tests**

```rust
// cache.rs
#[test]
fn names_are_stable_and_shaped() {
    let a = location_dir("D:\\V\\G.rar", &["Ghamak/842.zip".into(), "S/body.stl".into()]);
    assert_eq!(a.len(), 16);
    assert!(a.chars().all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase()));
    assert_eq!(a, location_dir("D:\\V\\G.rar", &["Ghamak/842.zip".into(), "S/body.stl".into()]));
    assert_ne!(a, location_dir("D:\\V\\G.rar", &["Ghamak/842.zip/S/body.stl".into()]));
    assert_eq!(content_name("1A2B3C4D", 1234).as_deref(), Some("1a2b3c4d-1234"));
    assert_eq!(content_name("", 1234), None);
    assert_eq!(content_hash(b"x").len(), 32);
    assert_eq!(cache_rel(&a, "1a2b3c4d-1234"), format!("{a}/1a2b3c4d-1234.webp"));
}

#[test]
fn cache_rel_is_validated() {
    assert!(is_valid_rel("0123456789abcdef/1a2b3c4d-1234.webp"));
    assert!(is_valid_rel("0123456789abcdef/0123456789abcdef0123456789abcdef.webp"));
    for bad in ["../index.db", "0123456789abcdef/../../x.webp", "C:\\x.webp", "/etc/x.webp",
                "0123456789abcdef\\a.webp", "0123456789abcdef/a.png", "0123456789ABCDEF/a.webp", ""] {
        assert!(!is_valid_rel(bad), "{bad}");
    }
}

// lib.rs
#[test]
fn kinds_by_extension() {
    assert_eq!(thumb_kind("A/B.JPG"), Some(ThumbKind::Image));
    assert_eq!(thumb_kind("x.stl"), Some(ThumbKind::Mesh));
    assert_eq!(thumb_kind("x.obj"), Some(ThumbKind::Mesh));
    assert_eq!(thumb_kind("x.lys"), None);
    assert_eq!(thumb_kind("x.3mf"), None);
}

// image.rs
#[test]
fn wide_image_is_fitted_and_padded() {
    let src = image::RgbaImage::from_pixel(400, 100, image::Rgba([255, 0, 0, 255]));
    let mut png = Vec::new();
    image::DynamicImage::ImageRgba8(src).write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png).unwrap();
    let t = image_thumb(&png).unwrap();
    assert_eq!(t.dimensions(), (128, 128));
    assert_eq!(t.get_pixel(64, 64)[3], 255); // centre is image
    assert_eq!(t.get_pixel(64, 2)[3], 0); // top band is padding
    let webp = encode_webp(&t).unwrap();
    assert_eq!(&webp[..4], b"RIFF");
    assert_eq!(image::load_from_memory(&webp).unwrap().dimensions(), (128, 128));
}

#[test]
fn garbage_is_an_error() {
    assert!(image_thumb(b"not an image").is_err());
}
```

`is_valid_rel`: exactly two `/`-separated parts; the first is 16 lowercase hex chars; the second is
`[0-9a-f-]+\.webp` and not empty before `.webp`. No other characters.

- [ ] **Step 2: Run** `cargo test -p stl-thumb` → compile errors (crate empty).
- [ ] **Step 3: Implement** the three modules.
- [ ] **Step 4: Run** `cargo test -p stl-thumb` → PASS.
- [ ] **Step 5: Commit** `feat(thumb): cache names and image thumbnails`.

---

### Task 3: Mesh loading and the software rasterizer

**Files:**
- Create: `crates/stl-thumb/src/mesh.rs`, `crates/stl-thumb/src/raster.rs`; add `stl_io`, `tobj` deps

**Interfaces:**
- Produces:
  - `pub type Tri = [[f32; 3]; 3];`
  - `mesh.rs`: `pub fn load_mesh(name: &str, bytes: &[u8]) -> Result<Vec<Tri>, String>`: STL (binary or ASCII, via
    `stl_io::read_stl` on a `Cursor`) or OBJ (`tobj::load_obj_buf` with `triangulate: true`, `single_index: true`, a
    material loader that returns an empty set). An empty result is an error (`"no triangles"`). Non-finite
    coordinates are dropped.
  - `raster.rs`: `pub fn render(tris: &[Tri]) -> image::RgbaImage` (128×128).
    - Rendered at 2× (256×256) and box-downsampled.
    - Z is up (STL convention).
    - View: rotate −45° around Z, then tilt by 35.264° (isometric), orthographic.
    - Projected bounds are fitted into 256 px with a 6 % margin each side, keeping aspect, centred.
    - Depth test with an `f32` z-buffer.
    - Flat shading per triangle from its geometric normal, two-sided (`|n·l|`), light = view direction rotated 25°
      up-left. Grey level = 60 + 180·(|n·l|), opaque.
    - Background transparent; alpha after downsampling = coverage.
    - A zero-extent axis (flat or point mesh) must not divide by zero.

- [ ] **Step 1: Write the failing tests**

```rust
// tests/helpers in raster.rs: cube(size) -> Vec<Tri> (12 triangles); stl_bytes(tris) via stl_io::write_stl.
#[test]
fn cube_renders_centred_and_non_empty() {
    let img = render(&cube(10.0));
    let (mut n, mut sx, mut sy) = (0u32, 0u64, 0u64);
    for (x, y, p) in img.enumerate_pixels() {
        if p[3] > 128 { n += 1; sx += x as u64; sy += y as u64; }
    }
    let cov = n as f64 / (128.0 * 128.0);
    assert!((0.2..0.9).contains(&cov), "coverage {cov}");
    let (cx, cy) = (sx as f64 / n as f64, sy as f64 / n as f64);
    assert!((cx - 64.0).abs() < 8.0 && (cy - 64.0).abs() < 8.0, "centroid {cx},{cy}");
    // three visible faces get three different grey levels
    let mut greys: Vec<u8> = img.pixels().filter(|p| p[3] == 255).map(|p| p[0]).collect();
    greys.sort(); greys.dedup();
    assert!(greys.len() >= 3);
}

#[test]
fn degenerate_mesh_renders() {
    let flat = vec![[[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]]];
    let _ = render(&flat);
    let point = vec![[[1.0, 1.0, 1.0]; 3]];
    let _ = render(&point);
    let _ = render(&[]);
}

// mesh.rs
#[test]
fn loads_binary_and_ascii_stl_and_obj() {
    assert_eq!(load_mesh("a.stl", &stl_bytes(&cube(1.0))).unwrap().len(), 12);
    let ascii = b"solid t\nfacet normal 0 0 1\nouter loop\nvertex 0 0 0\nvertex 1 0 0\nvertex 0 1 0\nendloop\nendfacet\nendsolid t\n";
    assert_eq!(load_mesh("a.STL", ascii).unwrap().len(), 1);
    let obj = b"v 0 0 0\nv 1 0 0\nv 1 1 0\nv 0 1 0\nf 1 2 3 4\n";
    assert_eq!(load_mesh("q.obj", obj).unwrap().len(), 2);
    assert!(load_mesh("e.obj", b"v 0 0 0\n").is_err());
    assert!(load_mesh("j.stl", b"\x00\x01junk").is_err());
}
```

- [ ] **Step 2: Run** `cargo test -p stl-thumb` → compile errors.
- [ ] **Step 3: Implement** `mesh.rs` and `raster.rs` (edge-function triangle fill over the bounding box of each
  projected triangle, top-left rule not required).
- [ ] **Step 4: Run** `cargo test -p stl-thumb` → PASS.
- [ ] **Step 5: Commit** `feat(thumb): STL/OBJ loading and CPU rasterizer`.

---

### Task 4: Batch generation (one archive pass per batch)

**Files:**
- Modify: `crates/stl-scan/src/extract.rs` (add `extract_many_entries`), tests in `crates/stl-scan/tests/` (the file
  that covers `extract_entry`)
- Create: `crates/stl-thumb/src/generate.rs`; add `stl-scan` dep, dev-deps `tempfile`, `zip`

**Interfaces:**
- Produces:
  - `stl_scan::extract::extract_many_entries(kind: SourceKind, source_path: &Path, outer: &[String], raw_names: &HashSet<String>, f: &mut dyn FnMut(&str, &[u8]) -> Result<(), ScanError>) -> Result<(), ScanError>`:
    opens the source, follows `outer` (the nested archive members, possibly empty) and calls
    `Archive::extract_many` on the innermost archive. For `SourceKind::Folder`, `outer` must be empty and each name is
    read from `source_path.join(name)`; a missing file is skipped (no callback).
  - `generate.rs`:
    - `#[derive(Debug, Clone, PartialEq, Eq)] pub struct Job { pub entry_id: i64, pub source_kind: SourceKind, pub source_path: String, pub chain: Vec<String>, pub size: u64, pub crc: String }`
    - `impl Job { pub fn group(&self) -> String }` = `source_path` + `\0` + `chain[..len-1]` joined with `\0`.
    - `pub type Outcome = Result<String, String>;` (`Ok(cache_rel)` / `Err(message)`)
    - `pub fn generate(thumbs_dir: &Path, jobs: &[Job], done: &mut dyn FnMut(&Job, Outcome))`: all jobs must share
      one `group()`. It extracts every job's last chain member in one `extract_many_entries` call and builds each
      thumbnail by `thumb_kind` (image → `image_thumb`; mesh → `load_mesh` + `render`). Each `.webp` is written to
      `<thumbs_dir>/<cache_rel>` (dirs created; write to `*.tmp` then rename).
      - **Content name:** `content_name(crc, size)`, else `content_hash(bytes)`. If the target file already exists,
        rendering is skipped and it reports `Ok`.
      - **Size caps:** jobs over the caps fail with `"too large"`, and their bytes are never read.
      - **Errors:** a per-job error is reported for that job only. If opening the archive fails, every job is
        reported with that error. A job whose member never came back is reported as `"not found in archive"`.
      - `done` is called exactly once per job.

- [ ] **Step 1: Write the failing tests**

```rust
// stl-scan test
#[test]
fn extract_many_entries_follows_nested_chain() {
    // outer.zip contains inner.zip which contains a.txt and b.txt
    let dir = tempfile::tempdir().unwrap();
    let inner = zip(&[("a.txt", b"A"), ("b.txt", b"B")]);
    let outer = zip(&[("d/inner.zip", &inner)]);
    let p = dir.path().join("outer.zip");
    std::fs::write(&p, outer).unwrap();
    let mut got = std::collections::BTreeMap::new();
    let want: HashSet<String> = ["a.txt".into(), "b.txt".into()].into();
    extract_many_entries(SourceKind::Archive, &p, &["d/inner.zip".into()], &want, &mut |n, b| {
        got.insert(n.to_string(), b.to_vec());
        Ok(())
    }).unwrap();
    assert_eq!(got.len(), 2);
    assert_eq!(got["a.txt"], b"A");
}

// generate.rs (fixtures: a PNG made with `image`, a binary STL from Task 3's helpers, a zip)
#[test]
fn batch_writes_thumbs_once_per_archive() { /* zip with x.png and y.stl; two jobs, same group; both Ok; files exist and decode to 128×128; a second generate() call reports Ok without rewriting (mtime unchanged) */ }

#[test]
fn bad_file_does_not_stop_the_batch() { /* zip with bad.jpg (garbage), good.png, empty.obj; good → Ok, the other two Err; done called 3 times */ }

#[test]
fn folder_jobs_hash_content() { /* loose file in a dir, crc "" → cache_rel content part is 32 hex; same bytes in a second location → same content name, different location dir */ }

#[test]
fn too_large_is_an_error_without_reading() { /* Job with size = 300 MiB, kind mesh, pointing at a missing archive → Err("too large") */ }
```

Write each of the four `generate` tests with real fixtures and exact assertions as described in the comments.

- [ ] **Step 2: Run** `cargo test -p stl-scan` and `cargo test -p stl-thumb` → compile errors.
- [ ] **Step 3: Implement** `extract_many_entries` (re-using `open_member` inside `stl-scan`) and `generate`.
- [ ] **Step 4: Run** `cargo test -p stl-scan -p stl-thumb` → PASS.
- [ ] **Step 5: Commit** `feat(thumb): batch generation with one archive pass per batch`.

---

### Task 5: The job queue

**Files:**
- Create: `crates/stl-thumb/src/queue.rs`

**Interfaces:**
- Consumes: `Job`, `Job::group` (Task 4).
- Produces:
  - `pub struct Queue` (cheap to share: `Arc` inside, `Clone`).
  - `Queue::new(threads: usize, run: Arc<dyn Fn(Vec<Job>) + Send + Sync>) -> Queue` starts `threads` workers.
  - `pub fn replace(&self, high: Vec<Job>, low: Vec<Job>)` replaces everything still queued (in-flight batches keep
    running). A job id in both lists counts as high. Duplicates are dropped, and so are ids currently in flight.
  - `pub fn pending(&self) -> usize` counts queued plus in-flight jobs.
  - Workers take the first high job, else the first low job. They then take every other queued job with the same
    `group()` from both lists, up to 64 per batch, and call `run(batch)`. A panic inside `run` is caught, so the
    worker stays alive. They wait on a `Condvar` when there is nothing to do.
  - `pub fn default_threads() -> usize` = `max(1, available_parallelism - 1)`.

- [ ] **Step 1: Write the failing tests** (jobs built by a helper `job(id, group_char)`; the `run` closure records
  batches through a channel):

```rust
#[test]
fn high_before_low_and_grouped() {
    // a gate (a Barrier or a Mutex the test holds) blocks the single worker on a first batch,
    // then the test queues low=[1a, 2b], high=[3b, 4c]; after releasing the gate the next batches
    // are [3b, 2b] (high first, same group pulled in) then [4c] then [1a].
}

#[test]
fn replace_drops_stale_jobs() {
    // worker gated on batch [1a]; replace(high=[2b..=50b], low=[]) then replace(high=[60c], low=[]);
    // after the gate opens only [60c] runs; pending() reaches 0.
}

#[test]
fn panicking_batch_keeps_the_worker() {
    // run panics for id 1 and records others; replace(high=[1a]) then replace(high=[2b]) → 2 is recorded.
}
```

- [ ] **Step 2: Run** `cargo test -p stl-thumb queue` → compile errors.
- [ ] **Step 3: Implement** `queue.rs`.
- [ ] **Step 4: Run** `cargo test -p stl-thumb` → PASS (run it 3 times to catch flakiness).
- [ ] **Step 5: Commit** `feat(thumb): priority job queue grouped by archive`.

---

### Task 6: `thumbstore` (DB side)

**Files:**
- Create: `crates/stl-index/src/thumbstore.rs` (+ `pub mod thumbstore;`), add `stl-thumb` dep to `stl-index`
- Test: `crates/stl-index/tests/thumbstore.rs`

**Interfaces:**
- Consumes: `stl_thumb::{Job, cache::*, thumb_kind}`, schema v5.
- Produces:
  - `pub fn lookup(conn: &Connection, thumbs_dir: &Path, ids: &[i64]) -> Result<Lookup, IndexError>` with
    `pub struct Lookup { pub ready: Vec<(i64, String)>, pub failed: Vec<i64>, pub jobs: Vec<Job> }`:
    - An `ok` row whose file exists goes to `ready` (`cache_rel`).
    - An `error` row goes to `failed`.
    - **CRC shortcut:** with no usable row, an archive entry with a CRC is checked at
      `cache_rel(location_dir, content_name)`. If that file exists, an `ok` row is written and the entry goes to `ready`.
    - **Everything else becomes a job:** a missing file, a stale `ok` row (the row is deleted), or an entry with no
      CRC.
    - Ids that are unknown, or whose name has no `thumb_kind`, are skipped.
  - `pub fn record(conn: &Connection, entry_id: i64, outcome: &Outcome) -> Result<(), IndexError>`: upsert; ignores an
    FK violation (the entry was re-indexed meanwhile).
  - `db::open` sets `busy_timeout(5 s)` so worker connections wait out an index write instead of failing.

- [ ] **Step 1: Write the failing tests** (fixture: `common::zip` archive with `a.png` + `b.stl` and a loose `c.png`,
  indexed with `index_all`; ids looked up by path):
  - `jobs_for_new_entries`: three jobs; the zip jobs share a `group()`; the loose job has `SourceKind::Folder`.
  - `record_then_ready`: write a dummy file at the cache path, `record(Ok(rel))` → `ready` contains `(id, rel)`.
  - `missing_file_is_not_a_hit`: `record(Ok(rel))` without the file → it comes back as a job and the row is gone.
  - `crc_shortcut_finds_existing_file`: write a file at the zip entry's CRC-named path → `ready`, row inserted.
  - `failed_is_not_retried`: `record(Err("x"))` → `failed`; after the zip is rewritten and re-indexed (new entry id)
    the new id is a job again and the old row is gone (cascade).
- [ ] **Step 2: Run** `cargo test -p stl-index --test thumbstore` → compile errors.
- [ ] **Step 3: Implement** `thumbstore.rs` and the busy timeout.
- [ ] **Step 4: Run** `cargo test -p stl-index` → PASS.
- [ ] **Step 5: Commit** `feat(index): thumbstore lookups and results`.

---

### Task 7: Carousel slots in listings, and saved selections

**Files:**
- Modify: `crates/stl-core/src/vtree/mod.rs` (`Info.hero_entry_id: Option<i64>`, `Info.hero_key: Option<String>`),
  `crates/stl-index/src/views.rs` (fill them), `app/src-tauri/src/browse.rs`, `app/src-tauri/src/commands.rs`
  (`select_thumb`), `app/src-tauri/src/lib.rs`
- Test: `crates/stl-index/tests/views.rs`, unit tests in `browse.rs`

**Interfaces:**
- Consumes: `Info.entry_id`, `Info.file_kind`, `stl_thumb::thumb_kind`.
- Produces:
  - `views.rs`: after building, for every Model node with `hero_asset_id`, and every Group/Collection node with
    `poster_asset_id`, set `hero_entry_id` / `hero_key` (`f:<source path>|<entry path>`) from that asset's entry.
  - `browse.rs`:
    - `#[derive(Serialize, Clone, Debug, PartialEq)] #[serde(rename_all = "camelCase")] pub struct Thumb { pub entry_id: i64, pub key: String }`
    - `Item` gains `composite: Vec<Thumb>` (0–4 cells; empty means there is no composite slot), `pics: Vec<Thumb>`,
      `slot: usize`. The UI's slot list is `[composite?] ++ pics`, and `slot` indexes into it.
    - `pub fn list(views: &Views, view: View, path: &[String], selected: &HashMap<String, String>) -> Listing` (the
      new parameter is `node_key → image_key`).
  - **Rules per item:**
    - **Leaf** with a `thumb_kind`: `pics = [itself]`, no composite, `slot = 0`. Other leaves get nothing.
    - **Folder-like node:**
      - `pics`: up to 11 image leaves found depth-first in tree order, deduplicated by entry. If the node has a
        `hero_entry_id` that isn't among them, it goes first. With no images, `pics` is up to 6 mesh leaves.
      - `composite`: for each folder-like child in order, the child's own displayed picture (its selected or default
        slot; if that is the composite, its first pic). Stop at 4. If at least one cell came from a child, fill the
        remaining cells from the node's own `pics` that aren't used yet. Otherwise there is no composite.
      - **Default slot:** the hero's index if there is a hero; else 0 (the composite, or the first pic).
      - **Saved selection:** `selected[key]` is `"composite"` (honoured only if a composite exists) or a pic key
        (honoured if present). Anything else falls back to the default.
  - Command `select_thumb(node_key: String, image_key: String)`: upsert into `selected_thumb`. Run it async, like the
    other commands. `list_folder_impl` loads the whole `selected_thumb` table into the map on each call.

- [ ] **Step 1: Write the failing tests**
  - `views.rs` integration: `model_hero_entry_is_set` (fixture `Kit.zip` with `Kit/Kit.jpg` + `Kit/a.stl` → the
    model node's `hero_key` ends with `|Kit/Kit.jpg`).
  - `browse.rs` (fixture `testutil::library` already has `Orc/orc.jpg`, `Orc/orc.stl`, `Elf/elf.stl`):
    - `model_with_image_defaults_to_it`: in the Models view `vol/coll`, item `orc` has `pics[0].key` ending in
      `orc.jpg`, and its `slot` points at that pic. Item `elf` has `pics` = `[elf.stl]` (meshes when there are no
      images).
    - `collection_composite_from_children`: in the Models view root → `vol` → item `coll` has a composite with cells
      from `elf` and `orc`, and `slot == 0` (no hero).
    - `stale_selection_falls_back_to_default`: `selected = {orc.key: "f:gone|x.jpg"}` → default slot;
      `{coll.key: "composite"}` → 0; `{orc.key: <orc.jpg key>}` → that index.
    - `leaf_files_show_themselves`: in the Files view `vol/coll/orc`, `orc.stl` has `pics == [itself]`.
  - `commands.rs`: `select_thumb_persists` — `select_thumb_impl` then `list_folder_impl` reflects it.
- [ ] **Step 2: Run** `cargo test -p stl-index --test views` and `cargo test -p app --lib` → FAIL/compile errors.
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run** `cargo test --workspace` → PASS.
- [ ] **Step 5: Commit** `feat(app): carousel slots in listings and saved thumbnail selection`.

---

### Task 8: Thumbnail service in the app: requests, workers, events, protocol

**Files:**
- Create: `app/src-tauri/src/thumbs.rs`
- Modify: `app/src-tauri/src/state.rs`, `app/src-tauri/src/commands.rs`, `app/src-tauri/src/lib.rs`,
  `app/src-tauri/Cargo.toml` (add `stl-thumb`)

**Interfaces:**
- Consumes: `thumbstore::{lookup, record}`, `stl_thumb::{Queue, generate, cache::is_valid_rel}`.
- Produces:
  - `pub fn thumb_url(rel: &str) -> String`: `http://thumb.localhost/<rel>` on Windows, `thumb://localhost/<rel>`
    elsewhere.
  - `ThumbReady { entry_id: i64, url: Option<String>, pending: usize }` (camelCase), emitted as `"thumb-ready"`.
  - `pub struct ThumbService { dir: PathBuf, queue: Queue }`.
    - `ThumbService::new(app_dir: &Path, notify: Arc<dyn Fn(ThumbReady) + Send + Sync>)`: each worker opens its own
      DB connection on first use. `run(batch)` calls `generate`, then `record` and `notify` for each job.
    - `pub fn request(&self, conn: &Connection, high: &[i64], low: &[i64]) -> Result<Vec<ThumbReady>, String>`: one
      `lookup` over both lists. It returns `ready` items with urls and `failed` items with `url: None`, and calls
      `queue.replace` with the jobs split back into high and low.
  - `AppState` gains `thumbs: OnceLock<ThumbService>`, initialised in `lib.rs` setup with a notifier that emits through
    the `AppHandle`.
  - Command `request_thumbs(high: Vec<i64>, low: Vec<i64>) -> Result<Vec<ThumbReady>, String>` (async).
  - Protocol: `register_asynchronous_uri_scheme_protocol("thumb", …)`.
    - The request path, without its leading `/`, must pass `is_valid_rel`. Otherwise the answer is 404.
    - Valid files are served as `image/webp` with `Cache-Control: max-age=31536000, immutable` (names are content
      hashes). A missing file gets a 404.
- [ ] **Step 1: Write the failing tests** in `thumbs.rs`:
  - `request_generates_then_serves_from_cache`. Build a `ThumbService` whose notifier sends to a channel, on
    `testutil::library` (`orc.jpg` must hold a real PNG/JPEG: extend the fixture to write a tiny PNG under that name).
    - `request(high=[orc.jpg id])` returns nothing ready.
    - A `ThumbReady` with `Some(url)` arrives within 10 s.
    - A second `request` returns it ready right away.
    - The file exists under `<home>/thumbs`.
  - `failed_thumb_is_reported_not_retried`: `orc.stl` holds the bytes `"o"`.
    - The first request queues it, and a `ThumbReady { url: None }` arrives.
    - The second request returns it immediately with `url: None`.
  - `thumb_url_shape`: on Windows `thumb_url("a/b.webp") == "http://thumb.localhost/a/b.webp"`.
- [ ] **Step 2: Run** `cargo test -p app --lib thumbs` → compile errors.
- [ ] **Step 3: Implement** the service, state, command and protocol; register `commands::request_thumbs` and
  `commands::select_thumb`.
- [ ] **Step 4: Run** `cargo test -p app --lib` → PASS; `cargo build -p app` with no warnings.
- [ ] **Step 5: Commit** `feat(app): thumbnail service, thumb-ready events and the thumb protocol`.

---

### Task 9: The carousel in the UI

**Files:**
- Create: `app/src/lib/carousel.ts`, `app/src/lib/carousel.test.ts`, `app/src/lib/TileArt.svelte`
- Modify: `app/src/lib/api.ts`, `app/src/lib/Tile.svelte`, `app/src/lib/TileGrid.svelte` (report the visible index
  range), `app/src/lib/StatusBar.svelte`, `app/src/routes/+page.svelte`

**Interfaces:**
- Consumes: `Item.composite`, `Item.pics`, `Item.slot`, `requestThumbs`, `selectThumb`, `onThumbReady`.
- Produces:
  - `api.ts`: `Thumb = { entryId: number; key: string }`; `Item` gains `composite: Thumb[]`, `pics: Thumb[]`,
    `slot: number`. Also `ThumbReady = { entryId: number; url: string | null; pending: number }`, plus
    `requestThumbs(high, low)`, `selectThumb(nodeKey, imageKey)` and `onThumbReady(cb)`.
  - `carousel.ts`:
    - `type Slot = { kind: "composite"; cells: Thumb[] } | { kind: "pic"; thumb: Thumb }`
    - `slots(item: Item): Slot[]`
    - `step(i: number, n: number, dir: 1 | -1): number` (wraps around)
    - `slotKey(s: Slot): string` (`"composite"` or the thumb key)
    - `shown(s: Slot): number[]` (the entry ids a slot needs)
    - `requests(items: Item[], chosen: Map<string, number>, visible: { first: number; last: number }): { high: number[]; low: number[] }`.
      `high` = ids shown by the items in `[first, last)` at their chosen (or default) slot. `low` = every other id in
      the items' slots. Both are deduplicated, and `high` wins over `low`.
  - `TileArt.svelte { slot: Slot | null; urls: SvelteMap<number, string | null>; kind; fileKind }`:
    - A composite renders as a 2×2 CSS grid of `<img>`s; empty cells stay blank.
    - A pic renders as one `<img>` (`object-fit: contain`).
    - An id not yet in `urls` shows a pulsing skeleton (`generating thumbnail…`, as in the mockup).
    - A `null` url, or no slot at all, shows `KindIcon`.
    - Images use `draggable="false"` and `decoding="async"`.
  - `Tile.svelte`:
    - Uses `TileArt`.
    - With more than one slot it shows ‹ › buttons on hover/focus (`aria-label` "Previous thumbnail" / "Next
      thumbnail") and a `k / n` chip top-right.
    - Clicking an arrow doesn't select or open the tile (`stopPropagation`). It calls `onSlot(item, newIndex)`.
  - `TileGrid` reports `onVisible({ first, last })` in item indices (row range × cols) whenever it changes.
  - Page:
    - Keeps `urls = new SvelteMap()` and `chosen = new Map<string, number>()` (per item key, reset per listing).
    - On each listing or visible-range change (debounced 120 ms), it calls `requestThumbs(requests(...))` and stores
      the returned urls.
    - `onThumbReady` sets `urls` and `thumbPending`.
    - `onSlot` updates `chosen`, calls `selectThumb(item.key, slotKey(slot))`, then re-requests.
    - Switching view or folder keeps `urls`: entry ids stay valid until an index run, so `index-done` clears it.
  - `StatusBar` gets `thumbPending: number`: `"Thumbnails: <n> queued"` when > 0, else `"Thumbnails up to date"`.
- [ ] **Step 1: Write the failing tests** (`carousel.test.ts`):

```ts
test("slots: composite first, then pics", () => {
  const it = item({ composite: [t(1), t(2)], pics: [t(3), t(4)], slot: 1 });
  const s = slots(it);
  assert.equal(s.length, 3);
  assert.equal(s[0].kind, "composite");
  assert.deepEqual(shown(s[1]), [3]);
  assert.equal(slotKey(s[0]), "composite");
  assert.equal(slotKey(s[2]), "k4");
});
test("step wraps", () => {
  assert.equal(step(0, 3, -1), 2);
  assert.equal(step(2, 3, 1), 0);
  assert.equal(step(0, 1, 1), 0);
});
test("requests: visible chosen slots are high, the rest low", () => {
  const a = item({ key: "a", composite: [t(1), t(2)], pics: [t(3)], slot: 0 });
  const b = item({ key: "b", pics: [t(4), t(5)], slot: 0 });
  const c = item({ key: "c", pics: [t(6)], slot: 0 });
  const r = requests([a, b, c], new Map([["b", 1]]), { first: 0, last: 2 });
  assert.deepEqual(r.high, [1, 2, 5]);
  assert.deepEqual(r.low, [3, 4, 6]);
});
```

(`item()` builds an `Item` with defaults; `t(n)` = `{ entryId: n, key: "k" + n }`.)
- [ ] **Step 2: Run** `npm --prefix app test` → FAIL (module missing).
- [ ] **Step 3: Implement** `carousel.ts`, then the components and the page.
- [ ] **Step 4: Run** `npm --prefix app test` and `npm --prefix app run check` → pass, 0 errors.
- [ ] **Step 5: Smoke-run** `tauri dev` (scratch `STL_BROWSER_HOME`, CDP as in milestone 4) on Vol.1 + Vol.2:
  - A collection fills in thumbnails.
  - Vol.2 Tinmen STC (no images) shows STL renders.
  - Cycling a carousel and restarting the app keeps the slot.
  - Deleting `thumbs/` regenerates.
  - The status bar shows the queue draining.
  - Record what could not be checked.
- [ ] **Step 6: Commit** `feat(ui): thumbnails and folder carousel`.

---

### Task 10: Docs and handoff

- [ ] Update `docs/index.md`:
  - Status: milestone 5 done.
  - Architecture: `stl-thumb`.
  - Data: schema v5 `thumbs`.
  - Decisions: queue replace semantics; the slot rules; the CRC shortcut; size caps; std threads instead of rayon.
  - Deferred: whatever the review defers.
- [ ] Update `CLAUDE.md` (the `stl-thumb` crate) and `HANDOFF.md` (next: milestone 6).
- [ ] Run all three test commands; commit `docs: milestone 5 status and decisions`.

Then one fresh reviewer on the whole branch, the fix pass, and `superpowers:finishing-a-development-branch`.
