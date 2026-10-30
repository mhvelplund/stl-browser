# Milestone 7: search (and an icon-only rescan button) — design

## Context

stl-browser is a Tauri + Svelte app that shows a library of STL files and archives as a virtual folder
tree, in two views: **Models** (resolved, with cleaned-up names) and **Files** (disk and archive names).
Milestone 7 was planned as "tags, search, filters". The user narrowed it to **search only** and a tidier
toolbar. Tags and filters, including filters on variants like `supported` and on attributes like designer
or edition, move to a later milestone with its own design. Today the only way to find something is to
click through folders.

## Goals

1. The Rescan button becomes icon-only, showing the glyph **🗘 (U+1F5D8)** instead of the SVG and the
   "Rescan folder" / "Rescan all" text. Behaviour, `disabled={busy}` and the scoped tooltip stay the same,
   and an `aria-label` takes the place of the text.
2. A **magnifying-glass** icon button sits in the top bar next to the rescan button. It toggles a
   **search bar** that sits under the toolbar and above the browser, across the full width.
3. The search bar contains:
   - a text field, focused when the bar opens;
   - a **mode** toggle with three states: **Free** (the default), **Word** and **Regexp**;
   - an **Aa** toggle for case sensitivity, off by default. It is **hidden in a view whose names are
     lowercased**: the Models view. There, search is always case-insensitive.
4. Matching runs against each node's name **as the current view names it**: the original disk or archive
   name, with its case, in the Files view, and the cleaned-up lowercase name in the Models view.
   - **Word**: the query is a contiguous substring of the name.
   - **Free**: the query's characters appear in the name in order, not necessarily next to each other.
   - **Regexp**: the name matches the query as a regular expression (Rust `regex` crate, unanchored).
   - With case sensitivity off, all three modes ignore case.
5. **Scope**: the search covers the folder the user was in when the bar opened, and everything below it,
   but not that folder itself. The bar shows the scope, for example "in Coll › Orc", or "in Library" at
   the root.
6. When the field is non-empty, the tile grid is **replaced by a list** of matching folders and files.
   Each row shows a kind icon, the name with the matched characters highlighted, and the folder it's in,
   dimmed and relative to the scope (`Coll › Orc › supported`). Clearing the field brings the folder view
   back while the bar stays open.
7. Result actions:
   - **Double-clicking a folder** opens it in the normal browser and closes search. Back then returns to
     the folder you were in before searching.
   - **Double-clicking a file** opens it in its Windows app, as `openFile` already does. Search stays open.
   - **Single click** selects the row, and the status bar shows its real path, as it does for tiles.

## Non-goals

- Tags, filters and attribute or variant facets (a later milestone).
- Thumbnails in the results list.
- Searching file contents, readmes or the raw names the Models view hides.
- Saved searches and search history.

## Anti-goals (what would make it a failure)

- Typing that lags: results must feel instant on a large library.
- A search that silently covers more or less than the scope the bar shows.
- Case sensitivity that does nothing, which is why the case-preserved name below is needed.

## Decisions made

| Decision | Whose |
| --- | --- |
| M7 covers search only; tags and filters come later | user |
| Double-clicking a folder goes there and closes search; Back returns | user |
| Rows show an icon, the highlighted name and a dimmed relative path | user |
| Match the name as the view names it (Files: original; Models: cleaned) | user |
| Default mode Free, case-insensitive; scope fixed when the bar opens | user |
| Aa hidden in the Models view (lowercased names); shown in the Files view | user |
| Switching views closes search, so the Aa state never applies to Models | my call |
| Matching runs in Rust on the cached view tree, not in JS | my call |
| Keep the case-preserved name on `Node` as a new `label` field (used by the Files view) | my call |
| Files view: clash suffixes (`(2)`) aren't searched; Models view searches `name`, suffixes included | my call |
| Cap of 1000 results, with a "showing first 1000" note | my call |
| An invalid regexp shows an inline error in the bar and an empty list | my call |
| Esc in the field closes the bar; clicking the glass again also closes it | my call |
| Breadcrumb, Up, Back or a view switch while searching closes search first | my call |
| Drag-out and the context menu (Open, Extract to…) work on result rows | my call |
| The bar's mode, case and open state are not remembered across restarts | my call |

## How it gets built

### Rust: the case-preserved name

- [crates/stl-core/src/vtree/mod.rs](crates/stl-core/src/vtree/mod.rs): add `pub label: String` to
  `Node`, set in `Tree::add` from the `name` argument before it's lowercased. `finish_children` renames only
  `name`. Lookups (`find`, `walk`) keep using `name`. Update the `Node { .. }` literals in tests.
- [files.rs](crates/stl-core/src/vtree/files.rs) already passes the cased disk or archive `display`. The
  Models view doesn't need `label`: it searches `name`.
- Note: tiles in **both** views display the lowercase `name` today, so the Files view's case only shows up
  in search. Showing original case on Files tiles is out of scope here.

### Rust: the matcher (pure, unit-tested)

- New `crates/stl-core/src/vtree/search.rs` (exported from `vtree`):
  - `enum Mode { Free, Word, Regexp }`
  - `struct Matcher` built by `Matcher::new(query, mode, case_sensitive) -> Result<Matcher, String>`.
    It returns an error only for a bad regexp. Case-insensitive Regexp uses `RegexBuilder::case_insensitive`.
  - `fn find(&self, text: &str) -> Option<Vec<(usize, usize)>>` returns the matched char ranges, which the
    UI uses for highlighting. Free merges adjacent hits into ranges and matches each query character
    greedily, leftmost first.
  - `fn search(tree, root: NodeId, m: &Matcher, keep: impl Fn(&Node) -> bool, limit) -> (Vec<Hit>, bool)`
    does a depth-first walk in tree order (the same order as listings) over the descendants of `root` and
    skips `root` itself. `keep` filters hidden nodes and stops descent into them. The `bool` reports
    truncation. `Hit` holds the node id plus the segment path from `root` to its parent.
- `regex` is already a dependency of stl-core ([names.rs](crates/stl-core/src/names.rs)).

### Rust: the Tauri command

- [app/src-tauri/src/browse.rs](app/src-tauri/src/browse.rs) gets
  `pub fn search(views, view, path, query, mode, case_sensitive) -> Result<Found, String>`:
  - `t.walk(path)` sets the scope, using the segments actually reached, as `list` does.
  - `keep = |n| visible(view, n)`, which reuses the existing filter so the Files view hides `notes.txt`.
  - Matched text: `label` in the Files view; `name` in the Models view, where `case_sensitive` is ignored
    (forced off).
  - `Found { path: Vec<String> /* scope used */, hits: Vec<Hit>, truncated: bool }`, where each `Hit` is
    `{ name, kind, key, fileKind, openable, real, parent: Vec<String> /* full segments */, marks: Vec<[usize;2]> }`.
    `parent` lets `openFile`, `beginDrag` and `extractTo` work unchanged with `(view, parent, [name])`.
    `marks` are char ranges into `name`. When `label` and `name` differ beyond case and suffix (Unicode
    lowercasing can change the length), send `marks: []`, which means no highlight.
  - Reuse `kind_name`, `visible` and the `Views` cache (`state.views()`).
- Register a `search` command in [commands.rs](app/src-tauri/src/commands.rs) next to `list_folder`,
  following the `*_impl(state, ..)` pattern so it can be tested.

### Frontend

- [app/src/lib/api.ts](app/src/lib/api.ts): types `SearchMode = "free" | "word" | "regexp"`, `Hit` and
  `Found`, plus a call `search(view, path, query, mode, caseSensitive)`.
- [app/src/lib/Toolbar.svelte](app/src/lib/Toolbar.svelte):
  - The rescan button becomes `class="btn icon"` with the 🗘 glyph and `aria-label`. Keep the `title`.
  - Add a magnifying-glass `btn icon` (SVG, matching the Back and Up icons) before it, with
    `aria-pressed={searching}`. New props `searching` and `onSearch`.
- New `app/src/lib/SearchBar.svelte`: the input, a segmented Free/Word/Regexp control (reuse the
  `.seg-group`/`.seg` styles from Toolbar), an Aa toggle rendered only when `view === "files"`, the scope
  label and an error slot.
- New `app/src/lib/ResultList.svelte`: a virtualised or plain scrolling list of rows with `KindIcon`, the
  highlighted name and the dimmed relative path. Selection, double-click, drag and context-menu events
  mirror `TileGrid`'s props (`onSelect`, `onOpen`, `onDragOut`, `onMenu`).
- [app/src/routes/+page.svelte](app/src/routes/+page.svelte):
  - State: `searching`, `query`, `mode = "free"`, `caseSensitive = false`, `scope: string[]`,
    `found: Found | null`, `searchError`.
  - Opening the bar sets `scope = nav.path`. Every change to the query or a toggle runs a debounced
    (~120 ms) `api.search` call, and stale replies are dropped with a sequence counter, as in `load()`.
  - `main` shows `ResultList` when `searching && query`, and the current listing otherwise.
  - Opening a folder result: `closeSearch(); load(goTo(nav, [...hit.parent, hit.name], scroll))`, so Back
    works through `nav.ts`.
  - Opening a file: `api.openFile(view, hit.parent, hit.name)`. Generalise `launch`, `dragOut`, `extract`
    and `showMenu`, which assume `nav.path` today, to take the parent path.
  - `onKey`: Esc closes search when the field has focus. The existing guard already ignores Backspace in
    inputs. `crumb`, `goUp`, `back` and `switchView` call `closeSearch()` first.
  - After `index-done`, re-run the current search if one is open, because the tree changed.

### Docs

- Commit this design as `docs/superpowers/specs/2026-10-30-search-design.md`.
- In [docs/index.md](docs/index.md), split row 7: "7. Search" and "8. Tags and filters (later)". Mention
  the search bar wherever the toolbar is described.

## Left to the builder

- The exact layout and spacing of the bar, the Aa toggle's look, and the row height.
- Whether `ResultList` needs virtualisation at a cap of 1000 rows (probably not).
- Highlight styling, which should use the existing theme tokens in `theme.css`.

## Verification

- `cargo test -p stl-core` covers the matcher: Free subsequence, Word substring, Regexp, case on and off,
  a bad regexp returning an error, and mark ranges.
- `cargo test -p stl-core vtree` checks that `label` keeps case while `name` stays lowercase, and that
  `find` and `walk` are unchanged.
- `cargo test` in `app/src-tauri` adds `browse.rs` tests on the `testutil::library` fixture:
  - the scope is respected, and the scope node itself is excluded;
  - the Files view hides `notes.txt`;
  - the Models view matches cleaned names;
  - `parent` round-trips through `list_folder_impl(view, parent)`, so the hit is among its items;
  - the truncation flag is set.
- Frontend unit tests (vitest, as in `nav.test.ts`) for any pure helper, such as relative-path formatting
  and splitting a name into highlighted runs.
- Manual check in the running app:
  - the rescan glyph and its tooltip;
  - the glass toggles the bar and focuses the field;
  - typing replaces the grid with the list, and clearing it restores the grid;
  - mode and Aa change the results, Aa is absent in the Models view, and a bad regexp shows an error;
  - double-clicking a folder navigates there and Back returns;
  - double-clicking a file opens it;
  - Esc closes the bar;
  - dragging a result to Explorer works.

## Next step after approval

Commit the spec, then invoke `superpowers:writing-plans` to turn this into the implementation plan.
