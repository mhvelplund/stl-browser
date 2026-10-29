mod common;

use std::path::PathBuf;

use common::{count, put, rewrite, zip};
use rusqlite::Connection;
use stl_index::resolvestore::{self, IndexStats};
use stl_index::{db, scanstore};
use stl_scan::nested::ListOptions;

struct Fixture {
    _tmp: tempfile::TempDir,
    root: PathBuf,
    conn: Connection,
}

fn fixture() -> Fixture {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().join("Vol");
    put(&root, "A/Kit.zip", &zip(&[("Kit/Kit.jpg", b"j"), ("Kit/a.stl", b"a"), ("Kit/b.stl", b"b")]));
    put(&root, "A/Two.zip", &zip(&[("Two/t.stl", b"t")]));
    // a collection holding a single archive resolves to one model named after the collection
    put(&root, "B/Other.zip", &zip(&[("Other/x.stl", b"x")]));
    let conn = db::open(&tmp.path().join("index.db")).unwrap();
    scanstore::add_root(&conn, &root).unwrap();
    Fixture { _tmp: tmp, root, conn }
}

fn index(f: &mut Fixture) -> IndexStats {
    resolvestore::index_all(&mut f.conn, &ListOptions::default(), &mut |_| {}).unwrap()
}

fn model_id(conn: &Connection, name: &str) -> i64 {
    conn.query_row("SELECT id FROM models WHERE name = ?", [name], |r| r.get(0)).unwrap()
}

#[test]
fn index_resolves_collections() {
    let mut f = fixture();
    let s = index(&mut f);
    assert_eq!((s.resolved, s.models), (2, 3));
    let (images, hero, files, crumb): (i64, Option<i64>, i64, String) = f
        .conn
        .query_row("SELECT image_count, hero_asset_id, file_count, breadcrumb FROM models WHERE name = 'Kit'", [], |r| {
            Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?))
        })
        .unwrap();
    assert_eq!((images, files, crumb.as_str()), (1, 2, "Vol / A"));
    assert!(hero.is_some());
    assert_eq!(count(&f.conn, "SELECT count(*) FROM models WHERE name = 'B' AND group_id IS NULL"), 1);
    assert_eq!(
        count(&f.conn, "SELECT count(*) FROM model_files mf LEFT JOIN entries e ON e.id = mf.entry_id WHERE e.id IS NULL"),
        0
    );
}

#[test]
fn unchanged_rescan_resolves_nothing() {
    let mut f = fixture();
    index(&mut f);
    let kit = model_id(&f.conn, "Kit");
    let s = index(&mut f);
    assert_eq!(s.resolved, 0);
    assert_eq!(model_id(&f.conn, "Kit"), kit);
}

#[test]
fn changed_collection_only_is_reresolved() {
    let mut f = fixture();
    index(&mut f);
    let kit = model_id(&f.conn, "Kit");
    rewrite(&f.root, "B/Other.zip", &zip(&[("Other/x.stl", b"x"), ("Other/y.stl", b"y")]));
    let s = index(&mut f);
    assert_eq!(s.resolved, 1);
    assert_eq!(model_id(&f.conn, "Kit"), kit);
}

#[test]
fn change_scanned_but_not_resolved_is_resolved_next_run() {
    // an index run interrupted after the scan stored a changed source, before resolving
    let mut f = fixture();
    index(&mut f);
    rewrite(&f.root, "A/Kit.zip", &zip(&[("Kit/Kit.jpg", b"j"), ("Kit/a.stl", b"a"), ("Kit/c.stl", b"c")]));
    let root_id: i64 = f.conn.query_row("SELECT id FROM roots", [], |r| r.get(0)).unwrap();
    scanstore::scan_root(&mut f.conn, root_id, &ListOptions::default(), &mut |_| {}).unwrap();
    let s = index(&mut f);
    assert_eq!(s.resolved, 1);
    assert_eq!(
        count(&f.conn, "SELECT count(*) FROM model_files mf LEFT JOIN entries e ON e.id = mf.entry_id WHERE e.id IS NULL"),
        0
    );
    assert_eq!(count(&f.conn, "SELECT count(*) FROM model_files mf JOIN models m ON m.id = mf.model_id WHERE m.name = 'Kit'"), 2);
}

#[test]
fn removed_collection_loses_rows() {
    let mut f = fixture();
    index(&mut f);
    std::fs::remove_file(f.root.join("B/Other.zip")).unwrap();
    index(&mut f);
    for t in ["models", "groups", "assets"] {
        assert_eq!(count(&f.conn, &format!("SELECT count(*) FROM {t} WHERE collection = 'Vol/B'")), 0, "{t}");
    }
    assert_eq!(count(&f.conn, "SELECT count(*) FROM models"), 2);
}

#[test]
fn collection_that_is_one_model_has_no_group() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().join("Vol");
    put(&root, "A/Solo.zip", &zip(&[("Solo/a.stl", b"a")]));
    let mut conn = db::open(&tmp.path().join("index.db")).unwrap();
    scanstore::add_root(&conn, &root).unwrap();
    resolvestore::index_all(&mut conn, &ListOptions::default(), &mut |_| {}).unwrap();
    assert_eq!(count(&conn, "SELECT count(*) FROM models WHERE group_id IS NULL"), 1);
}

#[test]
fn cancelled_index_resolves_what_it_listed() {
    let mut f = fixture();
    let cancel = std::sync::atomic::AtomicBool::new(false);
    let run = scanstore::ScanRun { cancel: Some(&cancel), ..Default::default() };
    // stop once the first source is being listed
    let s = resolvestore::index_run(&mut f.conn, &ListOptions::default(), &run, &mut |p| {
        if let scanstore::Progress::Source { .. } = p {
            cancel.store(true, std::sync::atomic::Ordering::SeqCst);
        }
    })
    .unwrap();
    assert!(s.scan.cancelled);
    assert_eq!((s.scan.listed, s.resolved), (1, 1));
    assert_eq!(count(&f.conn, "SELECT count(*) FROM models WHERE collection = 'Vol/A'"), 1);
    // the next run picks up the rest
    let s = index(&mut f);
    assert!(!s.scan.cancelled);
    assert_eq!(s.scan.listed, 2);
}

#[test]
fn scoped_index_resolves_only_the_scope() {
    let mut f = fixture();
    index(&mut f);
    rewrite(&f.root, "A/Two.zip", &zip(&[("Two/t.stl", b"t"), ("Two/u.stl", b"u")]));
    rewrite(&f.root, "B/Other.zip", &zip(&[("Other/x.stl", b"x"), ("Other/y.stl", b"y")]));
    let scope = vec![f.root.join("B")];
    let run = scanstore::ScanRun { scope: Some(&scope), ..Default::default() };
    let s = resolvestore::index_run(&mut f.conn, &ListOptions::default(), &run, &mut |_| {}).unwrap();
    assert_eq!((s.scan.listed, s.resolved), (1, 1));
}
