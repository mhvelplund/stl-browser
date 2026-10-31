//! The shown-endings filter: which files the views list.
use stl_core::vtree::filter::Shown;
use stl_core::vtree::{NodeKind, Tree};

fn shows(s: &Shown, name: &str, kind: NodeKind) -> bool {
    let mut t = Tree::new();
    let id = t.add(0, name, kind);
    s.shows(&t.nodes[id])
}

#[test]
fn folders_archives_and_locked_items_are_always_shown() {
    let s = Shown::new(["stl"]);
    for kind in [NodeKind::Folder, NodeKind::Archive, NodeKind::Model, NodeKind::Docs, NodeKind::Locked] {
        assert!(shows(&s, "kit.exe", kind), "{kind:?}");
    }
}

/// An inner archive that wasn't expanded (too deep, unreadable) is a file node named `x.zip`.
#[test]
fn unexpanded_archives_are_always_shown() {
    let s = Shown::new(["stl"]);
    for name in ["kit.zip", "Kit.RAR", "kit.7z"] {
        assert!(shows(&s, name, NodeKind::File), "{name}");
    }
}

#[test]
fn files_are_shown_by_their_ending() {
    let s = Shown::new(["stl", "jpg"]);
    assert!(shows(&s, "orc.stl", NodeKind::File));
    assert!(shows(&s, "Orc.STL", NodeKind::File));
    assert!(shows(&s, "orc.jpg", NodeKind::Image));
    assert!(!shows(&s, "setup.exe", NodeKind::File));
    assert!(!shows(&s, "readme", NodeKind::File));
    assert!(!shows(&s, "a.tar.gz", NodeKind::File));
    assert!(shows(&Shown::new(["gz"]), "a.tar.gz", NodeKind::File));
}

#[test]
fn config_endings_are_normalized() {
    let s = Shown::new([".STL", " Obj "]);
    assert!(shows(&s, "orc.stl", NodeKind::File));
    assert!(shows(&s, "orc.obj", NodeKind::File));
}

#[test]
fn defaults_show_print_files_images_and_documents() {
    let s = Shown::defaults();
    for name in ["a.stl", "a.obj", "a.3mf", "a.lys", "a.chitubox", "a.png", "a.jpeg", "a.pdf", "a.txt"] {
        assert!(shows(&s, name, NodeKind::File), "{name}");
    }
    for name in ["a.exe", "a.hmp", "a.qml", "a.dll", "a.mtl", "a.pyc"] {
        assert!(!shows(&s, name, NodeKind::File), "{name}");
    }
}
