# Multi-select: bulk tagging and multi-item drag-out

## Context

Today the browser selects one item at a time: one tile or one search hit. Tagging a batch of models means repeating
the same edit per item, and dragging several models to Explorer means one drag each. The user wants Explorer-style
multi-select so that a selection can be tagged in bulk or dragged out (or extracted) in one go, in folder listings and
in search results alike.

Outcome: select with click, Ctrl-click, Shift-click and a rubber band; the tag panel shows the selection's tags (shared
ones solid, partial ones faded) and every edit applies to the whole selection; a drag or Extract to… takes the whole
selection.

Approved 2026-10-31. The plan is `docs/superpowers/plans/2026-10-31-multi-select.md`.

## Design

### Goals

- Explorer-style selection in the tile grid (folder listings and grid search results) and in the result list.
- Bulk tag editing with a merged view of the selection's tags.
- Drag-out and Extract to… of the whole selection, including mixed folders from search results.

### Non-goals

- No keyboard-only selection moves (arrow keys with Shift). Only Ctrl+A and Esc (below).
- No bulk "Open", "Rescan" or "Use as folder thumbnail"; those stay single-item.
- No change to tag storage, search semantics or generated-tag rules.

### Anti-goals

- A bulk edit must never touch items outside the selection, nor silently drop the edit for selected items that can take
  it.
- Selecting must not break existing single-item behaviour: plain click, double-click open, the 6 px drag gesture, the
  carousel arrows and right-click all work as before for one item.

### Selection (user-decided: Ctrl-click, Shift-click, rubber band)

- **Click** selects just that item and sets the anchor.
- **Ctrl-click** toggles the item and moves the anchor to it.
- **Shift-click** selects the range anchor..item in display order (replacing the selection);
  **Ctrl+Shift-click** adds that range to the selection.
- **Rubber band:** pressing the primary button on empty space in the grid or list and dragging draws a box; every item
  whose tile/row rectangle intersects the box is selected (Ctrl held: added to the selection made before the band
  started). The grid is virtualized, so the hit test is computed from the grid geometry (`grid.ts` column/row maths and
  item index), not from DOM elements, and the band keeps working while it auto-scrolls near the top/bottom edge.
- **Click on empty space** (without a band) clears the selection.
- *My call:* **Ctrl+A** selects everything shown; **Esc** clears the selection (only when focus is not in an input, so
  the search bar keeps its Esc).
- *My call:* the selection clears on navigation to another folder, view switch, and when the search results change;
  a listing reload after a tag edit keeps it (dropping ids that no longer exist), as `keepSelection` does today.
- Selection state is a pure module (`app/src/lib/selection.ts` with tests): a set of ids plus an anchor, with
  `click`, `toggle`, `range`, `band`, `all`, `clear`, `prune(ids)`. Ids are `item.name` for listings and `hitId(h)` for
  hits, as now.
- Right-click on an unselected item selects only it (as now); right-click on a selected item keeps the selection.

### Tag panel with a selection

- **One item selected:** unchanged from today (own tags, From folders above, Hidden, owner hint).
- **Several selected:** the title reads `N items`. The panel shows the union of the items' own effective tags,
  grouped by kind as today:
  - a tag every taggable item has is a normal pill;
  - a tag only some have is a **faded** pill (reduced opacity), with a tooltip `k of N`.
  - "From folders above" and "Hidden" are not shown (user-decided).
- **Add** (any settable kind: user, support, scale, creator) adds the value to every selected item. For single-value
  kinds the new value replaces each item's old one, as the single add does today.
- **× on a solid pill** removes it from every item: per item, a user-set value is removed, a generated one is hidden
  (today's `removalAction` rule, decided per item, since the same tag can be user-set on one item and generated on
  another).
- **× on a faded pill** removes/hides it on only the items that have it.
- **Click on a faded pill** adds it to the rest of the selection, so it turns solid (user-decided, "edit a faded tag
  adds the result to all").
- **Shadowed items** (those that share their folder with an owner and refuse tag edits today) are skipped in bulk edits
  and don't count towards the solid/faded split; the panel shows `N items share a folder with another item and are
  skipped` (user-decided). If every selected item is shadowed, the panel shows the same hint and disables editing.
- An item that can't be tagged at all (no tag keys, e.g. the library root) is treated like a shadowed one.

### Drag-out and Extract to…

- Dragging a selected item drags the whole selection; dragging an unselected item selects only it and drags it
  (Explorer behaviour).
- Right-click → **Extract to…** extracts the whole selection when the right-clicked item is in it; the picker title
  reads `Extract N items to`. **Open**, **Rescan** and **Use as folder thumbnail** only appear for a single selection.
- Items from different folders (search results) land side by side at the top of the drop/target folder; clashing
  names get the existing unique-name postfix from `export_plan`.
- *My call:* if a selected item lies inside another selected item (possible in search results), only the outer one
  is exported, so files are not written twice.
- Locked items in the selection are skipped and counted in the existing "left out" notice; a selection of only locked
  items doesn't start a drag.

### Status bar

- With several selected, the footer shows `N selected` in place of the single item's real path.

### Backend changes

- **Targets instead of (path, names):** `begin_drag` and `extract_to` take `targets: Vec<Target>` where
  `Target { path: Vec<String>, name: String }`. `nodes_for` (`app/src-tauri/src/export.rs`) resolves each target with
  the same lookup it does now, then drops targets whose node has an ancestor among the others. `export_plan`
  (`crates/stl-core/src/vtree/export.rs`) already takes several root ids and makes sibling names unique, so it is
  reused unchanged. The single-folder callers become one-target or same-path targets.
- **`nodes_tags(view, targets)`** (new, `browse.rs` + `commands.rs`): for each target, its own effective tags and whether
  it is editable (not shadowed, has tag keys). The merge into solid/faded with counts is a pure function in
  `app/src/lib/tags.ts` (tested) so the backend stays a per-node lookup reusing `browse::node_tags` / `locate_node`.
- **`edit_tags(view, targets, op, kind, tag)`** (new): `op` is `add`, `remove` (per item: remove the user-set value
  and/or hide the generated one, skip if absent) or `fill` (per item lacking it: unhide if hidden, else add; refused
  for `edition`); see "Edge cases settled after review". It reuses
  `tag_target` and `tagstore::{add, remove, hide}`, skips shadowed/untaggable targets instead of failing, runs in one
  DB transaction, and returns how many items were changed and skipped. Existing single-node commands stay for the
  single-item panel.

### UI changes (files)

- `app/src/lib/selection.ts` (+ test): selection model.
- `app/src/lib/band.ts` (+ test): rubber-band rectangle to item index set for the grid geometry and for list rows.
- `TileGrid.svelte`, `Tile.svelte`, `ResultList.svelte`: `selected: Set<string>`, click handlers that pass modifier
  keys, band overlay, `onDragOut` drags the selection.
- `TagPanel.svelte` + `TagPill.svelte`: multi-target mode, `faded` and clickable pill props.
- `+page.svelte`: one selection state per mode (listing / results), derived tag targets, multi-target drag/extract,
  menu rules, keyboard, footer.
- `api.ts`: new command wrappers and `Target`.

### Edge cases settled after review (my calls)

- **Selection survives an edit's refresh, in results too.** A tag edit reloads the listing and re-runs the search; that
  refresh keeps the selection and prunes ids that are gone (a `tag:` search can drop edited items). Only a new query,
  scope or mode, navigation or a view switch clears it. `+page.svelte` tells a refresh from a new search with a
  `keepSelection` flag, as `load` already does.
- **Merging matches on (kind, name).** An item counts as having a tag when any of its effective tags has that kind and
  name, user-set or generated.
- **× per item does whatever makes the tag go away:** delete the user-set value if there is one, and hide the generated
  one if there is one (both, when both exist).
- **Clicking a faded pill ("fill"), per item lacking the tag:**
  - if the item has that generated tag hidden, unhide it;
  - otherwise add it as a user-set value (single-value kinds replace the item's old value, like any add);
  - `edition` tags can't be user-set, so a faded edition pill isn't clickable (its × still works).
- **A shadowed item whose owner is also selected** is covered by the owner: it isn't counted as skipped and doesn't
  trigger the hint. Only shadowed items whose owner is *not* in the selection are skipped and counted.
- **Rubber band in the result list** starts only from empty space (below the last row, or the list's side padding).
  A press on a row keeps today's click/drag behaviour, and Ctrl/Shift-click cover row selection.

### Left to the builder

- Exact faded styling (opacity value), band colours (reuse theme tokens), auto-scroll speed.
- Whether the single-item panel is the multi-item code path with N = 1, as long as single-item behaviour is unchanged.

## Docs

- Add a "Multi-select (milestone 9)" section and status row to `docs/index.md`, update the "Editing is in a side panel
  … one at a time" decision, and note the `Target` shape for `begin_drag`/`extract_to`.

## Verification

- `cargo test --workspace` (new tests: multi-target `nodes_for` with nesting removed, mixed-folder `build_export`
  names, `edit_tags` add/remove/fill with shadowed and absent items, `nodes_tags`).
- `npm --prefix app test` (selection, band, tag merge) and `npm --prefix app run check`.
- `npm --prefix app run tauri dev` with a scratch `STL_BROWSER_HOME`, by hand:
  - Ctrl/Shift/Ctrl+Shift-click, band select with and without Ctrl, band auto-scroll in a long folder, Ctrl+A, Esc;
  - tag 3 models where one already has a tag: faded pill shows `1 of 3`, clicking it makes it solid, × removes from all;
    × on a faded generated tag hides it only where present; a shadowed item in the selection is skipped with the hint;
  - drag 3 models to an Explorer window, and 2 hits from different folders with the same name (postfix applied);
  - Extract to… on a selection.
- `STL_BROWSER_DRAG_CHECK` real-data drag test still passes after the `Target` change.
