mod common;

use common::{put, rewrite, zip};
use stl_core::vtree::tags::{Tag, TagKind};
use stl_index::resolvestore;
use stl_index::tagstore;
use stl_index::{db, scanstore};
use stl_scan::nested::ListOptions;

fn open() -> (tempfile::TempDir, rusqlite::Connection) {
    let dir = tempfile::tempdir().unwrap();
    let conn = db::open(&dir.path().join("index.db")).unwrap();
    (dir, conn)
}

fn keys(k: &[&str]) -> Vec<String> {
    k.iter().map(|s| s.to_string()).collect()
}

#[test]
fn add_and_load() {
    let (_d, conn) = open();
    tagstore::add(&conn, &keys(&["r:a", "r:b"]), TagKind::User, "  Necrons ").unwrap();
    let t = tagstore::load(&conn).unwrap();
    let want = vec![Tag { kind: TagKind::User, name: "necrons".into(), by_user: true }];
    assert_eq!(t.set["r:a"], want);
    assert_eq!(t.set["r:b"], want);
}

#[test]
fn setting_a_kind_replaces_the_old_value() {
    let (_d, conn) = open();
    let k = keys(&["r:a"]);
    tagstore::add(&conn, &k, TagKind::Scale, "25mm").unwrap();
    tagstore::add(&conn, &k, TagKind::Scale, "32mm").unwrap();
    let t = tagstore::load(&conn).unwrap();
    assert_eq!(t.set["r:a"], vec![Tag { kind: TagKind::Scale, name: "32mm".into(), by_user: true }]);
    assert_eq!(tagstore::names(&conn).unwrap(), vec![(TagKind::Scale, "32mm".to_string())]);
}

#[test]
fn user_tags_accumulate() {
    let (_d, conn) = open();
    let k = keys(&["r:a"]);
    tagstore::add(&conn, &k, TagKind::User, "one").unwrap();
    tagstore::add(&conn, &k, TagKind::User, "two").unwrap();
    assert_eq!(tagstore::load(&conn).unwrap().set["r:a"].len(), 2);
}

#[test]
fn remove_drops_unused_names() {
    let (_d, conn) = open();
    let k = keys(&["r:a"]);
    tagstore::add(&conn, &k, TagKind::User, "x").unwrap();
    tagstore::remove(&conn, &k, TagKind::User, "X").unwrap();
    assert!(tagstore::names(&conn).unwrap().is_empty());
    assert!(tagstore::load(&conn).unwrap().set.is_empty());
}

#[test]
fn remove_keeps_names_still_in_use() {
    let (_d, conn) = open();
    tagstore::add(&conn, &keys(&["r:a", "r:b"]), TagKind::User, "x").unwrap();
    tagstore::remove(&conn, &keys(&["r:a"]), TagKind::User, "x").unwrap();
    assert_eq!(tagstore::names(&conn).unwrap().len(), 1);
    assert!(tagstore::load(&conn).unwrap().set.contains_key("r:b"));
}

#[test]
fn hide_and_unhide() {
    let (_d, conn) = open();
    let k = keys(&["r:a"]);
    tagstore::hide(&conn, &k, TagKind::Support, "supported").unwrap();
    assert_eq!(tagstore::load(&conn).unwrap().hidden["r:a"], vec![(TagKind::Support, "supported".to_string())]);
    tagstore::unhide(&conn, &k, TagKind::Support, "supported").unwrap();
    assert!(tagstore::load(&conn).unwrap().hidden.is_empty());
}

#[test]
fn edition_and_empty_are_refused() {
    let (_d, conn) = open();
    let k = keys(&["r:a"]);
    assert!(tagstore::add(&conn, &k, TagKind::Edition, "5e").is_err());
    assert!(tagstore::add(&conn, &k, TagKind::User, "  ").is_err());
    assert!(tagstore::load(&conn).unwrap().set.is_empty());
}

#[test]
fn user_data_survives_reindex() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().join("Vol");
    put(&root, "A/Kit.zip", &zip(&[("Kit/a.stl", b"a")]));
    let mut conn = db::open(&tmp.path().join("index.db")).unwrap();
    scanstore::add_root(&conn, &root).unwrap();
    resolvestore::index_all(&mut conn, &ListOptions::default(), &mut |_| {}).unwrap();
    tagstore::add(&conn, &keys(&["r:vol\\a\\kit.zip"]), TagKind::User, "painted").unwrap();
    tagstore::hide(&conn, &keys(&["r:vol\\a\\kit.zip"]), TagKind::Support, "supported").unwrap();
    rewrite(&root, "A/Kit.zip", &zip(&[("Kit/a.stl", b"a"), ("Kit/b.stl", b"b")]));
    resolvestore::index_all(&mut conn, &ListOptions::default(), &mut |_| {}).unwrap();
    let t = tagstore::load(&conn).unwrap();
    assert_eq!(t.set["r:vol\\a\\kit.zip"].len(), 1);
    assert_eq!(t.hidden["r:vol\\a\\kit.zip"].len(), 1);
}
