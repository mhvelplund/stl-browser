# STL Browser

A Windows desktop app for browsing 3D-print collections: folders of STL, OBJ and LYS files and their pictures, often
packed in RAR, ZIP or 7Z archives that are nested inside other archives. You add folders to an index and browse them
like Windows Explorer with very large icons. Each folder shows a carousel of the pictures inside it, and you can drag
anything out to Explorer to extract it.

Built with Rust and Tauri 2, with a Svelte 5 + TypeScript UI. **Windows only.**

![STL Browser](docs/img/screenshot.jpg)

## Features

- **Index:** added folders go into a SQLite index under `~/.stl-browser/`. Rescans are incremental, so unchanged folders
  take almost no time.
- **Archives:** ZIP, 7Z and RAR are read natively, so 7-Zip and WinRAR aren't needed. Archives nested inside other
  archives are read up to 3 levels deep. Password-protected archives show a padlock and are never opened.
- **Two views:** *Models* groups files into detected models with their images. *Files* shows the physical layout, with
  archives opened like folders.
- **Thumbnails:** images are scaled, and STL/OBJ meshes are rendered on the CPU. Folders get a picture carousel.
- **Drag-out and "Extract to…":** drag a model, file or folder straight out of an archive into Explorer.

Tags, search and filters are planned. See [`docs/index.md`](docs/index.md) for the milestone status.

## Download

Download
[`stl-browser-v0.1.0.exe`](https://github.com/mhvelplund/stl-browser/releases/download/v0.1.0/stl-browser-v0.1.0.exe)
from the [v0.1.0 release](https://github.com/mhvelplund/stl-browser/releases/tag/v0.1.0). It is a standalone executable,
so there's nothing to install. It needs Windows 10 or 11 with the WebView2 runtime, which Windows 11 already has. The
exe isn't code-signed, so Windows SmartScreen may warn you the first time you start it.

To build from source instead, read on.

## Prerequisites

- Windows 10 or 11 with the [WebView2 runtime](https://developer.microsoft.com/microsoft-edge/webview2/). Windows 11
  already has it.
- [Visual Studio Build Tools](https://visualstudio.microsoft.com/visual-cpp-build-tools/) with the **Desktop development
  with C++** workload. Tauri and the bundled `unrar` C++ sources need it.
- [Rust](https://rustup.rs/) stable 1.85 or newer (the workspace uses edition 2024), with the default MSVC toolchain.
- [Node.js](https://nodejs.org/) 20.19+ or 22.12+ (needed by Vite 8), with npm.

## Build a release binary

From the repository root:

```sh
npm --prefix app install
npm --prefix app run build:installer
```

This builds the Svelte UI and then the Rust app in release mode (LTO, so the first build takes a while). It produces:

| Output                                   | What it is                                 |
|------------------------------------------|--------------------------------------------|
| `target/release/app.exe`                 | The app itself; it runs without installing |
| `target/release/bundle/nsis/*-setup.exe` | NSIS installer (per-user, no admin needed) |

Tauri downloads NSIS on the first build. Like `build:exe` below, `build:installer` remaps local source paths so the
binary doesn't embed paths from your machine (a plain `tauri build` doesn't). To build only the executable, skip the
installer:

```sh
npm --prefix app run build:exe
```

This skips the installer and also copies the exe to `target/release/stl-browser-<version>.exe`, with the version taken
from `app/src-tauri/tauri.conf.json`. Upload that file to a GitHub release. It also remaps local source paths (your
Cargo home and the checkout) so the exe doesn't embed paths from your machine.

## Run it

Start `target/release/app.exe` (or `stl-browser-<version>.exe`), or run the installer and start **STL Browser**
from the Start menu.

1. Click **Add folder** and pick a folder with your collection, e.g. `D:\Models`.
2. Indexing runs in the background. The first index of a large collection can take a few minutes, mostly spent listing
   archives. Later rescans only look at what changed.
3. Browse in the *Models* or *Files* view. Drag items out to Explorer, or use **Extract to…**.

The app keeps its data in `%USERPROFILE%\.stl-browser\`:

- `index.db` holds the index plus your settings. Settings are kept when the listings are rebuilt.
- `thumbs\` is a thumbnail cache. You can delete it safely; thumbnails are regenerated.

Set the `STL_BROWSER_HOME` environment variable to use a different data folder, e.g. for testing.

## Development

```sh
npm --prefix app run tauri dev   # run the app with hot reload
cargo test --workspace           # Rust tests
npm --prefix app run check       # UI type check (svelte-check)
npm --prefix app test            # UI logic tests
cargo run --release -p stl-index --bin stlx -- add <dir>   # dev CLI: add, index, extract, tree, parity
```

[`CLAUDE.md`](CLAUDE.md) has the full command list and the project's rules and gotchas.

## Repository layout

| Path                | Contents                                                                    |
|---------------------|-----------------------------------------------------------------------------|
| `crates/stl-core`   | Pure logic: file-name heuristics, the model resolver, the virtual tree views |
| `crates/stl-scan`   | Archive access (ZIP/7Z/RAR), nested listing, extraction, folder discovery   |
| `crates/stl-thumb`  | Thumbnail generation: image scaling, STL/OBJ rendering, priority queue       |
| `crates/stl-drag`   | Windows OLE drag-out of virtual files                                        |
| `crates/stl-index`  | The SQLite index: schema, incremental scans, views, export; `stlx` CLI       |
| `app/src-tauri`     | The Tauri shell and its commands                                             |
| `app/src`           | The SvelteKit UI                                                             |

Design documents:

- [`docs/index.md`](docs/index.md): status, architecture and design decisions. Start here.
- [`docs/superpowers/specs/2026-10-27-stl-browser-design.md`](docs/superpowers/specs/2026-10-27-stl-browser-design.md):
  the design spec.
- [`docs/archive-patterns.md`](docs/archive-patterns.md): archive layout patterns and the model resolver.
- [`docs/virtual-tree.md`](docs/virtual-tree.md): the Models and Files views and their naming rules.

## License

[MIT](LICENSE)
