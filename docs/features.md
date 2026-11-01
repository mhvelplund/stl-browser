# Features: how they work

How each feature behaves today, the rules behind it, and where it lives. Read [`index.md`](index.md) first for the
crate map and the data on disk. [`ui.md`](ui.md) covers the UI side, and [`known-issues.md`](known-issues.md) lists
open problems.

## Indexing

### Roots, volumes and collections

- **Add folder** registers a *root*, which becomes a top-level *volume*. Each top-level subfolder of the root is a
  *collection*: `collection = "<volume>/<top folder>"`, or just `<volume>` for files directly in the root.
  `coll_path` is the source's path relative to its collection folder.
- **The volume name** is the root folder's last path component. A drive root (`D:\`) gives `D:` (`names::volume_name`).
- **Overlapping roots:** a root inside another root is never scanned on its own (`covering_root`), so each source is
  stored once, under the outermost root. The inner root still shows up as its own volume, which aliases the content of
  its folder in the outer volume (see [`virtual-tree.md`](virtual-tree.md)).
- **Adding the same folder twice**, with a different case or a trailing `\`, returns the existing root.

### Discovery (`stl-scan::discover`)

- Walks the root in sorted order. It skips dot-folders (such as another tool's `.3d-model-library\`), junk files
  (`desktop.ini`…) and secondary RAR volumes (`.partN.rar` with N ≠ 1). `archive_stem` strips `.part01`.
- Emits one source per archive, plus one *folder source* per top-level folder that holds loose (non-archive, non-junk)
  files. Unreadable folders are skipped with a warning.

### Listing (`stl-scan::archive`, `nested`)

- ZIP, 7Z and RAR are read natively (`zip`, `sevenz-rust2`, `unrar`) behind one `Archive` trait. No 7-Zip is needed.
- **Entry format:**
  - `path` is `/`-separated, with nested archives as path segments (`a/b.zip/c.stl`).
  - `chain` lists the raw member names to open in order, outermost first.
  - `crc` is 8 uppercase hex characters, or `""` when the format gives none.
  - Backslash member names (zips made on Windows) are normalised to `/` in `path` and kept raw in `chain`, so
    extraction still finds them.
- **Nesting:** archives are opened up to 3 levels deep. A nested archive stays in the listing as its own entry, with its
  children below it. Nested archives up to 256 MiB are read in memory. Bigger ones, and every archive nested in a RAR,
  are spilled to a temp file. unrar can't read a temp file that is still open, so close the handle first
  (`NamedTempFile::into_temp_path`).
- **Failures:** if the outer archive fails, the source gets `error` and the scan continues. A nested archive that fails
  becomes a warning. A panic inside an archive library is caught (the release profile uses `panic = "unwind"`) and
  recorded the same way.
- **Password-protected archives** are detected from their headers. Nothing is ever decrypted and no format may prompt
  for a password:
  - ZIP: the encryption flag in the central directory.
  - 7Z: a password-required error on open, or an AES coder in a block.
  - RAR: encrypted headers, or an encrypted entry.

  Such an archive is stored with `locked = 1` and no entries, and it isn't retried until its size or mtime changes. A
  locked nested archive is an entry with `locked = 1`. Both show as padlock items.

### Rescans (`stl-index::scanstore`, `resolvestore`)

- **Incremental, per source:** a source with the same size, mtime and (for folder sources) file-name fingerprint keeps
  its cached listing. Anything else is re-listed, with its entries replaced in one transaction. Vanished sources are
  deleted, and a source that failed last time is always retried.
- **A missing root folder** is reported (`status` flags it `missing`) and its data is left alone, so it stays
  browsable.
- **Resolving:** after the scan, every collection with a source listed or removed is resolved again (`dirty`), plus
  any collection that was never resolved. Resolved rows of collections with no sources left are dropped. The
  resolution unit is (root, collection). See [`archive-patterns.md`](archive-patterns.md).
- **Scope:** "Rescan folder" (the toolbar button inside a folder, or right-click → Rescan on a folder tile) rescans only
  that folder. `browse::scope` takes the node's real paths, cuts each at the first `!` (a member means "that archive"),
  keeps disk paths only and drops covered ones. Discovery still walks the whole root, but only sources in scope are
  listed or deleted. A source is in scope when its path is under a scope path, or when it is a folder source whose
  folder contains the scope (except the root's own folder source). At Library everything is rescanned.
- **Stop:** the cancel flag is checked before each source. A stopped run deletes no vanished sources, because it
  hasn't seen them all. It still resolves the collections it changed, so the DB stays consistent, then skips the
  remaining roots. Listing a single big archive can't be interrupted part-way.
- **Index runs in the app** (`indexer.rs`) run on a background thread with their own SQLite connection (WAL), so
  browsing keeps working. Only one run at a time: `try_begin_index` hands out a guard that clears the busy flag on drop,
  and a second request is refused. Events: `index-progress { index, total, name }` per source, and exactly one
  `index-done` (stats, `cancelled`, warnings or error). A panic in a run is caught and reported. The worker rebuilds
  both view trees and swaps them in before it sends `index-done`.

## Views

Both trees are built in memory from the DB (`views.rs` loads rows, `stl-core::vtree` builds them) and cached in
`AppState` with per-node counts. The index worker rebuilds them after every run. Layout and naming rules are in
[`virtual-tree.md`](virtual-tree.md). `stlx tree models|files [<path>] [--depth N] [--real]` prints either view.

- **`list_folder(view, path)`** takes the path as name segments (a name may contain `/`) and lists the deepest folder
  that still exists. A path that ends on a file lists that file's folder. The returned `path` says where it ended up,
  so the UI follows when a folder vanished in a rescan.
- **Counts** (`vtree::counts`): `models` counts Model nodes, `files` counts `model`/`scene` files, `images` counts
  images. Inner-root volumes share nodes, so the tree is a DAG: walks are iterative and memoised. Folder counts are not
  filtered by `shown_extensions`.
- **Shown file endings** (`stl_core::vtree::filter::Shown`): only the endings in `~/.stl-browser/config.yaml`
  (`shown_extensions`) show, in both views, in search and in exports. Folders, archives and locked archives always
  show, and so does an inner archive that wasn't expanded (a file node named `x.zip`). Defaults when the key is
  missing: `stl obj 3mf fbx blend dae`, `lys lyt chitubox ctb photon sl1`, `jpg jpeg png gif webp bmp`,
  `pdf txt md html htm`. The file is read once at startup. A missing file is written as a commented template, and one
  that doesn't parse is ignored (defaults apply, and the status bar says why). `browse::visible` applies the filter.
  `mtl` is hidden by default, so an exported OBJ loses its materials unless `mtl` is added. Folders that hold only
  hidden files still show, as empty folders.
- **Switching views** keeps your place (`vtree/locate.rs`, the `locate` command). It goes to the deepest non-file node
  of the other view whose real path contains this one (component prefix, ignoring case, split on `\`, `/` and `!`). If
  there is none, it goes to the shallowest node below it. If there is still none, it retries with the parent folder, up
  to Library.

## Thumbnails and the carousel

### Cache (`stl-thumb`, `thumbstore.rs`)

- Files: `~/.stl-browser/thumbs/<location>/<content>.jpg`, within 256×256 (`THUMB_SIZE`), JPEG quality 80. Images keep
  their shape. Mesh renders are square on the tile grey `#2A2E34`, because JPEG has no transparency. Small images are
  scaled up.
  - `<location>` = the first 16 hex chars of blake3(source path + `\0` + chain).
  - `<content>` = `<crc>-<size>` for archive entries with a stored CRC, else blake3 of the bytes (loose files).
- **The CRC shortcut:** an archive entry's cache name is known without extracting it, so a cache hit costs nothing.
- **`thumbs` rows** (`entry_id`, `cache_rel`, `status`, `error`) cascade with their entry. An `ok` row counts only while
  its file exists, so deleting `thumbs/` regenerates everything. An `error` row isn't retried until the entry changes.
- **What gets a thumbnail:** images `jpg jpeg png webp gif bmp` (≤ 64 MiB), meshes `stl obj` (≤ 256 MiB) and scenes
  `3mf lys` (≤ 512 MiB). Bigger files get an error row ("too large") without being read. macOS AppleDouble files
  (`__MACOSX/`, `._x.stl`) get none.
- **More files next to the still** (`cache.rs`). An entry counts as cached (`is_complete`) only when they are all there.
  A still cached without them is answered at once *and* queued, and the job fills in the rest without touching it:
  - a mesh's turntable strip `<content>.spin.jpg`;
  - a 3MF's carousel pictures `<content>.p<i>.jpg`, then their count `<content>.pics` (written last, also when it is 0).
- **The renderer** (`raster.rs`) is a CPU z-buffer, so no GPU is needed. It uses an orthographic isometric camera from
  (1, −1, 1) with Z up, fitted to the projected bounds with a 6 % margin. Shading is flat, two-sided grey Lambert, with
  the light 25° up-left of the view direction. It renders at 2× and box-downsamples. Flat, point and empty meshes must
  not panic. `cargo run --release -p stl-thumb --example thumb -- [--spin|--all] <in> <out.jpg>` writes one file's
  still, its strip, or a 3MF's whole carousel.
- **The turntable strip** is `SPIN_FRAMES` (12) squares side by side (3072×256): the camera steps 30° around Z from the
  still's view, every frame with one scale and centre (the bounding box's middle), so the model turns in place. It is
  made in the same pass as the still while the triangles are in memory, because making it on hover would mean
  extracting the file again (seconds in a big solid RAR). Frames skip the 2× supersampling, yet the strip still costs
  about 8× the still (~2.5 s for a 240 MB STL). A JPEG strip rather than a GIF: same encoder and naming, no 256-colour
  banding. A strip that fails leaves the still alone.
- **Scene previews** (`preview.rs`) are the pictures the files carry, not renders:
  - **LYS** is an Electron asar container: `u32 4`, `u32` header pickle size, `u32`, `u32` JSON length, the JSON file
    table at offset 16 (`mangoFiles`, or `files`), data from 8 + pickle size. The thumbnail is its `preview.png`
    (512×512 in the real data; `offset` is a decimal string).
  - **3MF** is a ZIP. The default picture is the package thumbnail its `_rels/.rels` names (OPC type
    `…/metadata/thumbnail`), else `Metadata/thumbnail.png`, else `Metadata/plate_1.png`. Its carousel then has the
    plate renders `Metadata/plate_<n>.png` by plate number and the designer's photos `Auxiliaries/Model Pictures/*` by
    name, never the default again, at most 12 (`MAX_EXTRAS`); one that won't decode is skipped. Small copies
    (`plate_N_small`, `thumbnail_*`), unlit renders, top views, the slicer's object-picking masks (`pick_N`) and profile
    pictures are left out. A 3MF without any picture has no thumbnail; rendering its mesh XML is not built.
  - Broken or cut-off files are an error, never a panic.

### Queue and service (`queue.rs`, `app/src-tauri/src/thumbs.rs`)

- **`request_thumbs(high, low)`** does one lookup. It returns the ids already ready (with urls) and the failed ones
  (`url: null`), and replaces the queue with the rest. It **replaces rather than appends**, so a folder you left doesn't
  keep the workers busy. An id in both lists counts as high, and in-flight ids are not queued again.
- **Workers** are std threads (cores − 1). rayon doesn't fit, because the queue needs priorities, grouping and
  replace. A worker takes the first high job (else low), then every queued job from the same archive level, up to 64.
  It reads them in one archive pass (`extract_many_entries`) and renders. A panic in a batch is caught. Each job emits
  `thumb-ready { entryId, url | null, extras, pending }`; `extras` are the urls of a file's own carousel pictures (a
  3MF's), also in `request_thumbs` answers.
- **The `thumb` protocol** serves `http://thumb.localhost/<rel>` on Windows and `thumb://localhost/<rel>` on macOS
  (`thumbs.rs` `thumb_url`). `rel` must pass `cache::is_valid_rel`
  (16 lowercase hex, `/`, `[0-9a-f-]+` with an optional `.spin` or `.p<n>`, `.jpg`), otherwise the answer is 404, and
  so is a missing file. Responses are `image/jpeg` and cached as immutable, because names are content hashes.
- The UI asks for what is on screen (high) and the rest of the folder (low), debounced 120 ms. Entry ids stay valid
  until an index run, so the url and extras maps are cleared on `index-done`.
- **Turning on hover:** a tile showing a mesh still loads its strip after the pointer rests 250 ms and slides it by
  whole frames (CSS `steps(12)`, 1.2 s a turn). A strip that isn't there yet, or fails, leaves the still. Off under
  `prefers-reduced-motion`.

### Carousel slots (`browse.rs`)

Each `Item` carries `composite` (0–4 cells, empty = no composite slot), `pics` and `slot`. The UI's slot list is
`[composite?] ++ pics`.

- **A leaf** with a thumbnail kind gets `pics = [itself]`. A file with pictures of its own (a 3MF) gets them as more
  slots in the UI once its thumbnail arrives (`carousel.ts`, `sub` slots): `[still, picture 1…n]`, never a composite,
  slot 0 (the `.rels` picture) by default. Choosing picture n saves `<file's entry key>#<n>`; `browse.rs` turns that
  back into `slot = n`, and the UI shows slot 0 until the pictures have arrived.
- **Scene previews count as meshes** when a folder collects pictures: they fill its carousel and composite only when it
  has no images.
- **A folder-like node** collects up to 11 images (`MAX_IMAGES`) and then up to 6 meshes (`MAX_MESHES`), depth-first in
  tree order, deduplicated by entry. The hero goes first if it lives outside the folder. A saved choice that is a deeper
  picture not among them is put first.
- **A folder's chosen picture stands in for that folder:** when the walk reaches a descendant folder with a saved
  choice that resolves to a picture below it, it takes just that picture and doesn't go in. A `composite` choice or a
  stale one doesn't stand in.
- **The composite** holds each folder-like child's shown picture (its chosen or default slot; the first pic if that is
  a composite), up to 4. It is filled up from the node's own images, and from its meshes only when it has no images. It
  needs at least 2 cells.
- **Default slot:** the hero (the model's hero, or a group's poster, via `hero_key`), else slot 0.
- **Saved choices** (`select_thumb(node_key, image_key)`, the `selected_thumb` table, keyed by `m:`/`f:` node keys,
  with `image_key` = an entry key, `<entry key>#<n>` or `composite`) are honoured only while they still exist.
- Right-click → **Use as folder thumbnail** on an image or mesh tile saves it for the open folder (`Listing.key`).

## Drag-out and Extract to…

- **Export plan** (`stl-core::vtree::export`): the folders and files a drag writes, depth-first in tree order, each
  folder before its contents. Paths mirror the current view: lowercase names, the `_N` clash names, only shown files.
  The Models view includes `docs`. Locked items and files without an entry are left out and counted as skipped.
- **Windows-safe names** (`safe_name`): `< > : " / \ | ? *` and control characters become `_`, trailing dots and spaces
  are dropped, empty becomes `_`, device names (`con`, `com1.stl`…) get a leading `_`, and names that clash after this
  get `_2`, `_3`.
- **Targets:** `begin_drag(view, targets)` and `extract_to(view, targets, dest)` take `Target { path, name }` (`name ==
  ""` = the folder at `path`), so search hits from many folders go together. `nodes_for` drops a target that lies
  inside another target, and clashing top-level names get the plan's postfix.
- **Locate up front** (`stl-index::export`): every file's (source kind, source path, chain, size) is looked up when the
  plan is made, and entry ids are never trusted after that. Drags and Extract to… are refused during an index run,
  because a run re-lists sources and reuses entry ids.
- **Extraction** goes through one `ExtractSession` (`stl-scan::session`) per drag or extract. It keeps the stack of
  open nested archives, so files from the same archive in a row don't reopen it. A failed file drops the stack. 7z and
  RAR decode from the start for every member, so file-by-file extraction is quadratic: Extract to… writes a whole 7z/RAR
  level in one pass, and a drag reads the level on the first request into it, keeping the other files in delete-on-close
  temp files (up to 2 GiB). Zip members and loose files are read directly. A panic in a reader fails only that file.

### The drag (`stl-drag`)

- **Gesture:** pressing a tile and moving 6 px (`dragstart.ts` `THRESHOLD`) with the primary button held calls
  `begin_drag` with the selection, or with just that tile when it isn't selected. If the button is already up when
  the drag would start, no drag starts.
- **Descriptors** are one per folder and file of the export plan, with `/`-separated relative paths, served by the same
  `DragContents` (`FileServer` with the read-ahead) on both platforms.

#### Windows (OLE)

- If the button is up when `DoDragDrop` would start, no drag starts (`primary_button_down` honours swapped buttons).
- **The data object** offers `CFSTR_FILEDESCRIPTORW` (one descriptor per folder and file, converted to `\` separators,
  `FD_ATTRIBUTES | FD_PROGRESSUI`, plus `FD_FILESIZE` when known), `CFSTR_FILECONTENTS` (one `IStream` per file,
  extracted only when Explorer asks) and `CFSTR_PREFERREDDROPEFFECT` = copy. It implements `IDataObjectAsyncCapability`,
  so Explorer copies on its own thread with its own progress dialog.
- **Threads:** the object is created in the MTA (kept alive by `CoIncrementMTAUsage`, in `prepare`). `run_drag` runs
  `DoDragDrop` on the main STA thread (`run_on_main_thread`) with a marshaled proxy, so Explorer's calls go to COM
  worker threads and never block the UI.
- **Streams** are delete-on-close temp files, so nothing is held in memory or left behind after a crash.
- **Sizes:** Explorer copies exactly the declared size. A file without a size, or longer than its indexed size (a stale
  index), fails rather than being truncated.
- **Paths over 259 characters** don't fit in a descriptor. They are left out of a drag, and the UI says how many.
  Extract to… still writes them.
- `tests/drop.rs` drops on the shell's own drop target for a temp folder, so no mouse is needed.

#### macOS (file promises)

- **One `NSFilePromiseProvider` per top-level item** (a dragged file or folder), with a generic icon for its type as
  the drag image. `run_drag` runs on the main thread from the window's content view with a synthesized mouse-dragged
  event; the drag goes on after it returns. Only copy is offered.
- **Finder asks for each item after the drop**, and the delegate writes it on its own serial `NSOperationQueue`, so
  the extraction never blocks the UI. The writing is `promise::Promises` (platform-neutral, unit-tested on both OSes):
  - Each file goes to a uniquely named `.<name>.….part` and is moved into place when complete; on failure nothing is
    left.
  - Nothing is overwritten: a name already in the drop folder becomes `name 2`, `name 2.stl`… as Finder does. The
    final move never replaces a file, so two drops into one folder at once can't clobber each other.
  - A folder writes all its files; a failed file doesn't stop the rest, and Finder shows "N of M files could not be
    written" with the first error.
  - A file longer than its indexed size fails. There is no path length limit.
- **The read-ahead is released** when the drag is cancelled, after the last item is written, or, if Finder never
  asked for any of its files, when the next drag starts. A drag that is still being written is kept alive (its source
  and delegate) until its last item is done, even while later drags run.

### Extract to…

- Right-click → **Extract to…** picks a folder, then writes the plan on a background thread (`extract-progress
  { index, total, name }`, one `extract-done { ok, files, dirs, skipped, failed: [[rel, error]], dest, seconds,
  cancelled }`). With several items selected, this is the only menu entry ("Extract N items to").
- A top-level name that already exists becomes `name (2)`, `(3)`… (`unique_dest`), so nothing is overwritten. Files are
  written as `.part` and renamed when complete. A file that fails is reported and the rest are still written. It can be
  stopped. Only one extraction runs at a time.

## Opening files

- A double-click on a file (or right-click → Open) opens it in its default app (`open_file`, `tauri_plugin_opener`).
- A loose file opens in place. An archive member is written to `stl-browser-open/<n>/` in the OS temp folder first
  (`%TEMP%` on Windows, `$TMPDIR` on macOS; the folder is emptied at app start), and is marked as downloaded
  (`mark_downloaded`): a Mark-of-the-Web on Windows (`Zone.Identifier`, internet zone), the `com.apple.quarantine`
  xattr on macOS.
- Only models, scenes, images and documents (not `.html`/`.htm`) are opened, never programs, scripts or shortcuts from
  an archive (`export.rs` `openable`).
- Opening is refused during an index run. A second double-click while a file is still being extracted is ignored.

## Search

- **Scope:** the folder open when the search bar opened, and everything below it, never the folder itself. If the
  scope vanished after a re-index, the search falls back to the deepest existing folder and reports it (`Found.path`).
- **Modes** (`stl_core::vtree::search::Matcher`):
  - **Free** (the default): the query's characters appear in order, matched greedily, leftmost first. Adjacent matched
    characters merge into one range.
  - **Word**: the leftmost occurrence of the query as a substring.
  - **Regexp**: the Rust `regex` crate, unanchored. A bad pattern is an error shown in the bar. A zero-width match is a
    hit with no marks.
  - Case-insensitive matching compares per character, so ranges stay aligned. Marks are **char** (Unicode scalar)
    ranges `[start, end)` into the displayed name.
- **What is matched:** the Files view matches `Node.label` (the disk or archive name with its case), and only there is
  the Aa (case) toggle shown. Marks are kept only when the lowercase label is a prefix of the shown name, so clash
  suffixes are fine. The Models view matches the cleaned lowercase `name` and is always case-insensitive.
- **Results:** depth-first in tree order, at most 1000 (`SEARCH_LIMIT`, with a "showing first 1000" note). Hidden
  nodes and everything below them are skipped. Each `Hit` flattens an `Item` (thumbnails, carousel, tags) and adds
  `parent` (full segments) and `marks`, so open, drag and extract work with `(parent, name)`.
- **UI rules:** queries are debounced 120 ms and stale replies are dropped with a sequence counter. `index-done`
  re-runs an open search. Navigating (crumb, Up, Back) or switching views closes search first. Double-clicking a
  folder hit opens it and closes search, and Back returns. Double-clicking a file hit goes to its folder with the file
  selected and scrolled into view, and right-click → Open still opens it. Hits are tiles by default, and a button
  switches to a compact list (remembered).
- **Tag terms:** see [Tags](#tags).

## Tags

### Kinds

- **User tags** are free-form labels (green).
- **Generated tags** come from the index, each kind with its own colour:
  - **support** (yellow): `supported`, `unsupported`, `lychee`, `chitubox`;
  - **scale** (blue): `25mm`, `32mm`…;
  - **creator** (red): the parser's designer handle;
  - **edition** (purple): `5e`.
- Dot and pill order: user, support, scale, creator, edition (`TagKind`'s `Ord`).
- **Support, scale and creator can be set by the user.** A set value replaces that kind's generated values on that
  item, keeps the kind's colour and gets a pencil mark. At most one user-set value per kind per item, so setting a new
  one replaces the old. Edition can't be set, only hidden.
- **A wrong generated tag can be hidden** per item. Hiding `supported` on a variant folder drops the folder out of
  `tag:supported`, but the files inside keep their own `supported` tag.

### Generated tags (`vtree/tags.rs`, `vtree/models.rs`, `views.rs`)

- Built with the views into `Info.tags` and never stored.
- **Variant folders** get the atoms of their label (`variant_tags`: split on `+`; support words; digits + `mm` is a
  scale; anything else, such as `finished`, is ignored). **Files** get the atoms of their `model_files.variant`
  (printable files only).
- **Models** get creator and edition from their columns, plus the support and scale atoms all their variants have in
  common. A model with no variants gets none. **Groups** get creator and edition from `groups.attrs` JSON.
- **Supported and unsupported exclude each other** (`exclusive`): an item that would get both gets neither, so a model
  with both kinds of variant has no support tag, and its variant folders and files carry their own. `lychee` and
  `chitubox` are file formats and never conflict. Scale follows the same rule: different scales give no scale tag.
- **Files-view nodes** copy tags from Models nodes (`copy_tags`). Leaves match on the same `f:` key. Folders take the
  tags their matching Models nodes (same tag key) have in common. Only Model, Variant and File nodes, and Group or
  Collection nodes that have tags, take part.

### Storage and keys (`tagstore.rs`, schema v6)

- `tags(kind, name)` + `node_tags(tag_key, tag_id)` hold user tags and user-set values. `hidden_tags(tag_key, kind,
  name)` holds hides. These are user data, never rebuilt. Rows for vanished paths are kept, and a `tags` row with no
  `node_tags` left is deleted.
- **Names are normalised:** trimmed, lowercased, whitespace runs collapsed (`tags::normalize`). Empty names are refused.
- **The tag key** is `r:` + the real path, lowercased, with `/` turned into `\` and a trailing `\` trimmed
  (`tag_key`). Real paths, not `m:`/`f:` keys, so a tag shows in both views and survives adding an outer root. A merged
  folder is written under each of its real paths and read as the union.
- **Shadowed nodes** (subfolders whose real paths are all their parent's) delegate user tags to their owner; see "Tag
  keys" in [`virtual-tree.md`](virtual-tree.md). Edits on a shadowed node are refused ("`<name>` shares its folder with
  `<owner>`; tag `<owner>` instead"), and the panel offers the owner as a button. A write to a node with no real path is
  an error ("can't be tagged").

### Effective tags and search

- **Effective tags** of a node = generated − hidden − generated kinds overridden by a user-set value + user-set
  (`UserTags::effective`). Only an item's own effective tags show as dots and pills.
- **Query parsing** (`parse_query`): `tag:word` and `tag:"two words"` terms are split out, also in Regexp mode. The
  prefix is case-insensitive and counts only at the start of a term (`stag:x` is name text). An unterminated quote takes
  the rest of the string, and a bare `tag:` is ignored. The remaining text is joined with single spaces.
- **Matching:** a term matches a tag of any kind by name, and hidden tags never match. Every term must hold.
  Inheritance (`Inherited`) carries the effective tags of all ancestors from the tree root, not only from the scope. A
  node passes when its own tags plus the inherited ones cover every term. **A user-set value overrides everything below
  it:** below an ancestor with a user-set support, scale or creator value, generated tags of that kind stop matching
  (a model set to `32mm` drops its `25mm` variants from `tag:25mm`).
- **Topmost match:** a search of only tag terms lists the topmost passing nodes and doesn't descend below a hit. With
  a name term too, a hit must match the name and pass the tags, and the walk descends. Searching inside a folder that
  already has the tag returns its direct children. An empty query with no tag terms returns nothing.
- **Completion** (`tagcomplete.ts`): once the term under the caret is `tag:` plus one character, the bar lists known
  names (`all_tags`, deduped across kinds) with that prefix. Up/Down select, Tab or a click replaces the whole term
  (quoted when the name has a space, plus one space), and Esc closes the list before it closes search. The list reloads
  when the search field gets focus.

### Commands

- One node is addressed as `(view, path, name)`, where `name == ""` means the folder at `path` itself.
- `node_tags` returns `own`, `inherited` (each ancestor's own effective tags, root down to the parent, with that
  folder's path as `from`) and `hidden`. `add_tag`, `remove_tag`, `hide_tag` and `unhide_tag` take `kind` and `tag`.
  `all_tags` returns the stored names plus the distinct generated names of the Models tree.
- `Item.tags` (`TagView { kind, name, byUser }`) is the node's own effective tags. User tags and hides are read per
  call, like `selected_thumb`, so an edit needs no tree rebuild. Editing during an index run is allowed, because keys
  are paths, not entry ids.

## Multi-select

- **Gestures** (tile grid, grid results and the result list): click selects one; Ctrl-click toggles; Shift-click
  selects the range from the anchor in display order; Ctrl+Shift-click adds that range. Pressing on empty space and
  dragging draws a rubber band (Ctrl adds to the selection made before the band). A click on empty space clears the
  selection. Ctrl+A selects everything shown, and Esc clears (only outside inputs; an open menu closes first). In the
  result list a band starts only off the rows.
- **Selection model** (`selection.ts`): an immutable `{ ids, anchor }` with `click`, `band`, `all`, `prune` and
  `focus`. Ids are `item.name` in listings and `hitId(h)` in results. `focus` keeps the selection when right-clicking or
  dragging a selected item, else selects just that one.
- **Rubber band** (`band.ts`, `bandgesture.ts`): hit-tested from the grid geometry (`gridHits`: cols, tile, gap, pad,
  row height; `listHits` for rows), not from DOM elements, so rows scrolled past count in the virtualized grid. It
  auto-scrolls near the top and bottom edges.
- **Lifetime:** navigation, a view switch and a new query, scope or mode clear the selection. The refresh after a tag
  edit or an index run keeps it, minus items that are gone (`prune`).
- **Single-item behaviour stays as it is:** plain click, double-click open, the 6 px drag, carousel arrows,
  right-click, and the one-item tag panel. Open, Rescan and Use as folder thumbnail are single-item only.

### Bulk tags

- `nodes_tags(view, targets)` returns `TargetTags { own, hidden, editable, covered }` per target. `editable` = has tag
  keys and no owner. `covered` = not editable because its owner is among the targets.
- `edit_tags(view, targets, op, kind, tag)` → `{ changed, skipped }`. Not-editable targets are skipped, and counted
  only if not covered. Per target:
  - `add`: add the value (single-value kinds replace).
  - `remove`: delete the user-set value and hide the generated one, whichever are present.
  - `fill` (for items lacking it): unhide where hidden, else add.
  - `edition` can't be added or filled; it is an error before any write.
- **The panel** (`mergeTags` in `tags.ts`, over editable targets only) shows tags every item has as solid pills and the
  rest faded with a `k of N` tooltip. Clicking a faded pill fills it, and × removes it everywhere it is. "From folders
  above" and "Hidden" show only for a single item. Skipped items show the hint "N items share a folder with another item
  and are skipped"; when every item is skipped, editing is disabled.
