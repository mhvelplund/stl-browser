# The virtual folder tree

This page describes how the index is shown as folders in the two views. **Models** is the resolver's tree, and **Files**
is the real disk and archive layout. The builders are `crates/stl-core/src/vtree/{models,files}.rs`, fed with rows by
`crates/stl-index/src/views.rs`. Both views follow the naming rules at the end.

## Resolved tables

These are written by the resolver and rebuilt on every index, so user data never goes in them.

| table         | role in the tree                                                                                                                       |
|---------------|----------------------------------------------------------------------------------------------------------------------------------------|
| `sources`     | The physical archives and folders on disk. Only needed to extract files, not to build the tree.                                        |
| `entries`     | Every file found, with `chain`, the steps needed to extract it from nested archives. These are the leaf payloads.                      |
| `groups`      | **Folders.** A tree via `parent_id`, per `collection`, at most 5 levels deep.                                                          |
| `models`      | **Model folders**, the main items in the browser. A model with no `group_id` (a collection that is one model) sits in its collection.  |
| `model_files` | **Files inside a model**, each with `kind`, `variant` and `part_group`, linked to `entries`.                                           |
| `assets`      | **Images and docs.** Rows with `model_id` belong to a model. Rows with a null `model_id` are group or collection posters (`group_id`). |

### Columns the browser relies on

- **`collection`**, e.g. `"Sci-Fi Collection Vol.2/06. Robots STC"`. Splitting it on `/` gives the volume and the
  collection.
- **Root group.** Each collection has one group with `parent_id IS NULL` and `path = ''`, named after the collection
  folder. That group *is* the collection folder, so it is not an extra level.
- **`groups.name` / `models.name`** are the cleaned display names. `raw_name` and `path` keep the original text; `path`
  is relative to the collection.
- **`models`:**
  - `breadcrumb`, `catalog_id`, `designer`, `edition`.
  - `variants` and `part_groups` as JSON.
  - `hero_asset_id`, and `hero_fallback_asset_id` (the nearest group poster) for when the hero is null.
- **`model_files.kind`:** model · scene (lys/chitubox/photon) · companion (mtl) · doc · meta · archive · other.
- **`model_files.variant`:** `''` or atom combinations such as `supported`, `supported+lychee`, `supported+32mm`.
- **`assets`:**
  - `role`: preview · thumb (`x.stl.png`) · instructions · doc · poster.
  - `match`: how the image was attached.
  - `rank`: 0 is the hero.

### Numbers for scale (the two real volumes)

- 124 groups: 11 roots, then 61 / 26 / 25 / 1 per level. 120 groups hold models directly, and 28 hold both subgroups and
  models.
- 1,132 models:
  - 352 have more than one variant.
  - 246 have part groups, 43 of them nested (`Soldiers/Pose_1`).
- 41,915 model files: model 37,934 · scene 3,485 · companion 354 · doc 121 · other 21.
- Group sizes vary a lot: Terrain is one flat group of 152 models.
- Model sizes vary too: 254 models have 1–5 files, 442 have 6–20, 366 have 21–100 and 70 have more than 100. The
  largest, Rustopia, has 2,916 files.
- Both views build in under 0.5 s; inner-root volumes share nodes, so the tree is a DAG (walk it iteratively).

## Models view

```text
/<volume>/<collection>/                      ← collection split on '/', root group (path = '')
    <group>/<subgroup>/…                     ← groups via parent_id; group poster = folder thumbnail
        <model name>/                        ← models; hero_asset_id ?? hero_fallback_asset_id = icon
            images/                          ← assets where model_id = m.id, in rank order (thumbs, instructions)
            docs/                            ← model_files kind doc/meta + assets role doc, deduplicated by entry
            <variant>/                       ← model_files.variant ('' → "default"); level skipped if only one variant
                <part_group>/…               ← model_files.part_group split on '/' ('' → files at this level)
                    file.stl / .lys / .obj   ← entries; .mtl shown beside its .obj
```

- **Node kinds:** volume, collection, group, model, images, docs, variant, part, file, image.
- **Variant above part group.** People print one variant at a time, so `supported/arms/…` reads better than
  `arms/supported/…`.
- **Combined variants** become one folder, e.g. `supported · lychee`.
- **Order:** `images`, `docs`, then folders, then files, each alphabetical (case-insensitive).
- **Two different models with the same display name are not merged.** They get their edition in brackets, or else their
  `raw_name`: `archivist zarvos (5e)`, `archivist zarvos (8e)`. Anything still clashing gets a numbered postfix as
  below.
- **`images` appears only if the model has a picture.**
- **Password-protected archives** are padlock items directly in their collection folder.
- **Node keys.** Row ids change on every re-index, so nodes are keyed by stable strings: collections, groups and models
  use `m:<collection>/<path>`; variants `<model key>#<variant>`; parts `<parent key>/<part>`; `images`/`docs`
  `<model key>#images` / `#docs`; leaves use the `f:` key of their file. Selected thumbnails use these keys; tags use
  real-path keys (below).

## Files view

- The tree is the real layout, built from `entries.path` per root. An archive is shown as a folder named after its stem
  (`x.zip/` becomes `x`), with a small format badge. Nested archives work the same way.
- **Password-protected archives** appear as one item with a box-with-padlock icon. They have no contents and can't be
  opened.
- Nodes are keyed by `f:<source path>|<inner path>`; disk folders by `f:<folder path>|`.
- Each node keeps `label`, its name as given with its case, which Files-view search matches.
- A nested archive that wasn't expanded (past the depth limit, or unreadable) stays a file node named `x.zip`.

## Naming rules (both views)

- **All names are shown in lowercase.** The real paths keep their case for hovering and extraction. They use the
  disk's own separators (`folder_real`): `\` below a Windows path, `/` below a Unix path (macOS), then `!` and `/`
  inside archives.
- **Folders that overlap merge their contents.** This covers names differing only in case, and an added folder that sits
  inside another added folder:
  - Each source is indexed once, under the outermost root.
  - The inner root still appears as its own volume, and its contents merge with the same folders in the outer one.
- **Files whose lowercase names collide get a numbered postfix before the extension.** They are numbered in a stable
  order (by real path), starting at `_1`: `ORC.STL` and `Orc.stl` become `orc_1.stl` and `orc_2.stl`.
- **Merge or number, by node kind:**
  - Folders merge only when they are the same folder. That means the same real path ignoring case, or an inner root.
  - In the Models view, variant and part folders also merge by name inside one model.
  - Anything else that clashes is numbered. For folders the postfix goes at the end: `X.zip`, `x.rar` and a folder `x/`
    side by side become `x_1`, `x_2`, `x_3`.
  - Groups and models never merge. Clashing models first get the edition or raw name in brackets, then `_N`.
- **An inner root's volume** shows the children of the node whose real path is that root's folder. In the Models view,
  if the resolver collapsed that folder, it lists the topmost nodes below it instead.
- **Hovering over any element shows its real path.**
  - Windows separators are used up to the first archive, then `/` inside archives, with `!` at each archive boundary:
    `D:\Models\Vol.1\01. Studio Collection\Grimlax.rar!Grimlax/842_Spiderwalker.zip!842_Spiderwalker_Supported/body.stl`.
  - A merged folder lists each of its real paths, one per line.
  - A folder that has no real path of its own (a model, group, variant, part, `images` or `docs`) shows the common
    folder of what it holds. For a model, that is the common folder of its files: pictures attached from elsewhere don't
    widen it.

## Tag keys

- **User tags and hides are keyed by real path**, not by the `m:`/`f:` node keys: `r:` + the node's real path,
  lowercased, e.g. `r:d:\models\vol.1\robots.zip!legion/`. The same thing therefore has the same key in both views, and
  the key survives adding an outer root.
- **A merged folder** is tagged under every one of its real paths, and reading takes the union.
- **Shadowed subfolders follow their owner.** A node whose real paths are all among its parent's (`images`, `docs`, a
  logical variant, a part folder, a model of loose files directly in its group's folder) is shadowed: the owner is the
  parent, or the parent's owner when the parent is shadowed too. The user tags and hides of the shared folder belong to
  the owner: the shadowed node shows only its generated tags, and tag edits on it are refused with a pointer to the
  owner. A wrong generated tag on a shadowed node therefore can't be hidden. Siblings whose shared folder isn't their
  parent's path (two models from loose files in one folder below a group with its own folder) still share user tags and
  hides. Generated tags are given per node and are never shared.
- **Generated tags** (support, scale, creator, edition) are derived from the resolved tables when the views are built
  and are not stored. A user tag on a folder counts for everything below it in search, but only the tagged item shows
  dots and pills.

## Oddities that will show up as strange folders

- **`stl BM019` is a model inside group `BM019`** (`Blue Makers/BM019/stl_BM019`). The wrapper folder wasn't collapsed
  into the model, so the tree shows `rm019/stl rm019/`.
- **A part group named `images` holds an `.obj`** (`images/large_preview_Turretleft_fixed.obj`), probably a Thingiverse
  export where a preview mesh landed in `images/`.
