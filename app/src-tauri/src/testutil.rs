//! Test fixtures: a tiny indexed library of loose files.
use std::path::Path;

use stl_index::{db, resolvestore, scanstore};
use stl_scan::nested::ListOptions;

use crate::state::AppState;

/// A 1×1 transparent PNG.
pub const TINY_PNG: &[u8] = &[
    0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0x00, 0x00, 0x00, 0x0D, 0x49, 0x48, 0x44, 0x52, 0x00, 0x00, 0x00, 0x01,
    0x00, 0x00, 0x00, 0x01, 0x08, 0x06, 0x00, 0x00, 0x00, 0x1F, 0x15, 0xC4, 0x89, 0x00, 0x00, 0x00, 0x0A, 0x49, 0x44, 0x41,
    0x54, 0x78, 0x9C, 0x63, 0x00, 0x01, 0x00, 0x00, 0x05, 0x00, 0x01, 0x0D, 0x0A, 0x2D, 0xB4, 0x00, 0x00, 0x00, 0x00, 0x49,
    0x45, 0x4E, 0x44, 0xAE, 0x42, 0x60, 0x82,
];

/// `<dir>/lib/Vol/Coll/{Orc/{orc.stl, orc.jpg, notes.txt}, Elf/elf.stl}`, with `<dir>/lib/Vol`
/// added as a root and indexed; the app state lives in `<dir>/home`. `orc.jpg` is a real image
/// (PNG bytes); the `.stl` files are not real meshes.
pub fn library(dir: &Path) -> AppState {
    for (rel, body) in [
        ("lib/Vol/Coll/Orc/orc.stl", b"o".as_slice()),
        ("lib/Vol/Coll/Orc/orc.jpg", TINY_PNG),
        ("lib/Vol/Coll/Orc/notes.txt", b"n"),
        ("lib/Vol/Coll/Elf/elf.stl", b"e"),
    ] {
        let p = dir.join(rel);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, body).unwrap();
    }
    let home = dir.join("home");
    std::fs::create_dir_all(&home).unwrap();
    let mut conn = db::open(&home.join("index.db")).unwrap();
    scanstore::add_root(&conn, &dir.join("lib/Vol")).unwrap();
    resolvestore::index_all(&mut conn, &ListOptions::default(), &mut |_| {}).unwrap();
    AppState::open(home).unwrap()
}
