# stl-browser — indexer

`stlindex` scans folders of 3D-print archives (RAR/ZIP/7Z, including archives nested inside
archives) and loose files, and resolves the messy paths into **groups → models → variants →
parts**, with each model's **preview images**. The layout heuristics are documented in
[archive-patterns-plan.md](archive-patterns-plan.md).

Requires Python 3.10+ and 7-Zip (`C:\Program Files\7-Zip\7z.exe` is found automatically).
No other dependencies.

```sh
python -m stlindex index "D:\STL\Warhammer 40K Collection Vol.1" "D:\STL\Warhammer 40K Collection Vol.2"
python -m stlindex report                                           # per-collection / per-archive summary
python -m stlindex report --source Ghamak --tree                    # groups/models of matching archives
python -m stlindex vtree --depth 3                                  # virtual folder tree (see folder-structure.md)
python -m stlindex vtree "Warhammer 40K Collection Vol.2/04. Primarchs" --files   # expand models into files
python -m stlindex extract 1234 --out .preview-cache                # pull one file (e.g. a hero image)
python -m unittest discover -s tests -t .                           # tests
```

**Collections.** Each top-level folder under a root is a *collection*, and is resolved as one tree in
which **archives are just folders**. So a picture next to an archive (`X.jpg` + `X.rar`), a folder
holding several variant archives, or an archive next to pictures of the models inside it, are all
handled by the same rules as the structure inside archives.

Archive listings are cached in the DB (keyed by size + mtime), so re-running `index` only re-lists
changed archives and then re-resolves everything (fast). Use `--rescan` after changing scanner code,
or just re-run `index` after changing the resolver heuristics. Folders starting with `.` are skipped.

## Database (`stl-index.db`)

| table | what |
|---|---|
| `sources` | one row per archive / loose-files folder; `collection` = `<root>/<top folder>`, `coll_path` = archive path inside it |
| `entries` | every file inside a source; `chain` = JSON list of members to extract (nested archives) |
| `groups` | release / bundle / studio / category folders above models (`parent_id` tree), per `collection` |
| `models` | `name`, `breadcrumb`, `catalog_id`, `designer`, `edition`, `variants` (JSON), `part_groups`, `hero_asset_id`, `hero_fallback_asset_id` (nearest group poster); `source_id` = archive holding most of its files |
| `model_files` | printable files per model with `part_group` and `variant` (e.g. `supported`, `lychee`, `supported+32mm`) |
| `assets` | images/docs: `role` = preview, thumb, instructions, poster, doc; `match` = how it was attached (inside, sidecar, thumb, only, id, name, poster); `rank` 0 = hero |
