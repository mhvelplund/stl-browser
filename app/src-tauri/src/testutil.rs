//! Test fixtures: a tiny indexed library of loose files.
use std::path::Path;

use stl_index::{db, resolvestore, scanstore};
use stl_scan::nested::ListOptions;

use crate::state::AppState;

/// `<dir>/lib/Vol/Coll/{Orc/{orc.stl, orc.jpg, notes.txt}, Elf/elf.stl}`, with `<dir>/lib/Vol`
/// added as a root and indexed; the app state lives in `<dir>/home`.
pub fn library(dir: &Path) -> AppState {
    for (rel, body) in [
        ("lib/Vol/Coll/Orc/orc.stl", "o"),
        ("lib/Vol/Coll/Orc/orc.jpg", "j"),
        ("lib/Vol/Coll/Orc/notes.txt", "n"),
        ("lib/Vol/Coll/Elf/elf.stl", "e"),
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
