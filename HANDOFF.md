# Handoff: stl-browser

## Goal

A Windows desktop app (Rust + Tauri 2, Svelte 5 UI) for browsing 3D-print collections: STL/OBJ/LYS files and images,
often inside RAR/ZIP/7Z archives nested in other archives. You add folders to an index, rescan them, and browse them
like Windows Explorer with very large icons:
- Each folder shows a carousel of the pictures inside it.
- STL files get rendered thumbnails.
- Dragging anything to Explorer extracts it with its folder structure.

Tags and search come later.

**Read [`docs/index.md`](docs/index.md) first.** It has the status, architecture, every decision (yours and mine)
and the deferred items.

## Current state

- **Branch `stl-browser-app`**, 20 commits ahead of `main`, all committed, not merged. The repo has no remote.
- **Milestone 1 (skeleton) is done.** The Tauri + SvelteKit app starts, opens or creates `~/.stl-browser/index.db`
  (schema v3), and shows the index path in a status bar.
- **Milestone 2 (scan) is done**, with a whole-branch review and its fixes:
  - **Archives:** native ZIP/7Z/RAR, nested archives up to 3 deep, extraction by chain, and discovery.
  - **Rescan:** incremental, with a file-name fingerprint for folder sources, errored sources retried, and vanished
    sources removed.
  - **Robustness:**
    - Unreadable folders are skipped with a warning.
    - A panic in an archive parser becomes an error on that source.
    - Big nested archives spill to disk.
    - Password-protected archives are flagged `locked` and never opened.
- **Tests:** `cargo test --workspace` gives 44 passed, 0 failed, plus 1 ignored RAR test that needs a real file.
- **Real data:** `D:\STL\Warhammer 40K Collection Vol.1` gives 45 sources, 18,050 entries (1,243 nested), 0 errors
  and 0 locked, in about 7–35 s. A rescan re-lists nothing. This matches the Python prototype exactly.
- **UI mockup** (approved, "looks good"): https://claude.ai/artifact/Vum7b4yY5TjCBW1Csez2BH

## Next step: Milestone 3 (resolve)

Write the plan with `superpowers:writing-plans` in `docs/superpowers/plans/`, then execute it **natively**. That is
the method chosen for Milestones 1–2: inline, TDD per step, a ledger in `.superpowers/sdd/<plan>/`, and one fresh
reviewer at the end.

Scope:
1. **Port the resolver to Rust in `crates/stl-core`.** The Python sources are `prototype/stlindex/names.py` (extend
   the existing `crates/stl-core/src/names.rs`), `resolve.py` and `vtree.py`.
2. **Port all 47 prototype tests** (`prototype/tests/`) with the same inputs and expected outputs.
3. **Add the resolved tables** (`groups`, `models`, `model_files`, `assets`) as a schema migration. They are rebuilt
   per collection on every index, and user data never goes in them.
4. **Build both views:** the Models view (from `vtree.py`) and the Files view (from `entries.path`).
5. **Apply the naming rules from `docs/virtual-tree.md` to both views:**
   - Names are lowercase.
   - Overlapping folders merge.
   - Colliding files get `_1`/`_2` postfixes.
   - Hover shows the real path with `!` at each step into an archive.
   - Password-protected archives show as padlock items.
6. **Parity check.** Index Vol.1 + Vol.2 and compare with `prototype/stl-index.db` and the baseline in
   `docs/archive-patterns.md`: 1,132 models (447 / 685), and "without own image" 70 / 135. Run the spot checks listed
   there too.
7. **Once parity holds, delete `prototype/`.** The user asked for this. `prototype/stl-index.db` and
   `prototype/HANDOFF.md` are **not in git**, so deleting the folder loses them for good. Confirm that nothing is
   still needed first.

After that: Milestone 4 (browse UI), 5 (thumbnails), 6 (drag-out). These are described in the spec.

## Where things are

| Path | What |
|------|------|
| `docs/index.md` | Overview, status, all decisions, deferred review items, working notes |
| `docs/archive-patterns.md` | Layout patterns P0–P25 / D1–D8, vocabularies, resolver algorithm, parity baseline, spot checks |
| `docs/virtual-tree.md` | Resolved tables, Models/Files view layout, naming rules (lowercase, merge, `_1`, `!` hover, padlock) |
| `docs/superpowers/specs/2026-10-27-stl-browser-design.md` | The design spec. It is the binding authority. |
| `docs/superpowers/plans/2026-10-27-skeleton-and-scan.md` | The completed plan for Milestones 1–2 |
| `crates/stl-core` | Pure logic: file kinds and name helpers (resolver goes here) |
| `crates/stl-scan` | `archive.rs` (Zip/SevenZ/Rar readers), `nested.rs`, `extract.rs`, `discover.rs` |
| `crates/stl-index` | `paths.rs`, `db.rs` (schema + migrations), `scanstore.rs` (add_root / scan_root), `bin/stlx.rs` |
| `app/src-tauri/src` | `lib.rs` (setup, opens the DB), `state.rs`, `commands.rs` (`app_info`) |
| `app/src/routes/+page.svelte` | The UI (status bar only so far) |
| `prototype/` | The Python reference implementation. Not part of the app. |

## Commands

```sh
cargo test --workspace
STL_BROWSER_RAR_FIXTURE=<some.rar> cargo test -p stl-scan --test rar -- --ignored
cargo run --release -p stl-index --bin stlx -- add "D:\STL\Warhammer 40K Collection Vol.1"
cargo run --release -p stl-index --bin stlx -- scan
cargo run --release -p stl-index --bin stlx -- extract <entry_id> <out file>
npm --prefix app run tauri dev        # launch the app
npm --prefix app run check            # svelte-check
```

Set `STL_BROWSER_HOME=<scratch dir>` for `stlx` and test runs, so they don't touch the user's `~/.stl-browser`.

## Gotchas

- **Shell heredocs here mangle backslashes** (`'\\'` became `'\'` in Rust source). Write source files with the
  editor tools.
- **`unrar_sys` doesn't link `advapi32`.** `crates/stl-scan/src/lib.rs` links it explicitly. Without that the link
  fails with Crypt*/Reg* unresolved.
- **unrar can't write to a file that is still open.** Close the handle first (`NamedTempFile::into_temp_path`), or
  nested archives inside RARs silently fail with "Could not create file".
- **No RAR fixtures.** RAR files can't be created without WinRAR, so RAR paths are verified against real Vol.1
  archives. Vol.1's Ghamak `.rar` holds 34 nested zips and is a good regression check.
- **Leave the other tool's folder alone.** `D:\STL\...Vol.1\.3d-model-library\` belongs to another program.
  Discovery skips dot-folders.
- **Don't kill processes by image name.** Blanket `taskkill //IM node.exe` was denied. To stop a dev server, find it
  by its port.
- **The release profile uses `panic = "unwind"` on purpose**, so parser panics can be caught. Don't switch it back to
  `abort`.
- **Schema changes go through `MIGRATIONS` in `crates/stl-index/src/db.rs`.** Bump `SCHEMA_VERSION`, then update
  `tests/db.rs`.

## Open items

- **Branch integration** (merge to `main`, open a PR, or keep) is undecided. The user was offered the choice but
  answered with new requirements instead.
- **Deferred review findings** are listed in `docs/index.md` → "Deferred". The most relevant for Milestone 3 is that
  nested-archive warnings aren't stored, so the UI can't show skipped nested archives.
