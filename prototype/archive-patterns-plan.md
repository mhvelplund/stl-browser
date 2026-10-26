# STL collection: layout patterns and a path-to-model resolver

## Context

`D:\STL\Warhammer 40K Collection Vol.1` is meant to feed a browser that shows **models**, each with its **preview
images** and **printable files** (STL/OBJ/LYS…). The folder holds 45 `.rar` files in 4 collection folders
(`01. Studio Collection`, `02. Miniatures Collection`, `03. Leviathan v10`, `04. Fast Necron Units`). Inside them: 10.4k
STL, 2.4k OBJ, 1.8k LYS, 1.9k images, plus 58 nested archives (34 .zip, 24 .7z). None are encrypted. All were listed
read-only with `C:\Program Files\7-Zip\7z.exe l -ba -slt`.

## Patterns found (beyond the one you spotted)

| #   | Pattern                                                                                                                                | Example(s)                                                                                                                                                                                                                                          | Mapping rule                                                                                                                                                                                                           |
|-----|----------------------------------------------------------------------------------------------------------------------------------------|-----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------|------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------|
| P0  | **Wrapper root.** Every archive has exactly one top folder, named like the archive. The `.rar`'s parent folder is the collection.      | all 45                                                                                                                                                                                                                                              | Drop the root folder. Collection = folder that holds the archive. Studio/pack = archive name.                                                                                                                          |
| P1  | **Flat kit.** Parts and one image sit right in the root.                                                                               | Socratus_Typho, Tau Waifu Commander, Veteran Boi, Hero of the king 2, Horus ascended                                                                                                                                                                | The whole archive is 1 model and the image is its preview.                                                                                                                                                             |
| P2  | **Image beside the parts folder.** Image(s) in the root, plus one folder of parts.                                                     | The Daemon Primarch Angron, Legion of Fire, Horus Lupercal (`STLs/`), Heldrake                                                                                                                                                                      | 1 model. Images in the parent belong to the only child folder.                                                                                                                                                         |
| P3  | **Model → variant subfolders** (your pattern, generalised). The model folder holds images; its children are print variants.            | Atlan Forge, Minigames, MeshBrain, MB, Blackrock, Imperitum, PuppetsWar                                                                                                                                                                             | Treat a child folder as a *variant*, not a model, when its name matches the variant vocabulary below. Images in the model folder are its previews.                                                                     |
| P4  | **Variant names prefixed with the model name.**                                                                                        | `Asgardian Trikers - Supported`, `GordonSupported`, `842_Tarantula_Unsupported`, `Colonel Supported STL`, `Nomads Presupported`, `Pressupported - By AiR`                                                                                           | Match the variant keyword as a *suffix or token* after normalising (lowercase, `_-` become spaces, typos `pressuported`/`unsuported` folded in).                                                                       |
| P5  | **Duplicate nesting.** A model folder holds only one child with the same or a similar name.                                            | `Aegyptian Amenti/Aegyptian Amenti/`, `Blackrock …/Blackrock Miniatures - X/`, `Destiny Bane/Destiny Bane/`, `PunchBot_RawOBJs/PunchBot_RawOBJs`                                                                                                    | Collapse chains where a folder has 1 subfolder and 0 model files. Keep the more descriptive name.                                                                                                                      |
| P6  | **Variant split above the model.** The variant or format folder is *higher* than the parts, and each variant repeats the same subtree. | Station Forge `01. FORMAT STL/` vs `02. FORMAT OBJ/`; VOIDWALKERS `RAW STLS/ · PRE-SUPPORTED STLS/ · LYCHEE SCENES/`; Ghamak `845-854 Imperator incursor Supported/`                                                                                | Remove the variant segment from the path. Join files whose remaining relative paths match into one model with several variants.                                                                                        |
| P7  | **Parts kit / builder.** Subfolders are part *categories*, not models.                                                                 | Imperial Fists Builder, CHAOS MARINE BUILDER, VOIDWALKERS (`ARMS/HEADS/LEGS/BASE`), Helbrute (`BODY/HEADS/LEFT_HAND`), Heresy Labs Titan (`body/legs/cannon`), Anvil (`Arms/Heads/Torsos`), **Heldrake in Portuguese** (`asas/bracos/cabeca/corpo`) | Parts vocabulary (arms, heads, legs, torso, body, bodies, backpack, base, weapons, guns, pads, bits, blades, hands, cabeca…). When most children match it, the *parent* is the model and the children are part groups. |
| P8  | **Release/bundle folders above models.**                                                                                               | Ghamak `824-841_Sci-Fi May 2023/`, `Puppetswar_Feb23_/`, Immaterium `Desert scavengers Vol. I (full release)`, MB `2305_-_Female_Knights`, Surrogate `…, April Hero Release`, OPR `OPR_Battle Sisters`                                              | Folder with ≥2 children that each look like models becomes a *group* (shown as a tag or breadcrumb). Dates or "Vol." in the name mark a release.                                                                       |
| P9  | **Numbered catalogue IDs.**                                                                                                            | Ghamak `NNN_Name`, `NNN-MMM_Range`, images `842 Complete Tarantula.jpg` (sometimes inside the *Unsupported* folder); Heresy Labs `HL165/HL165.jpg`                                                                                                  | Pull the leading `\d{3}` / `HL\d+` ID from folders and images, then match images to models by ID before matching by name.                                                                                              |
| P10 | **Dedicated image folders.**                                                                                                           | `Renders/`, `Poster/`, `poster/`, `01. Poster/`, `Images/`, `images/` (Wolf: 44 Thingiverse previews), `_PunchBot_Renders/`, `BuildGuide/`                                                                                                          | Image-only folders hand their images to the parent model. In a group folder they become group or collection posters.                                                                                                   |
| P11 | **Per-file thumbnails** named `<part>.stl.png`.                                                                                        | Modular Chonky Gloomy Knights (67)                                                                                                                                                                                                                  | Join to the part with the same full filename. Show as part thumbnails.                                                                                                                                                 |
| P12 | **Images named after sibling folders.**                                                                                                | Imperitum `Destiny Bane (Death Jester)-01.jpg` in the parent; Surrogate `<folder name>-01.jpg`; Ghamak release posters                                                                                                                              | Fuzzy-match image stems (with `-01`, `(3)`, `resize-` removed) to sibling folder names. If nothing matches, attach to the folder that contains the image.                                                             |
| P13 | **Non-preview images and docs.**                                                                                                       | `Assembly-Instructions.jpg`, `INST_01.png`, `CONTENTS.png`, `Part_list.jpg`, `Build Guide.png`, `Cape Assembly.jpg`, `*Painting Guide.pdf`, `Codex … .pdf`                                                                                          | Keyword tag `instructions` keeps these from being picked as the hero image. Generic names (`1.jpg`, `photo.jpg`, `untitled.822.jpg`, hashes like `LsB0hMMwR5E.jpg`) are allowed as previews but get lower rank.        |
| P14 | **Nested archives hold the real content.**                                                                                             | Ghamak: 34 `.zip` (each model folder → `<same name>.zip`); One Page Rules: 24 `.7z` and **no models outside them** (only 329 PNG + 8 JPG)                                                                                                           | Index recursively. A nested archive is one more level, and its name usually equals the model name.                                                                                                                     |
| P15 | **Metadata in names.**                                                                                                                 | `Canoptek Spyder (VirusesofDeath) (5E)`, `(HappyDuck3D)`, `By AiR`, `(Format OBJ)`, `32mm STL` / `25mm STL`, `9th`/`9E`, `v10`                                                                                                                      | Regex out the designer `(Name)`, edition `(\dE)`, format and scale tags. Store them as attributes and drop them from the display name.                                                                                 |
| P16 | **File pairing.**                                                                                                                      | 254 `.lys`+`.stl` share a stem; 149 `.obj`+`.mtl`; `*_supported.stl`, `support_*`, `_Flamer.stl` prefixes; `.chitubox`, `.lyt`, `.3mf`, `.zpr`                                                                                                      | Group files by stem in each folder: STL, LYS and OBJ of the same part become one entry. Treat `.mtl` as a companion, never as a model.                                                                                 |
| P17 | **Junk.**                                                                                                                              | `desktop.ini`, `.avi`, `whatgun.txt`                                                                                                                                                                                                                | Ignore, or list as attachments.                                                                                                                                                                                        |

Variant vocabulary seen: `supported` (55), `unsupported` (45), `presupported / pre-supported`,
`lychee / lychee scenes / lychee files / lys`, `original files`, `raw stls`, `stl`, `obj`, `chitubox`, `finished`,
`32mm (supported) stl`, `25mm (supported) stl`, `format stl/obj`.

## Proposed resolver (for the browser indexer)

1. **Enumerate.** Walk the folders, list each archive with 7-Zip (`l -slt`), and recurse into nested archives (P14).
   Emit `(collection, archive, inner_path, size, crc)` with no extraction. Previews are extracted on demand and cached.
2. **Build a virtual tree** per archive. Drop the root (P0).
3. **Classify each folder** bottom-up as one of `VARIANT | PARTGROUP | IMAGEDIR | WRAPPER | MODEL | GROUP`:
   - `VARIANT`: name matches the variant vocabulary, also as a suffix (P3/P4/P6).
   - `PARTGROUP`: name in the parts vocabulary (P7).
   - `IMAGEDIR`: only images, or a name like render/poster/images (P10).
   - `WRAPPER`: exactly 1 subfolder and no model files (P5).
   - `MODEL`: the lowest folder that holds model files after variant, part-group and image folders are absorbed into it.
     It is never itself a variant or part group.
   - `GROUP`: holds ≥2 `MODEL`s (P8). A root with models directly in it becomes a model (P1).
4. **Normalise names.** Strip IDs, attribution, edition and format tags into fields (P9, P15). Display name = the most
   descriptive name in the collapsed chain.
5. **Attach images** by precedence: per-file thumbnail (P11) → ID match (P9) → fuzzy name match to a sibling (P12) →
   nearest model at or below the image's folder, or the only child (P2) → group or collection poster. Tag instruction
   images (P13).
6. **Group files** by stem and variant into parts (P16).
7. **Output** a SQLite index: `collections, archives, groups, models(name, designer, edition, ids), variants, parts,
   files(archive, inner_path, ext, size), images(role: preview|instructions|poster|thumb)`.

## Verification

- Run the indexer on this folder and print one line per archive: models found, images attached, unattached images, and
  models with 0 images.
- Spot-check the known shapes: Atlan `Asgardian Trikers` → 1 model, 2 variants, 6 images. VOIDWALKERS → 1 model, 3
  variants, part groups ARMS/HEADS/…. Ghamak 842 Tarantula → image found inside Unsupported. OPR → models resolved from
  the nested .7z. Heldrake → 1 model. Imperial Fists Builder → 1 kit, not 9 models.
- Target: under 5% of images left unattached, and every archive yields ≥1 model.

## Patterns found while running the indexer on the real data

| #   | Pattern                                                                                     | Example(s)                                                                                                            | Rule implemented                                                                                                                                                                             |
|-----|---------------------------------------------------------------------------------------------|-----------------------------------------------------------------------------------------------------------------------|----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------|
| P18 | **Unit = one product.** Photos sit at the unit level, and the figure subfolders have none.  | Station Forge `Corrupted Guard Kill Team/Corrupted 1..11`, CHAOS MARINE BUILDER, Lore Altivrun, Kill Team Unsupported | A group whose models have no images of their own but which has images itself is re-resolved as **one model**. The figures become part groups. *Image granularity decides model granularity.* |
| P19 | **Google-Drive export folders.**                                                            | `578 Hoplites of Light A-20221219T082318Z-001/`                                                                       | Strip the suffix.                                                                                                                                                                            |
| P20 | **Parent name repeated** as the default-variant container next to a variant sibling.        | `578-590_Hoplites of Light/Hoplites of Light/` + `presupported/`                                                      | Transparent.                                                                                                                                                                                 |
| P21 | **Kit prefix on part folders.**                                                             | `TEOTH_Heads`, `TEOTH_Legs_DiscKnees`                                                                                 | Ignore tokens shared by all siblings when testing part-group names.                                                                                                                          |
| P22 | **Catalog ID only on the pictures.**                                                        | `Bullboyz 1/442 Complete bullboyz 1.png`; group image `resize-442-complete-bullboyz-1-1.jpg`                          | Model adopts the ID from its own pictures (years excluded). Group images then match by ID.                                                                                                   |
| P23 | **Shared prefixes / compounds in image names.**                                             | OPR `_Images/APC 1.png` against "Battle Sisters - APC"; `bs-exo-suits-render-3.png` against "Exosuits"                | Match on *distinctive* tokens: drop tokens common to most candidates and tokens of the image's own folder path, fold plurals, join adjacent tokens.                                          |
| P24 | **Leading variant word on part folders.**                                                   | `Supported Backpacks STL` next to `Backpacks`                                                                         | `variant_of` keeps the rest as the prefix, so they merge.                                                                                                                                    |
| P25 | **`(Format OBJ)` in names** is a tag, not a variant folder. Scale and format folders stack. | `01. FORMAT STL/…/32mm Supported STL`                                                                                 | Variants are stored as atoms: support state + scale. Format is dropped, since the file extension carries it.                                                                                 |

## Implementation (done)

`stlindex/` package, Python stdlib + 7-Zip. See `README.md` for usage.

- `names.py`: vocabularies and name heuristics. `resolve.py`: pure resolver (physical tree → logical tree → models →
  assets). `scan.py`: 7-Zip listing, nested archives, on-demand extract. `db.py`: SQLite. `cli.py`:
  `index | report | extract`.
- 32 unit tests in `tests/`, built from real layouts.
- Result on this collection: 45 archives, 18,050 files (incl. 1,243 from nested archives) → **453 models**. 377 have
  their own preview image. 76 have none of their own, and 59 of those get a group poster as `hero_fallback_asset_id`. 17
  have no image at all. 276 images stay as group/collection posters. A full scan takes about 1 minute; re-resolving from
  the cache takes 0.3 s.

## Vol.2 check (`D:\STL\Warhammer 40K Collection Vol.2`)

This was a read-only dry run. I listed all 297 archives (200 zip, 97 rar, 17.4k files inside, no errors) and ran the
resolver on each one in memory, without nested expansion.

**Layout differences from Vol.1**

- There are 122 loose images **on disk next to the archives**. Vol.1 had none.
- `01. Studio Collection` (40 rars) holds **bags of nested archives, each with a same-named preview image**: 227 nested
  archives and 334 images, 194 of which have exactly the stem of a nested archive.
- `02. Model Collection`, `03. ОРКИ`, `04. Primarchs` and `05. All primarch Bust` keep their pictures on disk beside the
  archive.
- `06. Necrons STC` has the same shape as Vol.1's Necron Ellit, with battle-role category folders (`01. HQ`,
  `02. Troops`…) on top.
- `07. Warhammer 40k Terrain` is 153 flat Thingiverse-export zips.

**Do the existing patterns hold?**

| Pattern                                   | Holds?                           | Evidence                                                                                                                                                                                                     |
|-------------------------------------------|----------------------------------|--------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------|
| P0 wrapper root                           | **Partly.** No longer universal. | single top = archive name 160; single top ≠ name 58; multi-top 70; flat 9. The resolver doesn't rely on P0 (wrapper collapse covers all four shapes).                                                        |
| P1 flat kit / P2 image beside folder      | Yes                              | `Bladeguard_Captain.rar`, `Cyber_Viking.rar`, `Stormtroopaz.rar` → 1 model each                                                                                                                              |
| P3/P4/P6 variants                         | Yes                              | labels: supported 44, unsupported 24, lychee 4, supported+chitubox 1. The "support"-like folders left unclassified are true non-variants (`05. Heavy Support`, `Clips & Supports`, `Modular Floor Supports`) |
| P7 part kits                              | Yes                              | `Imperial_Fists_Builder.zip`, `Venatari_Builder.zip` → 1 model each                                                                                                                                          |
| P10 image folders / P11 `.stl.png` thumbs | Yes                              | `modular-chonky-gloomy-knights.zip` (136 images)                                                                                                                                                             |
| P13 instructions / P16 pairing            | Yes                              | LYS 586, OBJ+MTL 205                                                                                                                                                                                         |
| P14 nested archives                       | **Holds, and is now dominant**   | 227 nested archives in the studio rars; the models only exist after expansion                                                                                                                                |
| P15 designer / edition tags               | Yes                              | Necrons STC `(VirusesofDeath)`, `(JMO)`, `(5E)/(8E)/(9E)`                                                                                                                                                    |
| P18–P25                                   | Yes, where applicable            | no regressions in the dry run                                                                                                                                                                                |

### New patterns in Vol.2

| #  | Pattern                                                                                                 | Example(s)                                                                                                                                                                                                                                    | Rule                                                                                                                                                                                                                                                                                                                           |
|----|---------------------------------------------------------------------------------------------------------|-----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------|--------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------|
| D1 | **Sidecar image beside an archive**, on disk or inside a parent archive. Same stem, or a variant of it. | `40k Squat STL.jpg` + `.rar`; `Eternal Eagle (1..4).jpg`; `Cyber_Viking1.jpg`; `BA DC Heads and Weapons1.jpg`; `Acasatus_Knight_Porphyrion-1_1.webp`; `Bug_Warrior.jpg` ↔ `Bug_Warriors.rar`; truncated `40K_Empress_fina.jpg` ↔ `…final.rar` | Sidecar match, done before id/name matching. Normalise the image stem (remove trailing `1`, `(1)`, `_1`). It must equal a sibling folder/archive name, or one must be a prefix of the other (≥6 chars), or the singular forms must match. The image goes to that sibling's model, or becomes a poster of the sibling's group. |
| D2 | **Disk folder = one model**, holding several archives that are variants or split parts.                 | `03. ОРКИ/Jetpack_boys_pk1/{Unsupported.rar, Jetpack_boys_pk1_unsupported.zip, *.jpg}`; `MrModulork's Fire Ladz/{Unsupported.zip, Unsupported_1.zip}`; `Wargame Exclusive/RoP_AGG/{RoP_AGG.jpg, RoP_AGG_STL.zip, RoP_AGG_STL_SUP.zip}`        | Treat archives as folders (below). Add `sup`/`unsup`/`presup` to the variant vocabulary, and strip split-part indices (`Unsupported_1`).                                                                                                                                                                                       |
| D3 | **One archive + pictures named after the models inside it.**                                            | `04. Primarchs/{Primarchs.zip, Angron.jpg, Mortarion.jpg…}`; `05. All primarch Bust…/{….rar, Konrad Kurze.jpg…}`                                                                                                                              | Falls out of "archives as folders": the images sit at the same logical level as the archive's models and match by name.                                                                                                                                                                                                        |
| D4 | **Folder of archives + a poster one level up.**                                                         | `TYRANIDS/VOD_-_TYRANIDS_Complete.jpg` + `VOD_-_TYRANIDS_Complete/*.rar` (24 units)                                                                                                                                                           | D1 match to a *folder*, which makes it the group poster and the hero fallback for the units.                                                                                                                                                                                                                                   |
| D5 | **Thingiverse export layout.**                                                                          | 140× `README.txt` / `LICENSE.txt`, 94× `attribution_card.html`, `SOURCES.txt`, `files/`, `images/`                                                                                                                                            | New file kind `meta`: stored, not shown as docs. `files`/`images` already work (P7/P10).                                                                                                                                                                                                                                       |
| D6 | **Collection-wide name prefix**, and `+` used for spaces.                                               | `Warhammer 40k - Wargames Terrain - Ancient+Alien+Pylon.zip` (×153)                                                                                                                                                                           | Display name drops a prefix shared by ≥80% of sibling models, and turns `+` into a space. `raw_name` stays unchanged.                                                                                                                                                                                                          |
| D7 | **More file types.**                                                                                    | `.photon` (16), `.obv` (20), `.max` (3)                                                                                                                                                                                                       | `.photon` becomes a scene (sliced file). `.obv`/`.max` stay `other`.                                                                                                                                                                                                                                                           |
| D8 | **Hollow vs solid casts** as variant folders. Found while running the full index.                       | `Primarchs.zip/…/Sanguinus/{Hueco, Normal}`                                                                                                                                                                                                   | Variant words `hollow`/`hueco` and `solid`/`macizo`. A `Normal`/`Full` folder next to a hollow sibling counts as `solid`.                                                                                                                                                                                                      |

### Design consequence: archives are folders, at disk level too

The resolver already treats a nested archive as a folder. Vol.2 needs the same at disk level:

- Resolve once per **collection folder** (each top-level folder under a root) instead of once per archive. Each
  archive's entries are placed under `<archive path in collection>/…`, and loose files under their own relative paths.
- Sidecars, folder-of-archives, archive + pictures and folder posters (D1–D4) then go through the same rules as
  in-archive structure.
- The listing cache stays per archive. `index` accepts several roots, so Vol.1 and Vol.2 can share one DB.

### Result after implementing D1–D8 (both volumes in one DB)

The full scan of both volumes (346 sources, nested archives expanded) takes 5.6 min. Re-resolving from the cache takes
1.2 s. There are 41 unit tests.

|                             | models | without own image | with no image or fallback | unattached images |
|-----------------------------|--------|-------------------|---------------------------|-------------------|
| Vol.1 (before, per-archive) | 453    | 76                | 17                        | 276               |
| Vol.1 (now, per-collection) | 447    | 70                | 11                        | 277               |
| Vol.2                       | 685    | 135               | 117                       | 69                |

- **Vol.1 regression:** only Station Forge changed. `Miscellaneous` now merges into one unit, so there are 6 fewer,
  imageless models. 6 more models gained an image through sidecar matching.
- **Vol.2 imageless models:** mostly genuine absence. Necrons STC has 63 models with no pictures in the archive, and 44
  Terrain zips ship without pictures. The rest are 11 Studio, 15 Model Collection and 2 Primarch-bust models.
- **Sidecars:** 364 picture↔archive/folder matches. A sidecar of a multi-model archive becomes that archive's group
  poster (Heretic Guard, Feudal Guard packs, Adepts of Asmodeus).
- **Spot checks pass:** `Jetpack boys pk1` (1 model, unsupported, 2 images); `RoP AGG/CCW/COM` (`''` + supported, own
  picture); the Primarchs (pictures matched by name, Sanguinus hollow+solid); `40K Empress final` ←
  `40K_Empress_fina.jpg`; TYRANIDS poster on the TYRANIDS group (33 of 34 units have their own picture);
  `Illuminor Szeras` ×2 with editions 5E/8E; Terrain names such as `Ancient Alien Pylon`.
- **Known leftovers:**
  - 1 Terrain zip holds only `contents.obv`, so it yields 0 models.
  - D6 strips only prefixes shared by ≥80% of siblings, so some Terrain names keep `Terrain - …` /
    `Warhammer 30K 40K - …`.
  - 8 un-nameable `DSC_*.jpg` photos remain posters in Primarchs.

## Raw data

The per-archive listings are in
`C:\Users\mandr\AppData\Local\Temp\claude\C--Users-mandr-Documents-Projects-stl-browser\5ab86fbd-2a80-4b94-8df6-af7f9a02e5a9\scratchpad\listings\`
(45 `.txt` files, 7-Zip `-slt` format).
