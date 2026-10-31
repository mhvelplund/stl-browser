//! The Files view: the real disk and archive layout, archives as folders.
use stl_core::vtree::files::*;
use stl_core::vtree::{NodeId, NodeKind, Tree};

fn s(x: &str) -> String {
    x.to_string()
}

fn root() -> RootRow {
    RootRow { id: 1, name: s("Vol"), path: s("D:\\Vol") }
}

fn archive(id: i64, rel: &str) -> SourceRow {
    SourceRow { id, root_id: 1, kind: s("archive"), path: format!("D:\\Vol\\{}", rel.replace('/', "\\")), rel_path: format!("Vol/{rel}"), locked: false }
}

fn folder(id: i64, rel: &str) -> SourceRow {
    SourceRow { id, root_id: 1, kind: s("folder"), path: format!("D:\\Vol\\{rel}"), rel_path: format!("Vol/{rel}"), locked: false }
}

fn entry(id: i64, source_id: i64, path: &str, chain: &[&str]) -> EntryRow {
    EntryRow { id, source_id, path: s(path), chain: chain.iter().map(|c| s(c)).collect(), locked: false }
}

fn at(t: &Tree, path: &str) -> NodeId {
    t.find(path).unwrap_or_else(|| panic!("no node at {path}"))
}

fn names(t: &Tree, id: NodeId) -> Vec<String> {
    t.nodes[id].children.iter().map(|&c| t.nodes[c].name.clone()).collect()
}

#[test]
fn archive_is_folder_with_badge() {
    let t = build_files(&[root()], &[archive(1, "A/Kit.zip")], &[entry(1, 1, "Kit/a.stl", &["Kit/a.stl"])]);
    let a = at(&t, "vol/a");
    assert_eq!((t.nodes[a].kind, t.nodes[a].real.clone()), (NodeKind::Folder, vec![s("D:\\Vol\\A")]));
    let k = at(&t, "vol/a/kit");
    assert_eq!(t.nodes[k].kind, NodeKind::Archive);
    assert_eq!(t.nodes[k].info.badge.as_deref(), Some("zip"));
    assert_eq!(t.nodes[k].real, [s("D:\\Vol\\A\\Kit.zip")]);
    assert_eq!(t.nodes[k].key, "f:D:\\Vol\\A\\Kit.zip|");
    let f = at(&t, "vol/a/kit/kit/a.stl");
    assert_eq!((t.nodes[f].kind, t.nodes[f].info.entry_id), (NodeKind::File, Some(1)));
    assert_eq!(t.nodes[f].key, "f:D:\\Vol\\A\\Kit.zip|Kit/a.stl");
}

#[test]
fn nested_archive_expanded_and_its_file_dropped() {
    let t = build_files(
        &[root()],
        &[archive(1, "A/Kit.zip")],
        &[entry(1, 1, "in/y.zip", &["in/y.zip"]), entry(2, 1, "in/y.zip/z/b.stl", &["in/y.zip", "z/b.stl"])],
    );
    assert_eq!(names(&t, at(&t, "vol/a/kit/in")), ["y"]);
    let y = at(&t, "vol/a/kit/in/y");
    assert_eq!((t.nodes[y].kind, t.nodes[y].info.badge.as_deref()), (NodeKind::Archive, Some("zip")));
    assert_eq!(t.nodes[y].real, [s("D:\\Vol\\A\\Kit.zip!in/y.zip")]);
    assert_eq!(t.nodes[at(&t, "vol/a/kit/in/y/z")].real, [s("D:\\Vol\\A\\Kit.zip!in/y.zip!z")]);
}

#[test]
fn file_hover_path() {
    let t = build_files(&[root()], &[archive(1, "A/Kit.zip")], &[entry(2, 1, "in/y.zip/z/b.stl", &["in\\y.zip", "z/b.stl"])]);
    assert_eq!(t.nodes[at(&t, "vol/a/kit/in/y/z/b.stl")].real, [s("D:\\Vol\\A\\Kit.zip!in/y.zip!z/b.stl")]);
}

#[test]
fn unexpanded_nested_archive_stays_a_file() {
    let t = build_files(&[root()], &[archive(1, "A/Kit.zip")], &[entry(1, 1, "deep.zip", &["deep.zip"])]);
    let d = at(&t, "vol/a/kit/deep.zip");
    assert_eq!((t.nodes[d].kind, t.nodes[d].info.file_kind.as_deref()), (NodeKind::File, Some("archive")));
}

#[test]
fn locked_source_and_locked_nested_entry() {
    let mut locked_src = archive(2, "A/Secret.7z");
    locked_src.locked = true;
    let mut nested = entry(1, 1, "inner/locked.zip", &["inner/locked.zip"]);
    nested.locked = true;
    let t = build_files(&[root()], &[archive(1, "A/Kit.zip"), locked_src], &[nested]);
    let a = at(&t, "vol/a/secret");
    assert_eq!((t.nodes[a].kind, t.nodes[a].info.badge.as_deref()), (NodeKind::Locked, Some("7z")));
    assert!(t.nodes[a].children.is_empty());
    let b = at(&t, "vol/a/kit/inner/locked");
    assert_eq!(t.nodes[b].kind, NodeKind::Locked);
    assert!(t.nodes[b].children.is_empty());
}

#[test]
fn case_variant_folders_merge() {
    let t = build_files(
        &[root()],
        &[archive(1, "A/Kit.zip")],
        &[entry(1, 1, "Foo/a.stl", &["Foo/a.stl"]), entry(2, 1, "foo/b.stl", &["foo/b.stl"])],
    );
    assert_eq!(names(&t, at(&t, "vol/a/kit")), ["foo"]);
    let foo = at(&t, "vol/a/kit/foo");
    assert_eq!(t.nodes[foo].children.len(), 2);
    assert_eq!(t.nodes[foo].real, [s("D:\\Vol\\A\\Kit.zip!Foo"), s("D:\\Vol\\A\\Kit.zip!foo")]);
}

#[test]
fn same_stem_archives_get_postfix() {
    let t = build_files(
        &[root()],
        &[archive(1, "A/X.zip"), archive(2, "A/x.rar"), folder(3, "A")],
        &[entry(1, 1, "a.stl", &["a.stl"]), entry(2, 2, "b.stl", &["b.stl"]), entry(3, 3, "x/c.stl", &["x/c.stl"])],
    );
    assert_eq!(names(&t, at(&t, "vol/a")), ["x_1", "x_2", "x_3"]);
    assert_eq!(t.nodes[at(&t, "vol/a/x_1")].real, [s("D:\\Vol\\A\\X.zip")]);
    assert_eq!(t.nodes[at(&t, "vol/a/x_2")].kind, NodeKind::Folder);
    assert_eq!(t.nodes[at(&t, "vol/a/x_3")].real, [s("D:\\Vol\\A\\x.rar")]);
}

#[test]
fn loose_folder_source() {
    let t = build_files(&[root()], &[folder(1, "A")], &[entry(1, 1, "sub/a.stl", &["sub/a.stl"])]);
    let f = at(&t, "vol/a/sub/a.stl");
    assert_eq!(t.nodes[f].real, [s("D:\\Vol\\A\\sub\\a.stl")]);
    assert_eq!(t.nodes[at(&t, "vol/a/sub")].real, [s("D:\\Vol\\A\\sub")]);
}

#[test]
fn label_keeps_case() {
    let t = build_files(&[root()], &[folder(1, "Orc Band")], &[entry(1, 1, "Orc.STL", &["Orc.STL"])]);
    let band = at(&t, "vol/orc band");
    assert_eq!((t.nodes[band].label.as_str(), t.nodes[band].name.as_str()), ("Orc Band", "orc band"));
    assert_eq!(t.nodes[at(&t, "vol/orc band/orc.stl")].label, "Orc.STL");
}
