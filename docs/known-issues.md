# Known issues and open ideas

These are known problems that are not fixed yet, grouped by area. Remove an item when you fix it, and add new ones
when a review defers them.

## Scanning and indexing

- **Slow ZIP opening.** Opening a ZIP reads every local file header twice. This is slow on network drives, and one bad
  header fails the whole archive.
- **Peak memory.** In-memory nested archives can peak at about 1.5 GiB in the worst case (3 levels × buffer growth).
- **Nested-archive warnings are not stored**, so the UI can't show which nested archives were skipped.
- **Stop waits for the current source.** Listing one big archive can't be interrupted part-way.
- **Adding an outer root after an inner one** re-reads every archive, because the cache lookup filters on `root_id`.
  The result is correct but slow, and there is no test.
- **Adding an outer root changes the resolution unit and the `m:` keys** of the inner content: it all becomes one
  collection. Saved thumbnail choices (`selected_thumb`) use `m:` keys, so they are lost for that content. Tags avoid
  this with real-path keys.
- **Duplicate roots already in an old DB** (`D:\Models` and `d:\models\`) are never merged. New ones are refused.
- **A source that keeps failing** makes its collection re-resolve on every run (about 0.1 s per collection).
- **A rescan of a folder that was renamed on disk** deletes its sources, but the new name is outside the scope and isn't
  added. Its models are gone until a parent folder is rescanned.
- **Edge cases:**
  - A file deleted between discovery and listing fails its folder source.
  - Paths that aren't valid Unicode are stored lossily.
  - A DB from a newer app version opens without a warning.
- **Missing indexes:** `sources(root_id, collection)` and `assets(group_id)`.
- **Missing tests:** Cyrillic paths end-to-end; multi-volume, solid and password-protected RAR (RAR fixtures can't be
  made without WinRAR; password-protected ZIP and 7Z are tested).

## Resolver

- **`parse_name` compiles a regex on every call.**
- **ASCII-only digit checks** (`is_digits`, the designer-tag test). Sources with no collection are skipped.
- **Same-stem archives merge** (`X.zip` + `x.rar` become one model), as in the prototype. The `x_1`/`x_2` rule applies
  only to the Files view.
- **Models without images show their first mesh in tree order**, often a base or an arm. Picking the largest file would
  do better.
- **The Models view can show fewer documents than the Files view** for the same scope, because the resolver doesn't
  place boilerplate like readmes and licenses.
- **The default shown endings leave out some kinds the resolver knows** (`ply ztl zpr step stp`,
  `cbddlp goo pwmo pwms`, `rtf`), so a model made only of those shows a file count but opens empty.

## App and startup

- **The startup error dialog** has never been exercised. `app_dir()` panics if `~/.stl-browser` can't be created.
- **`config.yaml` changes need a restart.** There is no settings UI.
- **Navigation can be lost under rapid input.** `nav` only changes when a listing arrives, so two quick Backspaces go
  up one level, and an `index-done` reload that lands mid-navigation drops the click.
- **`Listing.path` echoes the request's casing.** It's harmless today because the UI always sends tree names.
- **"files" means two things on screen.** The footer counts every indexed entry, while the title and tiles count only
  mesh files.
- **Selecting a tile hides the last index result** in the status bar until the selection clears.
- **Folders that hold only hidden files** show as empty folders, in both views.

## Thumbnails

- **Broken thumbnail images.** A thumbnail `<img>` that 404s shows the broken-image glyph: there is no `onerror`, and an
  `immutable` URL won't reload.
- **A composite whose cells all failed** shows four blank cells instead of the kind icon.
- **Skeleton flash after an index run.** The url map is cleared before the reload answers.
- **Image decoding is heavy.** It converts and resizes at full resolution, which costs a lot for 8k photos times the
  worker count.
- **The `thumb` protocol spawns one OS thread per request.** It should use `spawn_blocking`.
- **The queue builds a group key per queued job on every batch pop**, which is O(N) work under the lock.
- **Solid archives are decompressed once per 64-job batch**, and batches of one archive can run on several workers at
  once.
- **A NULL entry size becomes 0**, which bypasses the size caps. Archive listings always store sizes.
- **Orphaned cache files are never removed**, so the cache grows as archives change. Deleting `thumbs/` is always safe.
- **Carousels ask for up to 6 STL renders** as well as images, so a first visit renders more. Collecting pictures walks
  until both caps are reached, and `locate` lowercases every real path of the target view on each switch. Both are fine
  on a large library so far.
- **Not checked by hand:** thumbnails in the light theme, OBJ files from real data, and padlock tiles next to
  thumbnails.

## Drag-out, Extract to… and opening files

- **Extract to… may act on the wrong folder.** It reads the current folder after the picker returns, and Backspace with
  the context menu open also navigates.
- **The status line lists every failed file**, so a source deleted since indexing can fill it with thousands of
  entries.
- **Closing the app during Extract to…** leaves `.part` files.
- **The app never learns how a drag ended.** A file that fails shows only in Explorer's own error dialog.
- **Stream edge cases:** `IStream::Read` doesn't check for a null buffer, and `CopyTo` drops bytes if the target
  accepts a short write.
- **Large drags:** the descriptor block is rebuilt on every `GetData` (about 60 MB per call at 100k files), and streams
  are staged in `%TEMP%`, so a file bigger than the free space on C: fails.
- **Reserved names:** `conin$`, `conout$` and the superscript `com¹` / `lpt¹` device names are not escaped.
- **A tiny race:** a button release between the button check and `DoDragDrop` drops onto whatever is under the cursor.
- **No drag image:** the drag shows only the cursor.
- **Swapped mouse buttons** have not been tested with the OLE key state.
- **Opening an archive member blocks its command** until the member is extracted, shows only "Opening …", and can't be
  stopped.
- **Files opened from archives stay in `%TEMP%`** until the next app start, and one still open in its app then isn't
  deleted.

## Accessibility and polish

- **Tab can't reach tiles outside the rendered rows**, and there are no arrow keys in the grid (and no keyboard-only
  selection moves).
- **The progress block's `role="status"`** announces every source.
- **Locked tiles are tinted, not dimmed.** A view switch draws one frame with the old listing. After a fallback, the
  old scroll offset is restored.
- **Tag popover:** it may flash for a frame at the top-left, and it is mouse-only (the dots have only an
  `aria-label`).

## Tags and search

- **Siblings that share a folder share user tags and hides** when that folder isn't their parent's path (two models
  from loose files in one folder below a group with a folder of its own). Fixing it needs node-level keys that survive
  adding an outer root.
- **Tag search:**
  - `tag:"a"b` becomes the tag `a` plus the term `b`; this is unspecified.
  - The ancestor walk computes effective tags even when the query has no tag terms.
  - There are no tests for a tagged subtree skipped by the filter, the 1000-hit cap with tag terms, a user tag not
    masking a generated one, or an ancestor walk that stops at an unresolved segment.
  - `node_tags` echoes the caller's casing in `from`.
  - `tag:` terms collapse runs of whitespace in the name part of the query.
- **Tag store:** an unknown kind in a row is dropped silently, `hide` accepts any kind, and a user tag on a vanished
  path is kept for good.
- **Generated tags:** the Files-view copy skips Image and Locked nodes when matching by path, and `groups.attrs` JSON
  parsing has no test. Suggestion de-duplication in the tag panel has no test.

## Ideas not built yet

- A tag manager: rename, merge or delete a tag everywhere.
- Filters other than `tag:` terms (such as "has no picture"), and negation (`-tag:x`).
- Catalog id as a tag.
- Showing inherited tags as dots.
- Searching file contents or readmes, saved searches, and search history.
- Showing the original case on Files-view tiles (they show the lowercase name; the case appears only in search).
- Bulk Open, Rescan or Use as folder thumbnail for a selection.
