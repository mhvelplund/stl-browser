# The UI

The Svelte 5 + TypeScript UI is a SvelteKit app built with the static adapter. It is one page,
`app/src/routes/+page.svelte`, plus components and pure logic modules in `app/src/lib/`. Behaviour rules that are not
only UI are in [`features.md`](features.md). The clickable mockup the layout follows is
<https://claude.ai/artifact/Vum7b4yY5TjCBW1Csez2BH>.

## Layout

- **Toolbar** (`Toolbar.svelte`): Back, Up, breadcrumbs (starting at "Library"), and icon buttons.
  - **The view toggle** is one button showing the view it switches *to*: a wireframe box (Models) in the Files view, or
    a page with three lines (Files) in the Models view. Its hover text is "Switch to Models?" / "Switch to Files?".
  - **Search** is a magnifying glass, `aria-pressed` while the bar is open.
  - **Tags** shows or hides the tag panel.
  - **Rescan** is the 🗘 glyph (U+1F5D8, in "Segoe UI Symbol"). Its label is "Rescan folder" inside a folder and
    "Rescan all" at Library.
  - **Add folder** opens the dialog plugin's folder picker.
  - Add and Rescan are disabled while an index run is busy.
- **Search bar** (`SearchBar.svelte`), under the toolbar while search is open:
  - the field, focused when the bar opens, with the placeholder "Search names, tag:name";
  - Free / Word / Regexp as a segmented control;
  - Aa (case), shown in the Files view only;
  - the scope ("in Library", "in coll › orc");
  - an error slot (bad regexp) and the truncation note;
  - the list/tile toggle;
  - the `tag:` completion list.

  Esc closes it, and clicking the glass again closes it too.
- **Title row:** the folder name (or "Library") and `folderSummary` counts.
- **Main area:** a virtualized `TileGrid`, or in search the result grid or the compact `ResultList`. Beside it is the
  `<aside>` tag panel (`TagPanel.svelte`, about 260 px). The grid reflows by itself because it binds `clientWidth`.
- **Empty states:** with no roots, "No folders yet" with an Add folder button. An empty folder shows "This folder is
  empty". A search with no hits shows "No matches".
- **Status bar** (`StatusBar.svelte`):
  - left: "<n> folders indexed · <models> models · <files> files", with a warning when a root is missing (the paths are
    in its `title`);
  - middle: a hint, which is the selected item's real paths (`|`-joined), `N selected`, the last result (index, extract,
    open), or usage help;
  - right: index or extract progress (`Indexing k / n`, a bar, the source name, and a Stop × button), and
    "Thumbnails: n queued" / "Thumbnails up to date".

  Results stay until the next selection.
- **Context menu** (`ContextMenu.svelte`), on a tile or a hit:
  - **Open**, **Extract to…**, **Rescan** (folders), and **Use as folder thumbnail** (image or mesh files, in listings
    only);
  - with several items selected, only Extract to….

  Escape, a click outside or scrolling closes it. The WebView's own menu is suppressed on tiles only.

## Tiles

- **`TileGrid`:** tiles at least 200 px wide with an 8 px gap and 20 px padding. Rows are `tile + 56 (TEXT) + gap`
  high. Only the visible rows (plus overscan) are rendered (`grid.ts`). It restores the scroll offset on Back and
  reports the visible index range for thumbnail requests. Revealing a tile re-applies when the layout settles (a
  scrollbar appearing drops a column) until the user scrolls.
- **`Tile`:**
  - a button with `title` = the real paths, one per line;
  - square art (`TileArt`), a badge top-left (the archive format, or `MODEL`), the name (ellipsis) and `subtitle()`;
  - with more than one carousel slot: ‹ › buttons on hover/focus and a `k / n` chip. The arrows don't select or open the
    tile.
- **`TileArt`:** a composite is a CSS 2×2 grid, and a pic is one `<img>` (`object-fit: contain`, `draggable="false"`).
  An id with no url yet shows a pulsing skeleton. A `null` url, or no slot, shows `KindIcon`.
- **`KindIcon`:** inline stroke SVGs: folder (volume, collection, group, folder, variant, part, images, docs), model
  (hex box), archive (box), mesh, image, other file, and a **box with a padlock** for locked archives. Locked tiles are
  tinted and never open.
- **`TagDots`:** 8 px dots, one per tag kind, in the lower-right of the art (so the row height doesn't change), and at
  the end of a result row. Hovering shows a popover of `TagPill`s grouped by kind. A user-set pill of a generated kind
  has a ✎ mark.

## Input

- Click, Ctrl-click, Shift-click and the rubber band select (see [Multi-select](features.md#multi-select)).
- Double-click or Enter opens a folder. Double-clicking a file opens it in its Windows app.
- Backspace or Alt+Up goes up. Alt+Left goes back (Back restores the scroll position). The mouse Back button (button 3)
  goes up: `mousedown` and `mouseup` are prevented so the WebView doesn't navigate. Button 4 is left alone.
- Ctrl+A selects all shown items. Esc closes the menu, then the completion list, then search, then clears the
  selection.
- Global keys are ignored while focus is in an input.
- A drag starts after 6 px with the primary button held (`dragstart.ts`).

## Pure modules (tested with `node --test`)

| Module | What it holds |
|---|---|
| `api.ts` | Types mirroring the Rust DTOs (camelCase) and `invoke`/`listen` wrappers |
| `grid.ts` | Column count, tile width, visible rows |
| `nav.ts` | Immutable navigation history: `open`, `goTo`, `up`, `goBack`, `settle` (follow a fallback path), `crumbs` |
| `items.ts` | Tile subtitles and the title-row summary (singular/plural) |
| `indexrun.ts`, `extractrun.ts` | Status-bar text for finished index and extract runs ("Stopped after N of M") |
| `carousel.ts` | Slots from an item, stepping, slot keys, and high/low thumbnail requests from the visible range |
| `dragstart.ts` | The 6 px drag threshold |
| `search.ts` | Highlight runs from char marks (`Array.from`), the relative "where" path, the scope label |
| `tags.ts` | Kind order and labels, dot kinds, grouping, `removalAction`, `mergeTags` for a selection |
| `tagcomplete.ts` | `tag:` completion: the term under the caret, matching names, replacement |
| `selection.ts`, `band.ts`, `bandgesture.ts` | Selection model, rubber-band geometry, the band gesture with auto-scroll |

Keep logic out of the `.svelte` files where it can be tested here. Tests import with a `.ts` extension
(`allowImportingTsExtensions`) and use no test framework.

## Theme (`theme.css`)

Colours are CSS custom properties on `:root`, with a light override under `@media (prefers-color-scheme: light)`. The
scheme follows the OS.

| Token | Dark | Light |
|---|---|---|
| `--bg` | `#15171a` | `#f6f7f9` |
| `--bar` | `#1b1e22` | `#ffffff` |
| `--line` | `#2c3137` | `#dde1e6` |
| `--ctl` / `--ctl-line` | `#22262b` / `#343a42` | `#eef0f3` / `#cdd2d9` |
| `--text` / `--muted` | `#e8eaed` / `#9aa1ab` | `#1b1e22` / `#5b636d` |
| `--accent` / `--on-accent` | `#5cc8d6` / `#0e2226` | `#0e7c8a` / `#ffffff` |
| `--sel-bg` / `--sel-line` | `#1c2a2d` / `#3e8f99` | `#e1f3f5` / `#0e7c8a` |

There are more tokens (`--ctl-hover`, `--text-strong`, `--faint`, `--accent-hover`, `--tile-hover`, `--art`,
`--art-ink`, `--chip`, `--chip-text`, `--warn`, `--error`) in the file. Each tag kind has `--tag-<kind>` and
`--tag-<kind>-text` in both schemes: user green, support yellow, scale blue, creator red, edition purple. Use tokens,
never literal colours, and check contrast in both schemes. Focus rings are `2px solid var(--accent)`.

**Fonts:** `"IBM Plex Sans", "Segoe UI", system-ui, sans-serif` (and Plex Mono / Cascadia Mono). Nothing is fetched at
runtime.

## Remembered in `localStorage`

Wrap every access in try/catch.

- `results-as-list`: search results as the compact list instead of tiles.
- `tagPanel`: the tag panel is open.

The search bar's mode, case and open state are not remembered.

## Fixed UI text

Some strings are pinned by tests or by the design, so change them with care:

- the panel title `N items`;
- the faded pill tooltip `k of N`;
- the skip hint `N items share a folder with another item and are skipped`;
- the footer `N selected`;
- the picker title `Extract N items to`;
- `showing first 1000`.
