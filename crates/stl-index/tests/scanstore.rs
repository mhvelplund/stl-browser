use std::io::{Cursor, Write};
use std::path::Path;

use rusqlite::Connection;
use stl_index::db;
use stl_index::scanstore::{self, IndexError, Progress, ScanStats};
use stl_scan::nested::ListOptions;

fn zip(files: &[(&str, &[u8])]) -> Vec<u8> {
    let mut w = zip::ZipWriter::new(Cursor::new(Vec::new()));
    for (name, data) in files {
        w.start_file(*name, zip::write::SimpleFileOptions::default())
            .unwrap();
        w.write_all(data).unwrap();
    }
    w.finish().unwrap().into_inner()
}

fn put(root: &Path, rel: &str, bytes: &[u8]) {
    let p = root.join(rel);
    std::fs::create_dir_all(p.parent().unwrap()).unwrap();
    std::fs::write(p, bytes).unwrap();
}

struct Fixture {
    _tmp: tempfile::TempDir,
    root: std::path::PathBuf,
    conn: Connection,
    root_id: i64,
}

fn fixture() -> Fixture {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().join("Vol");
    put(
        &root,
        "A/one.zip",
        &zip(&[("Kit/a.stl", b"a"), ("Kit/a.jpg", b"j")]),
    );
    put(&root, "A/two.zip", &zip(&[("b.stl", b"b")]));
    put(&root, "A/loose.stl", b"loose");
    let conn = db::open(&tmp.path().join("index.db")).unwrap();
    let root_id = scanstore::add_root(&conn, &root).unwrap();
    Fixture {
        _tmp: tmp,
        root,
        conn,
        root_id,
    }
}

fn scan(f: &mut Fixture) -> Result<ScanStats, IndexError> {
    scanstore::scan_root(
        &mut f.conn,
        f.root_id,
        &ListOptions::default(),
        &mut |_: Progress| {},
    )
}

fn count(conn: &Connection, sql: &str) -> i64 {
    conn.query_row(sql, [], |r| r.get(0)).unwrap()
}

fn entries_of(conn: &Connection, source_suffix: &str) -> i64 {
    conn.query_row(
        "SELECT count(*) FROM entries e JOIN sources s ON s.id = e.source_id WHERE s.path LIKE '%' || ?",
        [source_suffix],
        |r| r.get(0),
    )
    .unwrap()
}

#[test]
fn add_root_is_idempotent() {
    let f = fixture();
    assert_eq!(scanstore::add_root(&f.conn, &f.root).unwrap(), f.root_id);
    let name: String = f
        .conn
        .query_row("SELECT name FROM roots", [], |r| r.get(0))
        .unwrap();
    assert_eq!(name, "Vol");
}

#[test]
fn first_scan_lists_all() {
    let mut f = fixture();
    let s = scan(&mut f).unwrap();
    assert_eq!(
        (s.sources, s.listed, s.reused, s.removed, s.errors),
        (3, 3, 0, 0, 0)
    );
    assert_eq!(count(&f.conn, "SELECT count(*) FROM entries"), 4);
    let (chain, kind): (String, String) = f
        .conn
        .query_row("SELECT e.chain, s.kind FROM entries e JOIN sources s ON s.id = e.source_id WHERE e.path = 'Kit/a.stl'", [], |r| {
            Ok((r.get(0)?, r.get(1)?))
        })
        .unwrap();
    assert_eq!(chain, r#"["Kit/a.stl"]"#);
    assert_eq!(kind, "archive");
}

#[test]
fn rescan_unchanged_reuses() {
    let mut f = fixture();
    scan(&mut f).unwrap();
    let ids = count(&f.conn, "SELECT sum(id) FROM entries");
    let s = scan(&mut f).unwrap();
    assert_eq!((s.listed, s.reused), (0, 3));
    assert_eq!(count(&f.conn, "SELECT sum(id) FROM entries"), ids);
}

#[test]
fn modified_archive_relisted() {
    let mut f = fixture();
    scan(&mut f).unwrap();
    let p = f.root.join("A/two.zip");
    std::fs::write(&p, zip(&[("b.stl", b"b"), ("c.stl", b"c")])).unwrap();
    let later = filetime::FileTime::from_unix_time(
        filetime::FileTime::from_last_modification_time(&std::fs::metadata(&p).unwrap())
            .unix_seconds()
            + 10,
        0,
    );
    filetime::set_file_mtime(&p, later).unwrap();
    let s = scan(&mut f).unwrap();
    assert_eq!((s.listed, s.reused), (1, 2));
    assert_eq!(entries_of(&f.conn, "two.zip"), 2);
}

#[test]
fn renamed_loose_file_relisted() {
    let mut f = fixture();
    scan(&mut f).unwrap();
    // same size, same mtime: only the name changes
    std::fs::rename(f.root.join("A/loose.stl"), f.root.join("A/renamed.stl")).unwrap();
    let s = scan(&mut f).unwrap();
    assert_eq!((s.listed, s.reused), (1, 2));
    let names: Vec<String> = {
        let mut st = f.conn.prepare("SELECT e.path FROM entries e JOIN sources s ON s.id = e.source_id WHERE s.kind = 'folder'").unwrap();
        st.query_map([], |r| r.get(0))
            .unwrap()
            .map(Result::unwrap)
            .collect()
    };
    assert_eq!(names, ["renamed.stl"]);
}

#[test]
fn deleted_archive_removed() {
    let mut f = fixture();
    scan(&mut f).unwrap();
    std::fs::remove_file(f.root.join("A/two.zip")).unwrap();
    let s = scan(&mut f).unwrap();
    assert_eq!(s.removed, 1);
    assert_eq!(entries_of(&f.conn, "two.zip"), 0);
    assert_eq!(count(&f.conn, "SELECT count(*) FROM sources"), 2);
}

#[test]
fn corrupt_archive_recorded() {
    let mut f = fixture();
    put(&f.root, "A/bad.zip", b"this is not a zip file at all");
    let s = scan(&mut f).unwrap();
    assert_eq!(s.errors, 1);
    let err: Option<String> = f
        .conn
        .query_row(
            "SELECT error FROM sources WHERE path LIKE '%bad.zip'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert!(err.is_some());
    assert_eq!(entries_of(&f.conn, "bad.zip"), 0);
    assert_eq!(entries_of(&f.conn, "one.zip"), 2);
}

#[test]
fn errored_source_relisted_after_recovery() {
    let mut f = fixture();
    let p = f.root.join("A/two.zip");
    let good = std::fs::read(&p).unwrap();
    let mtime = filetime::FileTime::from_last_modification_time(&std::fs::metadata(&p).unwrap());
    // a transient failure: same size and mtime, unreadable content
    std::fs::write(&p, vec![0u8; good.len()]).unwrap();
    filetime::set_file_mtime(&p, mtime).unwrap();
    let s = scan(&mut f).unwrap();
    assert_eq!(s.errors, 1);
    std::fs::write(&p, &good).unwrap();
    filetime::set_file_mtime(&p, mtime).unwrap();
    let s = scan(&mut f).unwrap();
    assert_eq!((s.listed, s.errors), (1, 0));
    assert_eq!(entries_of(&f.conn, "two.zip"), 1);
}

fn zip_encrypted() -> Vec<u8> {
    let mut w = zip::ZipWriter::new(Cursor::new(Vec::new()));
    let opts = zip::write::SimpleFileOptions::default().with_aes_encryption(zip::AesMode::Aes256, "pw");
    w.start_file("secret.stl", opts).unwrap();
    w.write_all(b"secret").unwrap();
    w.finish().unwrap().into_inner()
}

#[test]
fn password_protected_archive_stored_locked() {
    let mut f = fixture();
    put(&f.root, "A/locked.zip", &zip_encrypted());
    put(&f.root, "A/outer.zip", &zip(&[("inner/locked.zip", &zip_encrypted()), ("inner/ok.stl", b"k")]));
    let s = scan(&mut f).unwrap();
    assert_eq!((s.locked, s.errors), (1, 0));
    let (locked, err): (bool, Option<String>) =
        f.conn.query_row("SELECT locked, error FROM sources WHERE path LIKE '%locked.zip'", [], |r| Ok((r.get(0)?, r.get(1)?))).unwrap();
    assert!(locked);
    assert_eq!(err, None);
    assert_eq!(entries_of(&f.conn, "locked.zip"), 0);
    let nested_locked: bool =
        f.conn.query_row("SELECT locked FROM entries WHERE path = 'inner/locked.zip'", [], |r| r.get(0)).unwrap();
    assert!(nested_locked);
    // never retried while unchanged
    let s = scan(&mut f).unwrap();
    assert_eq!((s.listed, s.locked), (0, 0));
}

#[test]
fn missing_root_is_error_and_keeps_data() {
    let mut f = fixture();
    scan(&mut f).unwrap();
    std::fs::rename(&f.root, f.root.with_file_name("Gone")).unwrap();
    assert!(matches!(scan(&mut f), Err(IndexError::RootMissing(_))));
    assert_eq!(count(&f.conn, "SELECT count(*) FROM sources"), 3);
}
