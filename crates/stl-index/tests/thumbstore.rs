mod common;

use std::path::{Path, PathBuf};

use common::{put, rewrite, zip};
use rusqlite::Connection;
use stl_index::thumbstore::{lookup, record};
use stl_index::{db, resolvestore, scanstore};
use stl_scan::extract::SourceKind;
use stl_scan::nested::ListOptions;
use stl_thumb::cache::{cache_rel, content_name, location_dir};

/// `<tmp>/lib/Vol/Coll/{Kit.zip (Kit/a.png, Kit/b.stl), c.png}`, indexed.
fn indexed(tmp: &Path) -> Connection {
    put(tmp, "lib/Vol/Coll/Kit.zip", &zip(&[("Kit/a.png", b"png-a"), ("Kit/b.stl", b"stl-b")]));
    put(tmp, "lib/Vol/Coll/c.png", b"png-c");
    put(tmp, "lib/Vol/Coll/notes.txt", b"t");
    let mut conn = db::open(&tmp.join("index.db")).unwrap();
    scanstore::add_root(&conn, &tmp.join("lib/Vol")).unwrap();
    resolvestore::index_all(&mut conn, &ListOptions::default(), &mut |_| {}).unwrap();
    conn
}

fn id(conn: &Connection, path: &str) -> i64 {
    conn.query_row("SELECT id FROM entries WHERE path = ?", [path], |r| r.get(0)).unwrap()
}

fn thumbs(tmp: &Path) -> PathBuf {
    tmp.join("thumbs")
}

/// Pretend a worker wrote the file.
fn touch(dir: &Path, rel: &str) {
    let p = dir.join(rel);
    std::fs::create_dir_all(p.parent().unwrap()).unwrap();
    std::fs::write(p, b"webp").unwrap();
}

#[test]
fn jobs_for_new_entries() {
    let tmp = tempfile::tempdir().unwrap();
    let conn = indexed(tmp.path());
    let ids = [id(&conn, "Kit/a.png"), id(&conn, "Kit/b.stl"), id(&conn, "c.png"), id(&conn, "notes.txt"), 9999];
    let l = lookup(&conn, &thumbs(tmp.path()), &ids).unwrap();
    assert!(l.ready.is_empty() && l.failed.is_empty());
    assert_eq!(l.jobs.len(), 3, "{:?}", l.jobs); // notes.txt has no thumbnail, 9999 doesn't exist
    let (a, b, c) = (&l.jobs[0], &l.jobs[1], &l.jobs[2]);
    assert_eq!(a.group(), b.group());
    assert_eq!(a.source_kind, SourceKind::Archive);
    assert_eq!(a.chain, ["Kit/a.png"]);
    assert_eq!(a.size, 5);
    assert_eq!(a.crc.len(), 8);
    assert_eq!(c.source_kind, SourceKind::Folder);
    assert_eq!(c.chain, ["c.png"]);
    assert!(c.source_path.ends_with("Coll"), "{}", c.source_path);
}

#[test]
fn record_then_ready() {
    let tmp = tempfile::tempdir().unwrap();
    let conn = indexed(tmp.path());
    let c = id(&conn, "c.png");
    let rel = "0123456789abcdef/0123456789abcdef0123456789abcdef.webp";
    touch(&thumbs(tmp.path()), rel);
    record(&conn, c, &Ok(rel.to_string())).unwrap();
    let l = lookup(&conn, &thumbs(tmp.path()), &[c]).unwrap();
    assert_eq!(l.ready, [(c, rel.to_string())]);
    assert!(l.jobs.is_empty());
}

#[test]
fn missing_file_is_not_a_hit() {
    let tmp = tempfile::tempdir().unwrap();
    let conn = indexed(tmp.path());
    let c = id(&conn, "c.png");
    record(&conn, c, &Ok("0123456789abcdef/0123456789abcdef0123456789abcdef.webp".into())).unwrap();
    let l = lookup(&conn, &thumbs(tmp.path()), &[c]).unwrap();
    assert!(l.ready.is_empty());
    assert_eq!(l.jobs.len(), 1);
    let rows: i64 = conn.query_row("SELECT count(*) FROM thumbs", [], |r| r.get(0)).unwrap();
    assert_eq!(rows, 0);
}

#[test]
fn crc_shortcut_finds_existing_file() {
    let tmp = tempfile::tempdir().unwrap();
    let conn = indexed(tmp.path());
    let a = id(&conn, "Kit/a.png");
    let l = lookup(&conn, &thumbs(tmp.path()), &[a]).unwrap();
    let j = &l.jobs[0];
    let rel = cache_rel(&location_dir(&j.source_path, &j.chain), &content_name(&j.crc, j.size).unwrap());
    touch(&thumbs(tmp.path()), &rel);
    let l = lookup(&conn, &thumbs(tmp.path()), &[a]).unwrap();
    assert_eq!(l.ready, [(a, rel.clone())]);
    let stored: String = conn.query_row("SELECT cache_rel FROM thumbs WHERE entry_id = ?", [a], |r| r.get(0)).unwrap();
    assert_eq!(stored, rel);
}

#[test]
fn failed_is_not_retried() {
    let tmp = tempfile::tempdir().unwrap();
    let mut conn = indexed(tmp.path());
    let a = id(&conn, "Kit/a.png");
    record(&conn, a, &Err("bad image".into())).unwrap();
    let l = lookup(&conn, &thumbs(tmp.path()), &[a]).unwrap();
    assert_eq!(l.failed, [a]);
    assert!(l.jobs.is_empty());
    // the archive changes: re-listed entries get new ids and the old row goes with its entry
    rewrite(tmp.path(), "lib/Vol/Coll/Kit.zip", &zip(&[("Kit/a.png", b"png-a2"), ("Kit/b.stl", b"stl-b")]));
    resolvestore::index_all(&mut conn, &ListOptions::default(), &mut |_| {}).unwrap();
    let a2 = id(&conn, "Kit/a.png");
    assert_ne!(a, a2);
    let l = lookup(&conn, &thumbs(tmp.path()), &[a2]).unwrap();
    assert_eq!(l.jobs.len(), 1);
    let rows: i64 = conn.query_row("SELECT count(*) FROM thumbs", [], |r| r.get(0)).unwrap();
    assert_eq!(rows, 0);
}

#[test]
fn record_for_a_vanished_entry_is_ignored() {
    let tmp = tempfile::tempdir().unwrap();
    let conn = indexed(tmp.path());
    record(&conn, 9999, &Ok("0123456789abcdef/x.webp".into())).unwrap();
}
