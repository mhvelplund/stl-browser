# Milestone 8: tags — design

## Context

Milestone 8 ("Tags and filters") is next. Only the empty `tags` / `node_tags` tables exist. The user wants:

- **Their own labels on things with odd names.** Many models are renamed for copyright reasons, so the user wants to tag
  the "robot legion" folder `tinmen` and find it by that name.
- **Metadata as tags.** The index already parses support state, scale, edition and designer out of names. The user
  wants to see that and search on it, for example to find the supported or unsupported files.
- **Dots and pills.** A tagged tile shows one coloured dot per tag kind in the lower-right corner. Hovering the dots
  shows all the tags as coloured pills.
- **Search.** `tag:xxx` and `tag:"x y"` terms are requirements: every result must have all of the listed tags.
- **Storage.** Tags are stored in the index DB.

This design was approved on 2026-10-31. The implementation plan follows in `docs/superpowers/plans/`.

## Decided by the user

- **Two families of tags:**
  - *User tags* are free-form labels such as `tinmen`. They are **green**.
  - *Generated tags* come from the index, and each kind has its own bright colour:
    - **support** (supported, unsupported, lychee, chitubox): **yellow**
    - **scale** (`25mm`, `32mm`…): **blue**
    - **creator** (the parser's "designer" handle): **red**
    - **edition** (`5E`)
- **The user can set support, scale and creator values themselves.**
  - A value they set **replaces** the parsed values of that kind on that item.
  - It keeps the kind's colour (a user-set scale is still blue), with a small mark on the pill showing that the user
    set it.
- **A wrong generated tag can be hidden.** Its × in the panel hides it for that item. The hide is saved as user data
  and survives rescans.
- **Tags apply in both views.** A tag belongs to the real file or folder, so a tag added in Models also shows on the
  same thing in Files.
- **Inherited in search only.** A folder's tags count for everything below it when searching. Only the item that was
  tagged shows the dots and pills.
- **Topmost match.** A search made only of tag terms (`tag:tinmen`) lists the topmost matching items, such as the
  "robot legion" folder, and not their contents. Once a name term is added (`tag:tinmen warrior`), items inside a
  tagged folder match too.
- **Editing happens in a side panel** beside the grid, for the selected tile. Tagging many tiles at once is not part of
  this milestone.

## Design

### Identity: tags keyed by real path

- **The tag key** is `r:` + the node's real path, lowercased, e.g. `r:d:\models\vol.1\robots.zip!legion/`. Real paths
  are already built for hovering (`vtree/mod.rs` `archive_real`, `folder_real`, `common_real`).
  - Lowercasing follows the view rule that folders differing only in case are the same folder.
- **A merged folder** (several real paths) is tagged under every one of its real paths. Reading takes the union.
- **Why real paths, not the `m:`/`f:` node keys:**
  - The same thing gets the same key in both views. Models-view leaves already share `f:` keys with the Files view, but
    models, groups and variants only match Files folders by real path.
  - The keys survive adding an outer root, which changes `m:` keys (a deferred issue from milestone 3).
- **A known limit, my call:** a Models node whose real path is shared with another node shares its *user tags and
  hides* with that node. For example:
  - two models resolved from loose files in one folder both have that folder as their real path;
  - a model whose files all sit in one `supported/` folder has the same real path as that variant folder.

  Write this down in `docs/index.md` and don't work around it. Generated tags don't have this problem; see below.

### Data (schema v6, through `MIGRATIONS` in `crates/stl-index/src/db.rs`)

Drop the unused v1 `tags` / `node_tags` tables (they were never written) and create:

```sql
CREATE TABLE tags(id INTEGER PRIMARY KEY, kind TEXT NOT NULL, name TEXT NOT NULL, UNIQUE(kind, name));
-- kind: 'user' | 'support' | 'scale' | 'creator'      (user-set values only)
CREATE TABLE node_tags(tag_key TEXT NOT NULL, tag_id INTEGER NOT NULL REFERENCES tags(id) ON DELETE CASCADE,
                       PRIMARY KEY(tag_key, tag_id));
CREATE TABLE hidden_tags(tag_key TEXT NOT NULL, kind TEXT NOT NULL, name TEXT NOT NULL,
                         PRIMARY KEY(tag_key, kind, name));
```

- **All three tables are user data.** They are never rebuilt, and rows for paths that vanish are kept, the same as
  `selected_thumb`.
- **Bump `SCHEMA_VERSION` to 6** and update `crates/stl-index/tests/db.rs`.
- **A `tags` row with no `node_tags` left is deleted** when its last use is removed, so suggestions stay clean.
- **Tag names are normalised:** trimmed, lowercased, with runs of spaces collapsed. Empty names are refused.
- **Generated tags are not copied into tables.** They are derived from the resolved tables (`models.variants`,
  `models.designer` / `edition`, `groups.attrs`, `model_files.variant`) when the views are built. Those tables are
  already in the index DB.

### Generated tags (`stl-index::views`, cached with the trees)

When `build_views` runs, also build `GenTags: HashMap<tag_key, Vec<Tag>>` and keep it in `Views`. It is rebuilt only
after an index run.

- **Variant folders** get the atoms of their label: `supported+25mm` gives support `supported` and scale `25mm`.
  Support words are the `SUPPORT_ORDER` list without `finished`. Scale atoms end in `mm`.
- **Files** get the atoms of their `model_files.variant`.
- **Models** get creator and edition from their columns, and the support and scale atoms that all of their variants
  agree on.
- **Supported and unsupported exclude each other** *(the user's rule)*:
  - An item that would get both has an error in its metadata, so it gets neither.
  - So a model with a supported and an unsupported variant has no support tag. Its variant folders and files carry
    their own, and `tag:unsupported arm` finds only the arms in the unsupported variant.
  - `lychee` and `chitubox` are file formats, so they don't conflict with either.
  - *My call:* the same rule for scale. An item whose variants have different scales gets no scale tag.
- **Groups** get creator and edition from `groups.attrs` (read the JSON; `GroupRow` doesn't load it today).
- **Generated tags are given per node in the Models view**, by node id, not by real path. A model therefore doesn't take
  on the tags of a variant folder that has the same real path. User tags and hides still use real-path keys.
- **Files-view folders** take the generated tags of the Models nodes with the same real path. When several match, they
  get the tags those nodes have in common (my call). Files-view leaves share `f:` keys with Models leaves, so they take
  the file's tags directly.

**Effective tags of a node** = generated − hidden − (generated kinds overridden by a user-set value of that kind) +
user-set. Each tag is `{ kind, name, by_user }`.

### Search (`stl_core::vtree::search` + `browse::search`)

- **Query parsing**, in a new pure function in `stl-core` with unit tests:
  - Split out `tag:word` and `tag:"words with spaces"` terms. A tag name is normalised like a stored one.
  - Join what is left with single spaces. That is the name query, matched as today (Free/Word/Regexp, Aa).
  - In Regexp mode, `tag:` terms are split out as well (my call). The UI hint says so.
- **Matching:**
  - A tag term matches a tag of **any kind** by exact name, case-insensitively. So `tag:supported` finds generated and
    user-set support tags alike, and `tag:tinmen` finds the user tag.
  - Hidden tags never match.
- **Inheritance:** the walk carries the effective tags of all ancestors from the tree root (not only from the search
  scope). A node passes when its own tags plus its ancestors' tags cover every tag term.
- **A value the user sets overrides everything below it** *(the user's choice)*. When an ancestor has a user-set
  support, scale or creator value, the generated tags of that kind below it stop matching in search. For example, a
  model set to `32mm` means `tag:25mm` no longer finds its variant folders or files. Those items still show their own
  generated dots. The panel lists the override under "From folders above".
- **Searching inside a folder that already has the tag** (e.g. `tag:tinmen` inside "robot legion") returns its
  direct children as the topmost hits (my call). Search never returns the scope folder itself.
- **Results:**
  - With tag terms only: a passing node is a hit, and the walk does not go below it (topmost match). An empty name
    query with tag terms is now a valid search. Today an empty query returns nothing, and it still will when there are
    no tag terms.
  - With a name term as well: a hit must match the name and pass the tags. The walk descends as today.
  - The existing `visible` filter and the 1000-hit cap still apply.

### Backend API (`app/src-tauri`)

- **`Item` gets `tags: Vec<TagView>`** with `{ kind, name, by_user }`: the node's own effective tags, with nothing
  inherited. Search hits get it through the flattened `Item`. User tags and hides are loaded per call, as
  `selected_thumbs` is (`commands.rs:47`), so an edit needs no tree rebuild.
- **New commands:**
  - `node_tags(view, path, name)` returns the item's own tags, its inherited tags (with the folder each one comes
    from) and its hidden generated tags.
  - `add_tag(view, path, name, kind, tag)` adds a user tag or a user-set support, scale or creator value.
  - `remove_tag(view, path, name, kind, tag)` removes a user tag or a user-set value.
  - `hide_tag(...)` / `unhide_tag(...)` hide or restore a generated tag.
  - `all_tags()` returns the known names by kind, for suggestions (user rows plus the distinct generated names).
- **Writes go to every real path** of the node. Editing during an index run is allowed, because the keys are paths,
  not entry ids.

### UI (`app/src`)

- **Colour tokens** go in `theme.css`, for both the dark and the light scheme:
  - `--tag-user` (green), `--tag-support` (yellow), `--tag-scale` (blue), `--tag-creator` (red),
    `--tag-edition` (purple, my call: the primaries are used up).
  - Each needs readable pill text on top (`--tag-*-text`).
- **Dots on tiles** (`Tile.svelte`): a row of 8 px dots inside `.art`, in the lower-right corner, one per kind the item
  has, in the order user, support, scale, creator, edition.
  - Keeping them inside `.art` leaves the grid row height (`TileGrid` `TEXT = 56`) unchanged.
  - Hovering the dots shows a popover with every tag as a coloured pill. A user-set pill of a generated kind has a small
    mark (my call: a pencil glyph).
  - The popover replaces the `title` tooltip while the pointer is over the dots.
  - `ResultList.svelte` shows the same dots at the end of each row.
- **The side panel** (new `TagPanel.svelte`):
  - A tag button in the toolbar shows or hides it, and the choice is remembered in `localStorage`, like the list
    toggle.
  - `main` becomes a row with the grid and an `<aside>` of about 260 px. The grid reflows by itself, since it binds
    `clientWidth`.
  - It shows the selected tile, or the open folder when nothing is selected:
    - its name;
    - its own tags as pills grouped by kind, each with ×;
    - an add field with suggestions from `all_tags()` and a kind picker (user, support, scale, creator; user by
      default);
    - "From folders above": inherited tags, read-only, with where they come from;
    - "Hidden": hidden generated tags, each with a restore button.
  - × on a user tag removes it, and × on a generated tag hides it. After an edit, the listing and any open search are
    reloaded.
  - Locked items can be tagged too; they have real paths.
- **The search bar** placeholder becomes "Search names, tag:name". The tag-only search shows results the same way as
  today.
  - Added after M8: typing `tag:` plus one character opens a list of the known tag names with that prefix, narrowed
    as the user types. Up/Down move the selection, Tab completes the selected name, a click completes the clicked one.

### Out of scope (deferred)

- Tagging many tiles at once (multi-select).
- Renaming, merging or deleting a tag everywhere (a tag manager).
- Filters other than `tag:` search terms (e.g. a "has no picture" filter), and negation (`-tag:x`).
- Catalog id as a tag.
- Showing inherited tags as dots.

## Builder's freedom

- Component structure, the exact popover and panel styling, and the colour values within the hues above (check
  contrast in both schemes).
- Whether `GenTags` lives in `Views` or beside it, and the exact command signatures.
- Test layout, following the existing patterns: `stl-core` unit tests for parsing and tag matching, `stl-index` DB
  tests for the store and the migration, `node --test` for pure UI logic such as dot order and pill grouping.

## Files touched (main ones)

- `crates/stl-index/src/db.rs`, `tests/db.rs`: migration v6.
- New `crates/stl-index/src/tagstore.rs`: the user tag and hide store, reads and writes.
- `crates/stl-index/src/views.rs`: load the variant, creator and edition columns; build `GenTags`.
- `crates/stl-core/src/vtree/search.rs`, plus new `vtree/tags.rs` (query parsing, effective tags, matching).
- `app/src-tauri/src/browse.rs` (`Item.tags`, search with tag terms), `commands.rs` (new commands), `state.rs`.
- `app/src/lib/api.ts`, `Tile.svelte`, `TileGrid.svelte`, `ResultList.svelte`, `SearchBar.svelte`, `theme.css`, new
  `TagPanel.svelte` and `tags.ts` (+ test), `app/src/routes/+page.svelte`, `Toolbar.svelte`.
- `docs/index.md`: status, decisions, deferred items. `docs/virtual-tree.md`: tag keys.

## Verification

- `cargo test --workspace`, `npm --prefix app test`, `npm --prefix app run check`.
- On a scratch `STL_BROWSER_HOME` with Vol.1 indexed, drive `tauri dev` over CDP (port 9222):
  - Tag a model folder `tinmen` in Models. It shows a green dot. Switch to Files: the same folder shows the dot.
  - `tag:tinmen` lists only that folder. `tag:tinmen` plus a part name lists files inside it.
  - `tag:supported` lists supported variant folders, not models that have both kinds of variant. Hide `supported` on
    one: it drops out of that search. Restore it.
  - `tag:unsupported` plus a part name finds only parts in unsupported variants.
  - Set scale `32mm` on a model with a parsed `25mm`: the pill shows `32mm` in blue with the user mark, and
    `tag:25mm` no longer finds it or anything inside it.
  - Rescan: all user tags and hides survive. Restart the app: they are still there.
  - Hover the dots: the popover shows pills in the right colours, in both the light and the dark scheme.
