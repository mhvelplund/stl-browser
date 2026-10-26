# Virtual folder structure over `stl-index.db`

The database already has a natural tree: **volume → collection → groups → model**. Inside each model, the files sort
into **variant → part group → file**. Numbers below are from the current index (both volumes).

## Tables

| table         | rows   | role in a virtual tree                                                                                                |
|---------------|--------|-----------------------------------------------------------------------------------------------------------------------|
| `sources`     | 346    | The physical archives (342) and folders (4) on disk. Only needed to extract files, not to build the tree.             |
| `entries`     | 48,942 | Every file found, with `chain`, the steps needed to extract it from nested archives. These are the leaf payloads.     |
| `groups`      | 124    | **Folders.** A tree via `parent_id`, at most 5 levels deep (11 roots, then 61 / 26 / 25 / 1 per level).               |
| `models`      | 1,132  | **Model folders**, the main items in the browser. Every model has a `group_id`, so none sit outside the tree.         |
| `model_files` | 41,915 | **Files inside a model**, each tagged with `kind`, `variant` and `part_group`.                                        |
| `assets`      | 6,366  | **Images and docs.** Rows with `model_id` belong to a model; rows with `model_id` null are posters for a group (346). |

### Key fields

- **`collection`** is e.g. `"Warhammer 40K Collection Vol.2/06. Necrons STC"`. Splitting it on `/` gives the top two
  levels: volume, then collection.
- **Root group:** each collection has one group with `parent_id IS NULL`, `path = ''` and a name equal to the collection
  folder. That group *is* the collection folder, so don't add it as another level.
- **`groups.name` / `models.name`** are cleaned display names. `raw_name` and `path` keep the original text, and `path`
  is relative to the collection.
- **`models.breadcrumb`** is already the display path of the parent folder
  (`Warhammer 40K Collection Vol.2 / 06. Necrons STC / 04. Fast Attack`).
- **`models.variants`** (JSON) lists the variants the model has. `''` means unspecified. 352 models have more than one.
- **`models.part_groups`** (JSON) lists part groups such as `Arms`, `Heads`, `Soldiers/Pose_1`. 246 models have part
  groups, 43 of them nested (containing `/`).
- **`model_files.kind`**: model 37,934 · scene 3,485 (lys/chitubox/photon) · companion 354 (mtl) · doc 121 · other 21.
- **`model_files.variant`**: `''` 26.7k · supported 8.3k · unsupported 6.0k · lychee 412 · `supported+lychee` 225 ·
  scale combinations such as `supported+32mm`.
- **`assets.role`**: preview 5,655 · thumb 201 (`x.stl.png`) · instructions 38 · doc 126 · poster 346.

### Tree shape

- 120 of the 124 groups hold models directly, and 28 hold both subgroups and models. A normal folder view handles that
  mix without special cases.
- Group sizes vary a lot: Terrain is one flat group of 152 models, The Makers Cult has 66, Model Collection has 65.
- Model sizes vary too: 254 models have 1–5 files, 442 have 6–20, 366 have 21–100 and 70 have more than 100. The
  largest, Necrossia, has 2,916 files.

## Proposed mapping

```
/<volume>/<collection>/                      ← collection.split('/'), root group (path='')
    <group>/<subgroup>/…                     ← groups via parent_id; group posters = folder thumbnail
        <Model name>/                        ← models; hero_asset_id ?? hero_fallback_asset_id = icon
            Images/                          ← assets where model_id = m.id (rank order; thumbs, instructions)
            Docs/                            ← model_files kind doc/meta + assets role doc
            <variant>/                       ← model_files.variant  ('' → "Default"; omit level if only one)
                <part_group>/…               ← model_files.part_group split on '/'  ('' → files at this level)
                    file.stl / .lys / .obj   ← entries via entry_id; .mtl shown beside its .obj
```

### Design choices

1. **Variant above part group.** People print one variant at a time, so `Supported/Arms/…` reads better than
   `Arms/Supported/…`. When a model has a single variant, skip that level.
2. **Split combined variants.** Values like `supported+lychee` are combinations of atoms. Either show them as a folder
   name like `Supported · Lychee`, or turn them into filters instead of folders.
3. **Stable paths.** Ids change on every `index`, so build virtual paths from `collection` + `groups.path` /
   `models.path`. The same pair works as the key for favourites and tags.
4. **Name clashes.** 9 groups contain two models with the same display name (mostly edition duplicates like Illuminor
   Szeras 5E/8E). Add a suffix such as `(5E)` from `edition`, or fall back to `raw_name`.
5. **Posters.** Use a group's poster as its folder thumbnail. Models without their own image already have the nearest
   group poster in `hero_fallback_asset_id`.

## Oddities that will show up as strange folders

- **`stl RM019` is a model inside group `RM019`** (`Red Makers/RM019/stl_RM019`). The wrapper folder wasn't collapsed
  into the model, so the tree shows `RM019/stl RM019/`.
- **A part group named `images` holds an `.obj` file** (`images/large_preview_Turretleft_fixed.obj`). This is probably a
  Thingiverse export where a preview mesh landed in `images/`.
