# Collection layout patterns and the model resolver

The hard part of the app is mapping messy paths to **models**, each with its **preview images**, **variants** and
**printable files**. This page lists the layouts found in the real collections (`D:\STL\Warhammer 40K Collection
Vol.1` and `Vol.2`), the rule that handles each one, and the resolver that applies the rules. It is the spec for the
Rust resolver (milestone 3). The Python prototype implemented all of it and measured the results below.

## The data

- **Vol.1:** 45 `.rar` files in 4 collection folders (`01. Studio Collection`, `02. Miniatures Collection`,
  `03. Leviathan v10`, `04. Fast Necron Units`). Inside them are 10.4k STL, 2.4k OBJ, 1.8k LYS, 1.9k images, and 58
  nested archives (34 .zip, 24 .7z). None are encrypted.
- **Vol.2:** 297 archives (200 zip, 97 rar) holding 17.4k files.
  - There are 122 loose images on disk beside the archives.
  - `01. Studio Collection` holds 40 rars with 227 nested archives inside. Each nested archive usually comes with a
    preview image of the same name.
  - `06. Necrons STC` puts battle-role category folders (`01. HQ`, `02. Troops`…) on top.
  - `07. Warhammer 40k Terrain` is 153 flat Thingiverse-export zips.

## Patterns (Vol.1)

| #   | Pattern | Example(s) | Rule |
|-----|---------|------------|------|
| P0  | **Wrapper root.** The archive has exactly one top folder, named like the archive. The archive's parent folder is the collection. | all 45 in Vol.1. Only partly true in Vol.2: 160 single top = name, 58 single top ≠ name, 70 multi-top, 9 flat. | Don't rely on it. Wrapper collapse (P5) handles all four shapes. |
| P1  | **Flat kit.** Parts and one image sit right in the root. | Socratus_Typho, Tau Waifu Commander, Veteran Boi, Hero of the king 2, Horus ascended | The whole archive is 1 model and the image is its preview. |
| P2  | **Image beside the parts folder.** Image(s) in the root, plus one folder of parts. | The Daemon Primarch Angron, Legion of Fire, Horus Lupercal (`STLs/`), Heldrake | 1 model. Images in the parent belong to the only child folder. |
| P3  | **Model → variant subfolders.** The model folder holds images; its children are print variants. | Atlan Forge, Minigames, MeshBrain, MB, Blackrock, Imperitum, PuppetsWar | A child folder whose name matches the variant vocabulary is a *variant*, not a model. Images in the model folder are its previews. |
| P4  | **Variant names prefixed with the model name.** | `Asgardian Trikers - Supported`, `GordonSupported`, `842_Tarantula_Unsupported`, `Colonel Supported STL`, `Nomads Presupported`, `Pressupported - By AiR` | Match the variant keyword as a *suffix or token* after normalising (lowercase, `_-` become spaces, typos `pressuported`/`unsuported` folded in). |
| P5  | **Duplicate nesting.** A folder holds only one child with the same or a similar name. | `Aegyptian Amenti/Aegyptian Amenti/`, `Blackrock …/Blackrock Miniatures - X/`, `Destiny Bane/Destiny Bane/`, `PunchBot_RawOBJs/PunchBot_RawOBJs` | Collapse chains where a folder has 1 subfolder and 0 model files. Keep the more descriptive name. |
| P6  | **Variant split above the model.** The variant or format folder is *higher* than the parts, and each variant repeats the same subtree. | Station Forge `01. FORMAT STL/` vs `02. FORMAT OBJ/`; VOIDWALKERS `RAW STLS/ · PRE-SUPPORTED STLS/ · LYCHEE SCENES/`; Ghamak `845-854 Imperator incursor Supported/` | Remove the variant segment from the path. Files whose remaining relative paths match join one model with several variants. |
| P7  | **Parts kit / builder.** Subfolders are part *categories*, not models. | Imperial Fists Builder, CHAOS MARINE BUILDER, VOIDWALKERS (`ARMS/HEADS/LEGS/BASE`), Helbrute (`BODY/HEADS/LEFT_HAND`), Heresy Labs Titan (`body/legs/cannon`), Anvil (`Arms/Heads/Torsos`), Heldrake in Portuguese (`asas/bracos/cabeca/corpo`) | Parts vocabulary (arms, heads, legs, torso, body, bodies, backpack, base, weapons, guns, pads, bits, blades, hands, cabeca…). When most children match it, the *parent* is the model and the children are part groups. |
| P8  | **Release/bundle folders above models.** | Ghamak `824-841_Sci-Fi May 2023/`, `Puppetswar_Feb23_/`, Immaterium `Desert scavengers Vol. I (full release)`, MB `2305_-_Female_Knights`, Surrogate `…, April Hero Release`, OPR `OPR_Battle Sisters` | A folder with ≥2 children that each look like models becomes a *group*. Dates or "Vol." in the name mark a release. |
| P9  | **Numbered catalogue IDs.** | Ghamak `NNN_Name`, `NNN-MMM_Range`, images `842 Complete Tarantula.jpg` (sometimes inside the *Unsupported* folder); Heresy Labs `HL165/HL165.jpg` | Pull the leading `\d{3}` / `HL\d+` ID from folders and images. Match images to models by ID before matching by name. |
| P10 | **Dedicated image folders.** | `Renders/`, `Poster/`, `poster/`, `01. Poster/`, `Images/`, `images/` (44 Thingiverse previews), `_PunchBot_Renders/`, `BuildGuide/` | Image-only folders hand their images to the parent model. In a group folder they become group or collection posters. |
| P11 | **Per-file thumbnails** named `<part>.stl.png`. | Modular Chonky Gloomy Knights (67) | Join to the part with the same full filename. Show as part thumbnails. |
| P12 | **Images named after sibling folders.** | Imperitum `Destiny Bane (Death Jester)-01.jpg` in the parent; Surrogate `<folder name>-01.jpg`; Ghamak release posters | Fuzzy-match image stems (with `-01`, `(3)`, `resize-` removed) to sibling folder names. If nothing matches, attach to the folder that contains the image. |
| P13 | **Non-preview images and docs.** | `Assembly-Instructions.jpg`, `INST_01.png`, `CONTENTS.png`, `Part_list.jpg`, `Build Guide.png`, `Cape Assembly.jpg`, `*Painting Guide.pdf`, `Codex … .pdf` | Keyword tag `instructions` keeps these from being picked as the hero image. Generic names (`1.jpg`, `photo.jpg`, `untitled.822.jpg`, hashes like `LsB0hMMwR5E.jpg`) are allowed as previews but rank lower. |
| P14 | **Nested archives hold the real content.** | Ghamak: 34 `.zip` (each model folder has a `<same name>.zip`); One Page Rules: 24 `.7z` and **no models outside them** | Index recursively. A nested archive is one more folder level, and its name usually equals the model name. |
| P15 | **Metadata in names.** | `Canoptek Spyder (VirusesofDeath) (5E)`, `(HappyDuck3D)`, `By AiR`, `(Format OBJ)`, `32mm STL` / `25mm STL`, `9th`/`9E`, `v10` | Regex out the designer `(Name)`, edition `(\dE)`, and format and scale tags. Store them as attributes and drop them from the display name. |
| P16 | **File pairing.** | 254 `.lys`+`.stl` share a stem; 149 `.obj`+`.mtl`; `*_supported.stl`, `support_*`, `_Flamer.stl` prefixes; `.chitubox`, `.lyt`, `.3mf`, `.zpr` | Group files by stem within each folder: STL, LYS and OBJ of the same part become one entry. `.mtl` is a companion, never a model. |
| P17 | **Junk.** | `desktop.ini`, `.avi`, `whatgun.txt` | Ignore, or list as attachments. |

### Found while running the resolver on the real data

| #   | Pattern | Example(s) | Rule |
|-----|---------|------------|------|
| P18 | **Unit = one product.** Photos sit at the unit level; the figure subfolders have none. | Station Forge `Corrupted Guard Kill Team/Corrupted 1..11`, CHAOS MARINE BUILDER, Lore Altivrun, Kill Team Unsupported | A group whose models have no images of their own, while the group itself has images, is re-resolved as **one model**. The figures become part groups. *Image granularity decides model granularity.* |
| P19 | **Google-Drive export folders.** | `578 Hoplites of Light A-20221219T082318Z-001/` | Strip the suffix. |
| P20 | **Parent name repeated** as the default-variant container next to a variant sibling. | `578-590_Hoplites of Light/Hoplites of Light/` + `presupported/` | Transparent. |
| P21 | **Kit prefix on part folders.** | `TEOTH_Heads`, `TEOTH_Legs_DiscKnees` | Ignore tokens shared by all siblings when testing part-group names. |
| P22 | **Catalog ID only on the pictures.** | `Bullboyz 1/442 Complete bullboyz 1.png`; group image `resize-442-complete-bullboyz-1-1.jpg` | The model adopts the ID from its own pictures (years excluded). Group images then match by ID. |
| P23 | **Shared prefixes / compounds in image names.** | OPR `_Images/APC 1.png` against "Battle Sisters - APC"; `bs-exo-suits-render-3.png` against "Exosuits" | Match on *distinctive* tokens: drop tokens common to most candidates and tokens of the image's own folder path, fold plurals, join adjacent tokens. |
| P24 | **Leading variant word on part folders.** | `Supported Backpacks STL` next to `Backpacks` | `variant_of` keeps the rest as the prefix, so they merge. |
| P25 | **`(Format OBJ)` in names** is a tag, not a variant folder. Scale and format folders stack. | `01. FORMAT STL/…/32mm Supported STL` | Variants are stored as atoms: support state + scale. Format is dropped, since the file extension carries it. |

## Patterns (Vol.2)

| #  | Pattern | Example(s) | Rule |
|----|---------|------------|------|
| D1 | **Sidecar image beside an archive**, on disk or inside a parent archive. Same stem, or a variant of it. | `40k Squat STL.jpg` + `.rar`; `Eternal Eagle (1..4).jpg`; `Cyber_Viking1.jpg`; `BA DC Heads and Weapons1.jpg`; `Acasatus_Knight_Porphyrion-1_1.webp`; `Bug_Warrior.jpg` ↔ `Bug_Warriors.rar`; truncated `40K_Empress_fina.jpg` ↔ `…final.rar` | Sidecar match, done before ID and name matching. Normalise the image stem (remove a trailing `1`, `(1)`, `_1`). It must equal a sibling folder/archive name, or one must be a prefix of the other (≥6 chars), or the singular forms must match. The image goes to that sibling's model, or becomes a poster of the sibling's group. |
| D2 | **Disk folder = one model**, holding several archives that are variants or split parts. | `03. ОРКИ/Jetpack_boys_pk1/{Unsupported.rar, Jetpack_boys_pk1_unsupported.zip, *.jpg}`; `MrModulork's Fire Ladz/{Unsupported.zip, Unsupported_1.zip}`; `Wargame Exclusive/RoP_AGG/{RoP_AGG.jpg, RoP_AGG_STL.zip, RoP_AGG_STL_SUP.zip}` | Treat archives as folders (below). Add `sup`/`unsup`/`presup` to the variant vocabulary, and strip split-part indices (`Unsupported_1`). |
| D3 | **One archive + pictures named after the models inside it.** | `04. Primarchs/{Primarchs.zip, Angron.jpg, Mortarion.jpg…}`; `05. All primarch Bust…/{….rar, Konrad Kurze.jpg…}` | Falls out of "archives are folders": the images sit at the same logical level as the archive's models and match by name. |
| D4 | **Folder of archives + a poster one level up.** | `TYRANIDS/VOD_-_TYRANIDS_Complete.jpg` + `VOD_-_TYRANIDS_Complete/*.rar` (24 units) | A D1 match to a *folder* makes the image the group poster and the hero fallback for the units. |
| D5 | **Thingiverse export layout.** | 140× `README.txt` / `LICENSE.txt`, 94× `attribution_card.html`, `SOURCES.txt`, `files/`, `images/` | File kind `meta`: stored, not shown as docs. `files`/`images` already work (P7/P10). |
| D6 | **Collection-wide name prefix**, and `+` used for spaces. | `Warhammer 40k - Wargames Terrain - Ancient+Alien+Pylon.zip` (×153) | The display name drops a ` - `-terminated prefix shared by ≥80% of sibling models, and turns `+` into a space. `raw_name` stays unchanged. The ≥80% threshold is deliberately strict, so names like `GrimGuard - …` keep their prefix. |
| D7 | **More file types.** | `.photon` (16), `.obv` (20), `.max` (3) | `.photon` is a scene (sliced file). `.obv`/`.max` stay `other`. |
| D8 | **Hollow vs solid casts** as variant folders. | `Primarchs.zip/…/Sanguinus/{Hueco, Normal}` | Variant words `hollow`/`hueco` and `solid`/`macizo`. A `Normal`/`Full` folder next to a hollow sibling counts as `solid`. |

## Vocabularies

- **Variants seen:** `supported` (55), `unsupported` (45), `presupported / pre-supported`, `sup`/`unsup`/`presup`,
  `lychee / lychee scenes / lychee files / lys`, `original files`, `raw stls`, `stl`, `obj`, `chitubox`, `finished`,
  `hollow`/`hueco`, `solid`/`macizo`, `32mm (supported) stl`, `25mm (supported) stl`, `format stl/obj`.
  - Variants are stored as **atoms**: `supported`, `unsupported`, `lychee`, `chitubox`, `finished`, `hollow`, `solid`,
    and scales like `25mm`. Combinations such as `supported+lychee` and `supported+32mm` are allowed. `''` means
    unspecified. Formats are dropped.
  - Folders that look like variants but aren't: `05. Heavy Support`, `Clips & Supports`, `Modular Floor Supports`.
- **Part groups:** arms, heads, legs, torso(s), body/bodies, backpack(s), base(s), weapons, guns, pads, bits, blades,
  hands, and Portuguese `asas`, `bracos`, `cabeca`, `corpo`.
- **Image folders:** render(s), poster, images, buildguide, plus prefixed forms like `01. Poster`, `_PunchBot_Renders`.
- **File kinds:**
  - model: stl, obj, 3mf, ply, fbx, blend, ztl, zpr, step, stp
  - scene: lys, lyt, chitubox, ctb, cbddlp, goo, photon, pwmo, pwms, sl1
  - image: jpg, jpeg, png, webp, gif, bmp
  - companion: mtl
  - doc: pdf, txt, md, html, htm, rtf
  - meta: readme, license, sources, attribution card, link (as txt/html/md)
  - archive: zip, rar, 7z
  - junk: desktop.ini, thumbs.db, .ds_store
  - everything else is other

  These already live in `crates/stl-core/src/names.rs`.

## The resolver

**Unit of resolution: one collection** (a top-level folder under a root). Archives and nested archives count as
folders (`x.zip/` becomes `x/`), so disk-level and in-archive structure follow the same rules. The entries of every
source in the collection are combined into one path list: archive entries go under `<archive path in collection>/…`,
loose files under their own relative paths. Listings stay cached per archive.

1. **Physical tree.** Mark variant folders (`variant_of`, plus the sibling rule: `X` next to `X Supported` is the
   unsupported variant).
2. **Logical tree.**
   - Variant folders become transparent, and their label goes onto the files.
   - Image-only folders are hoisted to the parent.
   - A folder repeating its parent's name (P20), or with a Google-Drive export suffix (P19), is collapsed.
3. **Model detection** (bottom-up):
   - A folder with direct printable files is a model.
   - A folder whose children are mostly part groups (ignoring a shared kit prefix, P21) is a model.
   - Single-child chains collapse and keep the upper (more descriptive) name.
   - Anything else becomes a group. A group holds ≥2 models (P8). A root with models directly in it becomes a model
     (P1).
4. **Assets.**
   - Images inside a model belong to it, and `x.stl.png` is a thumbnail.
   - Every other image is tried in this order:
     1. sidecar match (D1)
     2. the only candidate
     3. catalog ID (P9)
     4. distinctive-token name match (P23)
     5. poster of the best or nearest group
   - Models adopt a catalog ID from their own pictures (P22).
   - Instruction images are tagged so they are never the hero (P13).
5. **Unit collapse (P18).** A group whose models have no pictures, while the group itself has some, is re-resolved as
   **one model**. This takes two passes.
6. **Names** (`parse_name`):
   - Pull out the catalog ID, the `(5E)` edition, designer tags (repeated across siblings, or `By X`) and
     `(Format OBJ)`.
   - Turn `+` and `_` into spaces.
   - Apply the D6 prefix rule.
   - Keep `raw_name`.
7. **Files.** Group files by stem and variant into parts (P16).

## Results to match (parity baseline)

These are the prototype's numbers on both volumes in one index:
- 346 sources: 342 archives and 4 folders.
- 48,942 entries: 1,243 nested in Vol.1.
- 1,132 models: Vol.1 447, Vol.2 685.
- 124 groups.
- 6,366 assets: preview 5,655 · thumb 201 · instructions 38 · doc 126 · poster 346.

|       | models | without own image | with no image or fallback | unattached images |
|-------|--------|-------------------|---------------------------|-------------------|
| Vol.1 | 447    | 70                | 11                        | 277               |
| Vol.2 | 685    | 135               | 117                       | 69                |

- **Sidecars:** 364 picture↔archive/folder matches. A sidecar of a multi-model archive becomes that archive's group
  poster (Heretic Guard, Feudal Guard packs, Adepts of Asmodeus).
- **Imageless models are mostly genuine absence:**
  - Necrons STC has 63 models with no pictures, and 44 Terrain zips ship without pictures. STL thumbnails cover these.
  - The rest are 11 Studio, 15 Model Collection and 2 Primarch-bust models.
- **Targets:** under 5% of images left unattached, and every archive yields ≥1 model.

### The Rust port's results (milestone 3)

`stlx parity <prototype.db>` compares the resolved tables with the prototype's.

- On Vol.1 + Vol.2, every number above matches exactly, per collection: models, groups, both image columns,
  unattached images, and assets by role.
- All the spot checks below pass, with the same values as the prototype.
- Group counts are 64 (Vol.1) and 60 (Vol.2).
- The TYRANIDS group holds 25 units, all with their own picture. The "33 of 34" in the spot checks below was an
  estimate.

### Spot checks

These must hold in the Rust port:
- `Asgardian Trikers`: 1 model, 2 variants, 6 images.
- VOIDWALKERS: 1 model, 3 variants, part groups ARMS/HEADS/….
- Ghamak 842 Tarantula: image found inside Unsupported.
- One Page Rules: models resolved from the nested `.7z`.
- Heldrake: 1 model. Imperial Fists Builder: 1 kit, not 9 models.
- `Jetpack boys pk1`: 1 model, unsupported, 2 images.
- `RoP AGG/CCW/COM`: `''` + supported, own picture.
- The Primarchs: pictures matched by name; Sanguinus is hollow + solid.
- `40K Empress final` ← `40K_Empress_fina.jpg`.
- TYRANIDS: poster on the TYRANIDS group, and 33 of 34 units have their own picture.
- `Illuminor Szeras` ×2, editions 5E/8E.
- Terrain names such as `Ancient Alien Pylon`.

### Known leftovers

These are accepted:
- 1 Terrain zip holds only `contents.obv`, so it yields 0 models.
- Some Terrain names keep `Terrain - …` / `Warhammer 30K 40K - …`, because the D6 threshold is strict.
- 8 un-nameable `DSC_*.jpg` photos remain posters in Primarchs.
- A single big flat folder isn't split by filename prefix (Immaterium "Desert scavengers Vol. I", 147 files).
- Edition duplicates (`Illuminor Szeras` 5E/8E) stay separate models with an `edition` attribute.
- Thingiverse `README.txt` (title, author) is stored as `meta` but not parsed, because that would need extraction at
  index time.

## Adding a pattern

1. A new layout gets a unit test built from the real path.
2. Add a row here.
3. Re-index and compare against the baseline above, watching for regressions in "without own image".
