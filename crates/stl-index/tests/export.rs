mod common;

use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};

use common::{put, sevenz, zip};
use rusqlite::Connection;
use stl_core::vtree::export::{ExportItem, ExportKind, ExportPlan};
use stl_index::export::{EntryLoc, ExportFile, FileServer, batch_key, locate, resolve_plan, unique_dest, write_tree};
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

/// `<tmp>/lib/Vol/Coll/{Pack.7z (a.stl, b.stl, c.stl, n.zip (d.stl)), Kit.zip (k.stl)}`, indexed.
fn indexed_7z(tmp: &Path) -> Connection {
    let n = zip(&[("d.stl", b"dd")]);
    put(tmp, "lib/Vol/Coll/Pack.7z", &sevenz(&[("a.stl", b"aa"), ("b.stl", b"bb"), ("c.stl", b"cc"), ("n.zip", &n)]));
    put(tmp, "lib/Vol/Coll/Kit.zip", &zip(&[("k.stl", b"kk")]));
    let mut conn = db::open(&tmp.join("index.db")).unwrap();
    scanstore::add_root(&conn, &tmp.join("lib/Vol")).unwrap();
    resolvestore::index_all(&mut conn, &ListOptions::default(), &mut |_| {}).unwrap();
    conn
}

/// `p/{a.stl, k.stl, b.stl, d.stl, c.stl}`: the 7z's files interleaved with others.
fn plan_7z(conn: &Connection) -> ExportPlan {
    ExportPlan {
        items: vec![
            dir("p"),
            file("p/a.stl", id(conn, "a.stl")),
            file("p/k.stl", id(conn, "k.stl")),
            file("p/b.stl", id(conn, "b.stl")),
            file("p/d.stl", id(conn, "n.zip/d.stl")),
            file("p/c.stl", id(conn, "c.stl")),
        ],
        skipped: 0,
    }
}

fn loc(kind: SourceKind, source: &str, chain: &[&str]) -> EntryLoc {
    EntryLoc { kind, source_path: source.into(), chain: chain.iter().map(|c| c.to_string()).collect(), size: Some(1) }
}

#[test]
fn batch_key_only_for_sequential_archives() {
    let k = batch_key(&loc(SourceKind::Archive, "D:/x/P.7z", &["a.stl"])).unwrap();
    assert_eq!(k, ("D:/x/P.7z".to_string(), vec![]));
    assert!(batch_key(&loc(SourceKind::Archive, "D:/x/p.RAR", &["dir/a.stl"])).is_some());
    assert!(batch_key(&loc(SourceKind::Archive, "D:/x/k.zip", &["a.stl"])).is_none());
    assert!(batch_key(&loc(SourceKind::Archive, "D:/x/P.7z", &["n.zip", "d.stl"])).is_none());
    assert!(batch_key(&loc(SourceKind::Archive, "D:/x/k.zip", &["in/n.7z", "d.stl"])).unwrap().1 == ["in/n.7z"]);
    assert!(batch_key(&loc(SourceKind::Folder, "D:/x", &["a.stl"])).is_none());
}

#[test]
fn write_tree_reads_a_7z_in_one_pass() {
    let tmp = tempfile::tempdir().unwrap();
    let conn = indexed_7z(tmp.path());
    let files = resolve_plan(&conn, &plan_7z(&conn)).unwrap();
    let out = tmp.path().join("out");
    let mut session = ExtractSession::new();
    let mut seen = Vec::new();
    let r = write_tree(&out, &files, &mut session, &mut |i, rel| seen.push((i, rel.to_string())), &AtomicBool::new(false));
    assert!(r.failed.is_empty(), "{:?}", r.failed);
    assert_eq!((r.files, r.dirs, r.batched), (5, 1, 3));
    // only k.stl (zip) and d.stl (zip in the 7z) went through the session: 7z + n.zip + Kit.zip
    assert_eq!(session.opens(), 3);
    for (name, body) in [("a", b"aa"), ("b", b"bb"), ("c", b"cc"), ("d", b"dd"), ("k", b"kk")] {
        assert_eq!(std::fs::read(out.join(format!("p/{name}.stl"))).unwrap(), body);
    }
    // progress counts every item once, in the order written
    assert_eq!(seen.iter().map(|(i, _)| *i).collect::<Vec<_>>(), [0, 1, 2, 3, 4, 5]);
    assert_eq!(seen.len(), files.len());
}

#[test]
fn file_server_reads_a_7z_once_for_a_drag() {
    let tmp = tempfile::tempdir().unwrap();
    let conn = indexed_7z(tmp.path());
    let files = resolve_plan(&conn, &plan_7z(&conn)).unwrap();
    let mut server = FileServer::new(files, 1 << 30);
    let get = |s: &mut FileServer, i: usize| {
        let mut out = Vec::new();
        s.write(i, &mut out).unwrap();
        out
    };
    // Explorer may ask in any order
    assert_eq!(get(&mut server, 3), b"bb");
    assert_eq!(server.batches(), 1);
    assert_eq!(get(&mut server, 5), b"cc");
    assert_eq!(get(&mut server, 1), b"aa");
    assert_eq!(get(&mut server, 2), b"kk");
    assert_eq!(get(&mut server, 4), b"dd");
    assert_eq!(server.batches(), 1);
    assert_eq!(server.session_opens(), 3); // Kit.zip, then the 7z and n.zip for d.stl
    assert_eq!(get(&mut server, 1), b"aa"); // asked again: extracted again
    assert!(server.write(0, &mut Vec::new()).is_err()); // a folder
}

#[test]
fn file_server_respects_the_cache_limit() {
    let tmp = tempfile::tempdir().unwrap();
    let conn = indexed_7z(tmp.path());
    let files = resolve_plan(&conn, &plan_7z(&conn)).unwrap();
    let mut server = FileServer::new(files, 2); // room for one other file
    for (i, body) in [(1, b"aa"), (3, b"bb"), (5, b"cc")] {
        let mut out = Vec::new();
        server.write(i, &mut out).unwrap();
        assert_eq!(out, body);
    }
    assert_eq!(server.batches(), 1);
    assert_eq!(server.session_opens(), 1); // c.stl didn't fit the cache
}
