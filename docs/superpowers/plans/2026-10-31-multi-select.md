# Multi-select Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or
> superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Explorer-style multi-select (click, Ctrl/Shift-click, rubber band) in the tile grid and the search results, with
bulk tag editing (solid and faded pills) and drag-out / Extract to… of the whole selection.

**Architecture:** The backend gains a `Target { path, name }` list for `begin_drag` / `extract_to`, and two new commands,
`nodes_tags` (per-target tags plus editability) and `edit_tags` (`add` / `remove` / `fill` over many targets, skipping
shadowed ones). The UI keeps selection, rubber-band hit testing and the solid/faded tag merge in pure, tested TS
modules; the Svelte components only wire events to them.

**Tech Stack:** Rust (Tauri 2 commands, `stl-core` / `stl-index`), Svelte 5 + TypeScript, `node --test`.

**Spec:** `docs/superpowers/specs/2026-10-31-multi-select-design.md`

## Global Constraints

- Branch `multi-select` (already created from a fresh `main`); land through a PR.
- Commit types only `feat`, `fix`, `chore(<scope>)`; don't bump versions or touch `CHANGELOG.md`.
- No schema change (user data and tag storage stay as they are).
- No real names in tests or docs; use the fixture's `orc` / `elf` / `vol` / `coll`.
- Write source files with the editor tools, not shell heredocs.
- Single-item behaviour (plain click, double-click open, 6 px drag, carousel arrows, right-click, the one-item tag panel
  with "From folders above", "Hidden" and the owner hint) must stay unchanged.
- Copy fixed by the spec: panel title `N items`; faded tooltip `k of N`; skip hint
  `N items share a folder with another item and are skipped`; footer `N selected`; picker title `Extract N items to`.

## Review Focus

1. **A tag search that loses items after an edit** (`tag:wip`, remove `wip` from 3 of 5 selected): the remaining 2 stay
   selected and the panel shows them. Test: `prune` in Task 3 and the refresh path in Task 7.
2. **Selecting a folder and a file inside it** in search results, then dragging: the file is written once, under the
   folder. Test: `targets_drop_nested` in Task 1.
3. **Two hits with the same name from different folders** dropped together: both land, the second renamed. Test:
   `build_export_mixed_folders_renames_clashes` in Task 1.
4. **Mixed user-set and generated copies of one tag** across the selection (× must clear both). Test:
   `edit_tags_remove_clears_set_and_generated` in Task 2.
5. **Rubber band in a scrolled, virtualized grid** (tiles off-screen not in the DOM): the band selects by geometry
   including rows scrolled past. Test: `band covers rows scrolled past` in Task 4.

---

### Task 1: Multi-target drag and extract

**Files:**
- Modify: `app/src-tauri/src/export.rs` (`nodes_for`, `build_export`, `begin_drag`, `extract_to`, tests)
- Modify: `app/src-tauri/src/browse.rs` (add `Target`)
- Modify: `app/src/lib/api.ts`, `app/src/routes/+page.svelte` (callers pass one target)
- Modify: `drag_real_item_to_shell_folder` in `app/src-tauri/src/export.rs` (builds one `Target` from `STL_BROWSER_DRAG_CHECK`)

**Interfaces:**
- Produces (Rust, `browse.rs`):
  `#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)] #[serde(rename_all = "camelCase")] pub struct Target { pub path: Vec<String>, pub name: String }`
  (`name == ""` means the folder at `path` itself, as in `locate_node`).
- Produces (Rust, `export.rs`): `pub fn nodes_for(views: &Views, view: View, targets: &[Target]) -> Result<Vec<NodeId>, String>`;
  `pub fn build_export(state: &AppState, view: View, targets: &[Target]) -> Result<(Vec<ExportFile>, u32), String>`;
  commands `begin_drag(view, targets: Vec<Target>)` and `extract_to(view, targets: Vec<Target>, dest)`.
- Produces (TS, `api.ts`): `export type Target = { path: string[]; name: string }`;
  `beginDrag(view: View, targets: Target[])`, `extractTo(view: View, targets: Target[], dest: string)`.

- [ ] **Step 1: Write the failing tests** (in `export.rs` tests; rework the two existing `nodes_for_*` tests and every
  `build_export` call to the target form, with a local `fn tg(path: &[&str], name: &str) -> Target`)
  - `nodes_for_finds_targets_in_different_folders`: Files view, `[tg(["vol","coll"],"Orc"), tg(["vol","coll","elf"],"elf.stl")]`
    → node names `["orc", "elf.stl"]`.
  - `nodes_for_rejects_stale_names`: `tg(["vol","coll"],"gone")` and `tg(["vol","nope"],"orc")` are `Err`.
  - `targets_drop_nested`: Files view, `[tg(["vol","coll","orc"],"orc.stl"), tg(["vol","coll"],"orc")]` → one id, the
    `orc` folder; and `build_export` of the same gives `rels == ["orc", "orc/notes.txt", "orc/orc.jpg", "orc/orc.stl"]`.
  - `build_export_mixed_folders_renames_clashes`: write `lib/Vol/Coll/Elf/orc.stl`, re-index (as
    `shadowed_folders_name_their_owner` in `browse.rs` does), Files view,
    `[tg(["vol","coll","orc"],"orc.stl"), tg(["vol","coll","elf"],"orc.stl")]` → two files, first `rel == "orc.stl"`,
    second a different name ending in `.stl`.
- [ ] **Step 2: Run** `cargo test -p app --lib export::` — expected: compile errors / FAIL.
- [ ] **Step 3: Implement.** `nodes_for` resolves each target with `browse::locate_node(views, view, &t.path, &t.name)`
  (error text unchanged: `<name> no longer exists`), dedupes ids, then drops any id that has another selected id among
  its `ancestors`. Pass the result to the unchanged `export_plan`. `begin_drag` / `extract_to` take `targets`.
  Update `api.ts`, and the four call sites in `+page.svelte` (`dragFrom`, `extract`) to pass `[{ path, name }]`.
- [ ] **Step 4: Run** `cargo test -p app --lib` and `npm --prefix app run check` — expected: PASS, 0 errors.
- [ ] **Step 5: Commit** `feat: drag and extract take targets from any folder`

### Task 2: Bulk tag commands

**Files:**
- Modify: `app/src-tauri/src/commands.rs` (`nodes_tags`, `edit_tags`), `app/src-tauri/src/lib.rs` (register),
  `app/src-tauri/src/browse.rs` (tests next to the existing tag tests), `app/src/lib/api.ts`

**Interfaces:**
- Consumes: `Target` (Task 1); `browse::locate_node`, `UserTags::{effective, hidden_of}`, `tagstore::{add, remove, hide, unhide}`.
- Produces (Rust):
  - `#[serde(rename_all = "camelCase")] pub struct TargetTags { pub own: Vec<TagView>, pub hidden: Vec<TagView>, pub editable: bool, pub covered: bool }`
    — `editable`: has tag keys and no `tag_owner`; `covered`: not editable because its `tag_owner` is one of the targets.
  - `pub fn nodes_tags_impl(state: &AppState, view: View, targets: Vec<Target>) -> Result<Vec<TargetTags>, String>` (same
    order as `targets`) and command `nodes_tags`.
  - `#[serde(rename_all = "lowercase")] pub enum EditOp { Add, Remove, Fill }`;
    `#[serde(rename_all = "camelCase")] pub struct EditResult { pub changed: u32, pub skipped: u32 }`;
    `pub fn edit_tags_impl(state: &AppState, view: View, targets: Vec<Target>, op: EditOp, kind: String, tag: String) -> Result<EditResult, String>`
    and command `edit_tags`.
- Produces (TS): `TargetTags`, `EditOp = "add" | "remove" | "fill"`, `EditResult`; `nodesTags(view, targets)`,
  `editTags(view, targets, op, kind, tag)`.

Semantics of `edit_tags` per target (the tag name is normalized with `tags::normalize` first; unknown kind → `Err`):
- Not editable → skip; counted in `skipped` only if not `covered`.
- `add`: `tagstore::add` (single-value kinds replace). `edition` → `Err("edition tags can't be set")` before any write.
- `remove`: if the item has the tag user-set → `tagstore::remove`; if it has it generated and not hidden → `tagstore::hide`;
  both when both; neither → unchanged, not counted.
- `fill`: items that already have it are unchanged; if it is among `hidden_of` → `tagstore::unhide`; else `tagstore::add`.
  `edition` → same `Err` as `add`.
- "Has the tag" means some effective tag with the same `(kind, name)`. `changed` counts targets that got a write.
- Load `UserTags` once, hold the DB lock for the whole call; no single transaction is required (the `tagstore` calls
  each open their own).

- [ ] **Step 1: Write the failing tests** (`browse.rs` tests module, fixture `library`, Models view, targets `orc` and
  `elf` under `["vol","coll"]`)
  - `nodes_tags_reports_each_target`: tag `orc` user `wip`; result `[0].own` contains `wip`, `[1].own` doesn't, both
    `editable`, neither `covered`.
  - `edit_tags_add_tags_all`: `add user Paint` → `changed == 2`, both show `paint`.
  - `edit_tags_fill_adds_where_missing`: `orc` has `wip`; `fill user wip` → `changed == 1`, both have it.
  - `edit_tags_remove_only_where_present`: `orc` has `wip`; `remove user wip` → `changed == 1`, neither has it.
  - `edit_tags_remove_clears_set_and_generated`: give `orc` a generated tag `g` (any generated tag the fixture yields for
    a model, e.g. read it from `nodes_tags`) and also add the same `(kind, name)` user-set; `remove` → it's gone from
    `effective` and appears in `hidden`.
  - `edit_tags_fill_unhides`: hide a generated tag on `orc` only; `fill` it → `orc` has it again and `hidden` is empty;
    no user-set copy was added (`by_user == false`).
  - `edit_tags_skips_shadowed`: with the second picture from `shadowed_folders_name_their_owner`, targets
    `(["vol","coll","orc"],"images")` + `(["vol","coll"],"elf")`: `add` → `changed 1, skipped 1`; with `orc` itself also
    in targets → `skipped 0`, and `nodes_tags` marks `images` `covered`.
  - `edit_tags_refuses_edition`: `add edition x` → `Err`, nothing written.
- [ ] **Step 2: Run** `cargo test -p app --lib edit_tags nodes_tags` — expected: FAIL (not defined).
- [ ] **Step 3: Implement** in `commands.rs`, register both commands in `lib.rs`, add the `api.ts` wrappers and types.
- [ ] **Step 4: Run** `cargo test -p app --lib` — expected: PASS.
- [ ] **Step 5: Commit** `feat: bulk tag commands`

### Task 3: Selection model

**Files:**
- Create: `app/src/lib/selection.ts`, `app/src/lib/selection.test.ts`

**Interfaces:**
- Produces:
  ```ts
  export type Selection = { ids: ReadonlySet<string>; anchor: string | null };
  export const empty: Selection;
  /** `order` is the display order of ids. Plain: just `id`. ctrl: toggle `id`. shift: anchor..id (replacing, or
   *  added with ctrl). The anchor moves to `id` except on shift. */
  export function click(s: Selection, id: string, order: readonly string[], mods: { ctrl: boolean; shift: boolean }): Selection;
  /** The band's ids, or `base` ∪ them with ctrl; anchor unchanged. */
  export function band(base: Selection, hit: readonly string[], ctrl: boolean): Selection;
  export function all(order: readonly string[]): Selection;
  /** Drop ids not in `order`; the anchor too if gone. */
  export function prune(s: Selection, order: readonly string[]): Selection;
  /** Right-click / drag start on `id`: keep `s` if `id` is in it, else just `id`. */
  export function focus(s: Selection, id: string): Selection;
  ```

- [ ] **Step 1: Write the failing tests** (order `["a","b","c","d","e"]`)
  - plain click `c` → `{c}`, anchor `c`; ctrl-click `e` → `{c,e}`, anchor `e`; ctrl-click `c` again → `{e}`.
  - shift-click `d` from anchor `b` → `{b,c,d}`, anchor still `b`; then shift-click `a` → `{a,b}`.
  - ctrl+shift-click `e` with `{a}` anchor `c` → `{a,c,d,e}`.
  - shift-click with no anchor acts like a plain click.
  - `band(base {a}, ["c","d"], false)` → `{c,d}`; with ctrl → `{a,c,d}`.
  - `all` → five ids; `prune({a,c} anchor c, ["a","b"])` → `{a}`, anchor `null`.
  - `focus({a,b}, "b")` keeps `{a,b}`; `focus({a,b}, "c")` → `{c}`.
- [ ] **Step 2: Run** `npm --prefix app test` — expected: FAIL (module missing).
- [ ] **Step 3: Implement** `selection.ts` (immutable: every function returns a new `Selection`).
- [ ] **Step 4: Run** `npm --prefix app test` — expected: PASS.
- [ ] **Step 5: Commit** `feat: selection model`

### Task 4: Rubber-band geometry

**Files:**
- Create: `app/src/lib/band.ts`, `app/src/lib/band.test.ts`

**Interfaces:**
- Produces:
  ```ts
  /** In content coordinates (scrollTop already added). */
  export type Rect = { x0: number; y0: number; x1: number; y1: number };
  export function rect(ax: number, ay: number, bx: number, by: number): Rect; // normalized
  /** Grid tiles: tile i is at col i % cols, row i / cols; left = pad + col*(tile+gap), top = pad + row*rowHeight,
   *  size tile × (rowHeight - gap). Returns the indexes of the tiles the rect intersects, ascending. */
  export function gridHits(r: Rect, n: number, g: { cols: number; tile: number; gap: number; pad: number; rowHeight: number }): number[];
  /** List rows of equal `rowHeight` starting at `top`, full width: indexes intersecting [y0, y1]. */
  export function listHits(r: Rect, n: number, rowHeight: number, top: number): number[];
  ```

- [ ] **Step 1: Write the failing tests** (grid: `cols 3, tile 100, gap 8, pad 20, rowHeight 164`, `n = 10`)
  - a rect inside the gap between tiles 0 and 1 → `[]`.
  - a rect from the middle of tile 0 to the middle of tile 4 → `[0, 1, 3, 4]`.
  - `band covers rows scrolled past`: rect `y0 = 20, y1 = 20 + 3*164` (rows 0–3, far beyond a viewport) across all
    columns → `[0..9]`; a rect past the last tile (row 3, col 2, no tile) → excludes index 11 and anything ≥ `n`.
  - `rect` normalizes a drag up-and-left.
  - `listHits({..y0: 30, y1: 75}, 10, 28, 0)` → `[1, 2]`.
- [ ] **Step 2: Run** `npm --prefix app test` — expected: FAIL.
- [ ] **Step 3: Implement** `band.ts`.
- [ ] **Step 4: Run** `npm --prefix app test` — expected: PASS.
- [ ] **Step 5: Commit** `feat: rubber-band hit testing`

### Task 5: Tag merge

**Files:**
- Modify: `app/src/lib/tags.ts`, `app/src/lib/tags.test.ts`

**Interfaces:**
- Consumes: `TargetTags` (Task 2).
- Produces:
  ```ts
  export type MergedTag = Tag & { count: number; faded: boolean };
  /** Over editable targets only: union by (kind, name); `count` = editable targets having it; `faded` = count < total.
   *  `byUser` is true when any has it user-set. Sorted like `groupByKind`. */
  export function mergeTags(targets: TargetTags[]): { tags: MergedTag[]; total: number; skipped: number };
  ```
  `skipped` counts targets with `!editable && !covered`.

- [ ] **Step 1: Write the failing tests**
  - three editable targets, `wip` on all, `paint` on one → `wip` `count 3, faded false`; `paint` `count 1, faded true`;
    `total 3`.
  - the same `(user, wip)` user-set on one and generated on another → one entry, `byUser true`, `count 2`.
  - a non-editable, non-covered target with `x` → `x` absent, `skipped 1`; a covered one → `skipped 0`.
- [ ] **Step 2: Run** `npm --prefix app test` — expected: FAIL.
- [ ] **Step 3: Implement** `mergeTags`.
- [ ] **Step 4: Run** `npm --prefix app test` — expected: PASS.
- [ ] **Step 5: Commit** `feat: merge tags across a selection`

### Task 6: Multi-select in the grid and the result list

**Files:**
- Modify: `app/src/lib/TileGrid.svelte`, `app/src/lib/Tile.svelte`, `app/src/lib/ResultList.svelte`

**Interfaces:**
- Consumes: `band.ts` (Task 4).
- Produces (props, replacing the single `selected` / `onSelect`):
  - `selected: ReadonlySet<string>`
  - `onSelect: (i: T, mods: { ctrl: boolean; shift: boolean }) => void` — plain/Ctrl/Shift click on a tile or row
    (`ctrlKey || metaKey`, `shiftKey`). Enter calls it with no modifiers, then `onOpen`. Right-click no longer calls
    `onSelect`; the page's `showMenu` applies `focus` (Task 7).
  - `onBand: (ids: string[], ctrl: boolean) => void` — the band's current hit ids, sent on every pointer move while
    the band is drawn (live feedback).
  - `onClear: () => void` — click on empty space without a band.
  - `onDragOut: (i: T) => void` unchanged (the page decides what to drag).

- [ ] **Step 1: Implement the grid band.** In `TileGrid`, a `pointerdown` with button 0 whose target isn't inside a
  `.tile` starts a band (pointer capture on the scroller); positions are content coordinates (`clientY - box.top +
  scrollTop`). Moving < `THRESHOLD` (from `dragstart.ts`) then releasing → `onClear`. Draw the band as an absolutely
  positioned div in the spacer (border `var(--accent)`, fill the same at low alpha; builder picks tokens). While the
  pointer is within 32 px of the top/bottom edge, scroll by a fixed step per animation frame and recompute. Hits come
  from `gridHits` with the grid's `cols, tile, GAP, PAD, rowHeight`, mapped to `idOf(items[i])`.
- [ ] **Step 2: Implement the list band.** Same in `ResultList`, starting only off the rows (below the last row or in
  the list's padding), using `listHits` with the measured row height and list top.
- [ ] **Step 3: Pass modifiers** from `Tile` and `ResultList` clicks to `onSelect`; `selected.has(idOf(item))` drives
  the `selected` class and `aria-pressed`.
- [ ] **Step 4: Run** `npm --prefix app run check` — expected: 0 errors (the page is updated in Task 7; commit both
  together if the check needs it).
- [ ] **Step 5: Commit** with Task 7.

### Task 7: Page wiring: selection, keyboard, drag, extract, menu, footer

**Files:**
- Modify: `app/src/routes/+page.svelte`

**Interfaces:**
- Consumes: Tasks 1, 3, 6.

- [ ] **Step 1: Replace** `selected: string | null` and `selectedHit` with `sel: Selection` (listing) and
  `hitSel: Selection` (results), ids `item.name` and `hitId(h)`. `select(i, mods)` → `click(sel, i.name, order, mods)`;
  `showIn` sets `sel = click(empty, name, …)` so reveal still works. Derive `selectedItems: Item[]` /
  `selectedHits: Hit[]` in display order.
- [ ] **Step 2: Clearing and pruning.** `load(…, keepSelection)` prunes `sel` against the new items instead of clearing
  when `keepSelection`; otherwise `sel = empty`. The search refresh after a tag edit (`tagsChanged`) passes a keep
  flag to `runSearch` that prunes `hitSel`; a new query, scope or mode clears it. View switch and navigation clear.
- [ ] **Step 3: Keyboard** in `onKey` (which already skips inputs): Ctrl+A → `all(order)` of the shown items/hits
  (`preventDefault`); Escape → `empty` (only when nothing else handled it: the menu closes first if open).
- [ ] **Step 4: Drag.** `dragOut(item)` / `dragHit(h)`: `sel = focus(sel, id)`, then `beginDrag(view, targets)` where
  targets are the selected, non-locked items in display order (`{ path: nav.path, name }` or `{ path: h.parent, name }`);
  an all-locked selection doesn't call it. Keep the existing `skipped` notice.
- [ ] **Step 5: Menu.** `showMenu` first does `sel = focus(sel, id)`. With more than one selected: only
  **Extract to…**, picker title `Extract ${n} items to`, `extractTo(view, targets, dest)`; otherwise today's entries.
- [ ] **Step 6: Footer.** With more than one selected, `footerHint` shows `${n} selected` unless a `notice` is set.
- [ ] **Step 7: Tag panel targets.** Derive `tagTargets: Target[]`: the selected items/hits; with no selection in a
  listing, `[{ path: nav.path, name: "" }]` (the open folder, as today); with no selection in results, `[]`. Pass it to
  `TagPanel` (Task 8) along with today's single-target props for the one-item case.
- [ ] **Step 8: Run** `npm --prefix app run check` and `npm --prefix app test` — expected: 0 errors, PASS. Launch
  `npm --prefix app run tauri dev` (scratch `STL_BROWSER_HOME`) and check click / Ctrl / Shift / band / Ctrl+A / Esc,
  dragging three tiles to Explorer, and Extract to… on a selection.
- [ ] **Step 9: Commit** `feat: select several items and drag or extract them together`

### Task 8: Tag panel for a selection

**Files:**
- Modify: `app/src/lib/TagPanel.svelte`, `app/src/lib/TagPill.svelte`

**Interfaces:**
- Consumes: `nodesTags`, `editTags` (Task 2), `mergeTags` (Task 5), `tagTargets` (Task 7).
- Produces: `TagPanel` prop `targets: Target[]`; with `targets.length <= 1` it behaves exactly as today (keep
  `path` / `name` / `title` for that case). `TagPill` gains `faded?: boolean`, `count?: string` (tooltip) and
  `onpick?: (t: Tag) => void` (click on the pill body).

- [ ] **Step 1: TagPill.** `faded` → class with reduced opacity (builder's value); `title={count}`; when `onpick` is set
  the pill body is a button that calls it (the × keeps `stopPropagation`).
- [ ] **Step 2: Multi mode** (`targets.length > 1`): title `${targets.length} items`; load with `nodesTags`, merge with
  `mergeTags`; pills grouped with `groupByKind`, faded ones with `count = "${t.count} of ${total}"` and `onpick` → `fill`
  (not for `edition`); × → `remove`; the add form → `add`. No "From folders above" / "Hidden" sections. When
  `skipped > 0`, show `${skipped} items share a folder with another item and are skipped`; when `total === 0`, show the
  same hint and disable the form and pills. After each edit: `reload`, `loadKnown`, `onchange` (as `edit` does now).
- [ ] **Step 3: Run** `npm --prefix app run check` and `npm --prefix app test` — expected: 0 errors, PASS. In the
  running app: select 3 models where one has `wip` → faded `wip` with tooltip `1 of 3`; click it → solid on all three;
  × → gone from all; × on a faded generated tag hides it only where present; add a scale → all three show it; a
  selection with a shadowed `images` item and no owner shows the skip hint.
- [ ] **Step 4: Commit** `feat: tag several items at once`

### Task 9: Docs and final verification

**Files:**
- Modify: `docs/index.md`, `CLAUDE.md` (the `app/src/lib` module list gains `selection` / `band`)

- [ ] **Step 1:** In `docs/index.md` add milestone row "9. Multi-select: bulk tags, drag and extract of a selection"
  (done), a "Multi-select (milestone 9)" section summarizing selection gestures, the bulk tag rules and `Target`, and
  change the decision "Editing is in a side panel for the selected tile, one at a time" to cover the selection.
  Update the drag-out "Gesture" bullet to `begin_drag(view, targets)`.
- [ ] **Step 2: Run** `cargo test --workspace`, `npm --prefix app test`, `npm --prefix app run check` — all PASS / 0
  errors. Run the opt-in real-data drag test with `STL_BROWSER_DRAG_CHECK` (see CLAUDE.md) — PASS.
- [ ] **Step 3: Commit** `chore(docs): multi-select`
