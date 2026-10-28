mod common;

use std::path::Path;

use common::{put, zip, zip_encrypted};
use rusqlite::Connection;
use stl_core::vtree::{NodeId, NodeKind, Tree};
use stl_index::{db, resolvestore, scanstore, views};
use stl_scan::nested::ListOptions;

/// `<base>/A/{Kit.zip, Two.zip}` so that `A` is a group holding the models `Kit` and `Two`.
fn kits(base: &Path) {
    put(base, "A/Kit.zip", &zip(&[("Kit/Kit.jpg", b"j"), ("Kit/a.stl", b"a")]));
    put(base, "A/Two.zip", &zip(&[("Two/t.stl", b"t")]));
}

fn indexed(tmp: &Path, roots: &[&str]) -> Connection {
    let mut conn = db::open(&tmp.join("index.db")).unwrap();
    for r in roots {
        scanstore::add_root(&conn, &tmp.join(r)).unwrap();
    }
    resolvestore::index_all(&mut conn, &ListOptions::default(), &mut |_| {}).unwrap();
    conn
}

fn at(t: &Tree, path: &str) -> NodeId {
    t.find(path).unwrap_or_else(|| panic!("no node at {path}"))
}

fn names(t: &Tree, id: NodeId) -> Vec<String> {
    t.nodes[id].children.iter().map(|&c| t.nodes[c].name.clone()).collect()
}

#[test]
fn models_view_lists_volume_collection_model() {
    let tmp = tempfile::tempdir().unwrap();
    kits(&tmp.path().join("Vol"));
    let conn = indexed(tmp.path(), &["Vol"]);
    let t = views::models_view(&conn).unwrap();
    let kit = at(&t, "vol/a/kit");
    assert_eq!(t.nodes[kit].kind, NodeKind::Model);
    assert_eq!(names(&t, kit)[0], "images");
}

#[test]
fn model_hero_entry_is_set() {
    let tmp = tempfile::tempdir().unwrap();
    kits(&tmp.path().join("Vol"));
    let conn = indexed(tmp.path(), &["Vol"]);
    let t = views::models_view(&conn).unwrap();
    let kit = &t.nodes[at(&t, "vol/a/kit")].info;
    assert!(kit.hero_key.as_deref().unwrap().ends_with("|Kit/Kit.jpg"), "{:?}", kit.hero_key);
    assert!(kit.hero_entry_id.is_some());
    assert_eq!(t.nodes[at(&t, "vol/a/two")].info.hero_key, None);
}

#[test]
fn poster_is_the_hero_of_its_group_and_of_models_without_images() {
    let tmp = tempfile::tempdir().unwrap();
    kits(&tmp.path().join("Vol"));
    put(&tmp.path().join("Vol"), "A/poster.jpg", b"p");
    let conn = indexed(tmp.path(), &["Vol"]);
    let t = views::models_view(&conn).unwrap();
    let coll = &t.nodes[at(&t, "vol/a")].info;
    assert!(coll.hero_key.as_deref().unwrap_or("").ends_with("poster.jpg"), "{:?}", coll.hero_key);
    let two = &t.nodes[at(&t, "vol/a/two")].info;
    assert!(two.hero_key.as_deref().unwrap_or("").ends_with("poster.jpg"), "{:?}", two.hero_key);
}

#[test]
fn files_view_shows_archive_as_folder() {
    let tmp = tempfile::tempdir().unwrap();
    kits(&tmp.path().join("Vol"));
    let conn = indexed(tmp.path(), &["Vol"]);
    let t = views::files_view(&conn).unwrap();
    let f = at(&t, "vol/a/kit/kit/a.stl");
    assert_eq!(t.nodes[f].kind, NodeKind::File);
    assert!(t.nodes[f].info.entry_id.is_some());
}

#[test]
fn inner_root_volume_aliases_outer_content() {
    let tmp = tempfile::tempdir().unwrap();
    kits(&tmp.path().join("Outer/Vol"));
    put(&tmp.path().join("Outer"), "Other/o.zip", &zip(&[("o/o.stl", b"o")]));
    let conn = indexed(tmp.path(), &["Outer", "Outer/Vol"]);
    for t in [views::models_view(&conn).unwrap(), views::files_view(&conn).unwrap()] {
        assert_eq!(names(&t, 0), ["outer", "vol"]);
        let inner = names(&t, at(&t, "vol"));
        assert!(!inner.is_empty());
        assert_eq!(inner, names(&t, at(&t, "outer/vol")));
    }
}

#[test]
fn locked_source_shows_in_both_views() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().join("Vol");
    kits(&root);
    put(&root, "A/Secret.zip", &zip_encrypted());
    let conn = indexed(tmp.path(), &["Vol"]);
    let files = views::files_view(&conn).unwrap();
    assert_eq!(files.nodes[at(&files, "vol/a/secret")].kind, NodeKind::Locked);
    let models = views::models_view(&conn).unwrap();
    assert_eq!(models.nodes[at(&models, "vol/a/secret")].kind, NodeKind::Locked);
}
