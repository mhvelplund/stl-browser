//! Export plans: the folders and files a drag or Extract to… writes, mirroring a view's tree.
use stl_core::vtree::export::*;
use stl_core::vtree::{Node, NodeId, NodeKind, Tree};

fn file(t: &mut Tree, parent: NodeId, name: &str, kind: NodeKind, entry: i64) -> NodeId {
    let id = t.add(parent, name, kind);
    t.nodes[id].info.entry_id = Some(entry);
    id
}

fn rels(p: &ExportPlan) -> Vec<&str> {
    p.items.iter().map(|i| i.rel.as_str()).collect()
}

fn all(_: &Node) -> bool {
    true
}

/// `orc/{images/orc.jpg, orc.stl, docs/}`
fn orc() -> (Tree, NodeId) {
    let mut t = Tree::new();
    let orc = t.add(0, "orc", NodeKind::Model);
    let images = t.add(orc, "images", NodeKind::Images);
    file(&mut t, images, "orc.jpg", NodeKind::Image, 1);
    file(&mut t, orc, "orc.stl", NodeKind::File, 2);
    t.add(orc, "docs", NodeKind::Docs);
    (t, orc)
}

#[test]
fn plan_mirrors_tree() {
    let (t, orc) = orc();
    let p = export_plan(&t, &[orc], &all);
    assert_eq!(rels(&p), ["orc", "orc/images", "orc/images/orc.jpg", "orc/orc.stl", "orc/docs"]);
    let kinds: Vec<_> = p.items.iter().map(|i| i.kind.clone()).collect();
    assert_eq!(
        kinds,
        [
            ExportKind::Dir,
            ExportKind::Dir,
            ExportKind::File { entry_id: 1 },
            ExportKind::File { entry_id: 2 },
            ExportKind::Dir
        ]
    );
    assert_eq!(p.skipped, 0);
}

#[test]
fn plan_of_a_file_root() {
    let mut t = Tree::new();
    let elf = file(&mut t, 0, "elf.stl", NodeKind::File, 7);
    let p = export_plan(&t, &[elf], &all);
    assert_eq!(rels(&p), ["elf.stl"]);
    assert_eq!(p.items[0].kind, ExportKind::File { entry_id: 7 });
}

#[test]
fn plan_skips_locked_and_filtered() {
    let (mut t, orc) = orc();
    file(&mut t, orc, "secret.zip", NodeKind::Locked, 9);
    file(&mut t, orc, "notes.txt", NodeKind::File, 10);
    let p = export_plan(&t, &[orc], &|n: &Node| n.name != "notes.txt");
    assert!(!rels(&p).iter().any(|r| r.contains("secret") || r.contains("notes")), "{:?}", rels(&p));
    assert_eq!(p.skipped, 1);
}

#[test]
fn plan_two_roots() {
    let mut t = Tree::new();
    let a = t.add(0, "a", NodeKind::Folder);
    file(&mut t, a, "x.stl", NodeKind::File, 1);
    let b = t.add(0, "b", NodeKind::Folder);
    file(&mut t, b, "y.stl", NodeKind::File, 2);
    let p = export_plan(&t, &[a, b], &all);
    assert_eq!(rels(&p), ["a", "a/x.stl", "b", "b/y.stl"]);
}

#[test]
fn safe_name_replaces_reserved_chars() {
    assert_eq!(safe_name("a:b?.stl"), "a_b_.stl");
    assert_eq!(safe_name("x/y"), "x_y");
    assert_eq!(safe_name("tab\there"), "tab_here");
    assert_eq!(safe_name("<a|b>\"c\\*"), "_a_b__c__");
}

#[test]
fn safe_name_trims_and_guards_devices() {
    assert_eq!(safe_name("name. "), "name");
    assert_eq!(safe_name("..."), "_");
    assert_eq!(safe_name(""), "_");
    assert_eq!(safe_name("con.stl"), "_con.stl");
    assert_eq!(safe_name("COM1"), "_COM1");
    assert_eq!(safe_name("lpt9.x.y"), "_lpt9.x.y");
    assert_eq!(safe_name("console.stl"), "console.stl");
    assert_eq!(safe_name("com10"), "com10");
}

#[test]
fn plan_renames_sanitized_clashes() {
    let mut t = Tree::new();
    let d = t.add(0, "d", NodeKind::Folder);
    file(&mut t, d, "a?.stl", NodeKind::File, 1);
    file(&mut t, d, "a_.stl", NodeKind::File, 2);
    let p = export_plan(&t, &[d], &all);
    assert_eq!(rels(&p), ["d", "d/a_.stl", "d/a__2.stl"]);
}

#[test]
fn plan_follows_shared_children() {
    // inner-root volumes share children: the same subtree under two parents is written twice
    let mut t = Tree::new();
    let a = t.add(0, "a", NodeKind::Volume);
    let s = t.add(a, "s", NodeKind::Folder);
    file(&mut t, s, "x.stl", NodeKind::File, 1);
    let b = t.add(0, "b", NodeKind::Volume);
    t.nodes[b].children.push(s);
    let p = export_plan(&t, &[a, b], &all);
    assert_eq!(rels(&p), ["a", "a/s", "a/s/x.stl", "b", "b/s", "b/s/x.stl"]);
}
