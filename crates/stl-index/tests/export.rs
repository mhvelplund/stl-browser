mod common;

use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};

use common::{put, zip};
use rusqlite::Connection;
use stl_core::vtree::export::{ExportItem, ExportKind, ExportPlan};
use stl_index::export::{ExportFile, locate, resolve_plan, unique_dest, write_tree};
use stl_index::{db, resolvestore, scanstore};
use stl_scan::extract::SourceKind;
use stl_scan::nested::ListOptions;
use stl_scan::session::ExtractSession;

/// `<tmp>/lib/Vol/Coll/{Kit.zip (Kit/a.png, Kit/inner.zip (deep.stl)), c.png}`, indexed.
fn indexed(tmp: &Path) -> Connection {
    let inner = zip(&[("deep.stl", b"deep")]);
    put(tmp, "lib/Vol/Coll/Kit.zip", &zip(&[("Kit/a.png", b"png-a"), ("Kit/inner.zip", &inner)]));
    put(tmp, "lib/Vol/Coll/c.png", b"png-c");
    let mut conn = db::open(&tmp.join("index.db")).unwrap();
    scanstore::add_root(&conn, &tmp.join("lib/Vol")).unwrap();
    resolvestore::index_all(&mut conn, &ListOptions::default(), &mut |_| {}).unwrap();
    conn
}

fn id(conn: &Connection, path: &str) -> i64 {
    conn.query_row("SELECT id FROM entries WHERE path = ?", [path], |r| r.get(0)).unwrap()
}

fn dir(rel: &str) -> ExportItem {
    ExportItem { rel: rel.into(), kind: ExportKind::Dir }
}

fn file(rel: &str, entry_id: i64) -> ExportItem {
    ExportItem { rel: rel.into(), kind: ExportKind::File { entry_id } }
}

/// `kit/{a.png, inner/deep.stl, c.png, empty/}`
fn plan(conn: &Connection) -> ExportPlan {
    ExportPlan {
        items: vec![
            dir("kit"),
            file("kit/a.png", id(conn, "Kit/a.png")),
            dir("kit/inner"),
            file("kit/inner/deep.stl", id(conn, "Kit/inner.zip/deep.stl")),
            file("kit/c.png", id(conn, "c.png")),
            dir("kit/empty"),
        ],
        skipped: 0,
    }
}

fn write(dest: &Path, files: &[ExportFile]) -> stl_index::export::WriteReport {
    let mut session = ExtractSession::new();
    write_tree(dest, files, &mut session, &mut |_, _| {}, &AtomicBool::new(false))
}

#[test]
fn locate_returns_chain_and_kind() {
    let tmp = tempfile::tempdir().unwrap();
    let conn = indexed(tmp.path());
    let (deep, c) = (id(&conn, "Kit/inner.zip/deep.stl"), id(&conn, "c.png"));
    let locs = locate(&conn, &[deep, c, 9999]).unwrap();
    assert_eq!(locs.len(), 2);
    assert_eq!(locs[&deep].kind, SourceKind::Archive);
    assert_eq!(locs[&deep].chain, ["Kit/inner.zip", "deep.stl"]);
    assert!(locs[&deep].source_path.ends_with("Kit.zip"));
    assert_eq!(locs[&deep].size, Some(4));
    assert_eq!(locs[&c].kind, SourceKind::Folder);
    assert_eq!(locs[&c].chain, ["c.png"]);
}

#[test]
fn write_tree_writes_plan() {
    let tmp = tempfile::tempdir().unwrap();
    let conn = indexed(tmp.path());
    let files = resolve_plan(&conn, &plan(&conn)).unwrap();
    assert_eq!(files.len(), 6);
    assert!(files[0].is_dir && files[0].loc.is_none());
    let out = tmp.path().join("out");
    std::fs::create_dir(&out).unwrap();
    let r = write(&out, &files);
    assert_eq!((r.files, r.dirs), (3, 3));
    assert!(r.failed.is_empty(), "{:?}", r.failed);
    assert_eq!(r.bytes, 5 + 4 + 5);
    assert_eq!(std::fs::read(out.join("kit/a.png")).unwrap(), b"png-a");
    assert_eq!(std::fs::read(out.join("kit/inner/deep.stl")).unwrap(), b"deep");
    assert_eq!(std::fs::read(out.join("kit/c.png")).unwrap(), b"png-c");
    assert!(out.join("kit/empty").is_dir());
}

#[test]
fn write_tree_continues_after_error() {
    let tmp = tempfile::tempdir().unwrap();
    let conn = indexed(tmp.path());
    let files = resolve_plan(&conn, &plan(&conn)).unwrap();
    std::fs::remove_file(tmp.path().join("lib/Vol/Coll/c.png")).unwrap();
    let out = tmp.path().join("out");
    let r = write(&out, &files);
    assert_eq!(r.failed.len(), 1);
    assert_eq!(r.failed[0].0, "kit/c.png");
    assert_eq!(r.files, 2);
    assert_eq!(std::fs::read(out.join("kit/inner/deep.stl")).unwrap(), b"deep");
    let left: Vec<_> = std::fs::read_dir(out.join("kit")).unwrap().map(|e| e.unwrap().file_name()).collect();
    assert!(!left.iter().any(|n| n.to_string_lossy().ends_with(".part")), "{left:?}");
}

#[test]
fn vanished_entry_is_a_failed_file() {
    let tmp = tempfile::tempdir().unwrap();
    let conn = indexed(tmp.path());
    let files = resolve_plan(&conn, &ExportPlan { items: vec![file("gone.stl", 9999)], skipped: 0 }).unwrap();
    assert!(!files[0].is_dir && files[0].loc.is_none());
    let r = write(&tmp.path().join("out"), &files);
    assert_eq!(r.failed.len(), 1);
}

#[test]
fn write_tree_stops_on_cancel() {
    let tmp = tempfile::tempdir().unwrap();
    let conn = indexed(tmp.path());
    let files = resolve_plan(&conn, &plan(&conn)).unwrap();
    let out = tmp.path().join("out");
    let cancel = AtomicBool::new(false);
    let mut calls = 0;
    let r = write_tree(&out, &files, &mut ExtractSession::new(), &mut |_, _| {
        calls += 1;
        cancel.store(true, Ordering::SeqCst);
    }, &cancel);
    assert_eq!(calls, 1);
    assert_eq!((r.files, r.dirs), (0, 0));
    assert!(!out.join("kit").exists());
}

#[test]
fn unique_dest_picks_free_name() {
    let tmp = tempfile::tempdir().unwrap();
    let d = tmp.path();
    assert_eq!(unique_dest(d, "orc", true), d.join("orc"));
    std::fs::create_dir(d.join("orc")).unwrap();
    assert_eq!(unique_dest(d, "orc", true), d.join("orc (2)"));
    std::fs::write(d.join("elf.stl"), b"").unwrap();
    std::fs::write(d.join("elf (2).stl"), b"").unwrap();
    assert_eq!(unique_dest(d, "elf.stl", false), d.join("elf (3).stl"));
    std::fs::create_dir(d.join("v1.2")).unwrap();
    assert_eq!(unique_dest(d, "v1.2", true), d.join("v1.2 (2)"));
}
