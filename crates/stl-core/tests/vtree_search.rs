//! Name search over a view tree: the three modes, case, char ranges and the walk.
use stl_core::vtree::search::*;
use stl_core::vtree::{NodeId, NodeKind, Tree};

fn f(q: &str, mode: Mode, cs: bool, text: &str) -> Option<Vec<(usize, usize)>> {
    Matcher::new(q, mode, cs).unwrap().find(text)
}

fn segs(p: &[&str]) -> Vec<String> {
    p.iter().map(|s| s.to_string()).collect()
}

#[test]
fn word_is_a_substring() {
    assert_eq!(f("orc", Mode::Word, false, "Big Orc Boss"), Some(vec![(4, 7)]));
    assert_eq!(f("ob", Mode::Word, false, "orc boss"), None);
}

#[test]
fn free_is_in_order() {
    assert_eq!(f("obo", Mode::Free, false, "orc boss"), Some(vec![(0, 1), (4, 6)]));
    assert_eq!(f("so", Mode::Free, false, "orc boss"), None);
}

#[test]
fn case_toggle() {
    for mode in [Mode::Word, Mode::Free, Mode::Regexp] {
        assert_eq!(f("Orc", mode, true, "orc"), None, "{mode:?}");
        assert!(f("Orc", mode, false, "orc").is_some(), "{mode:?}");
    }
}

#[test]
fn regexp_matches_and_rejects() {
    assert_eq!(f(r"^\d+ ", Mode::Regexp, false, "565 mira"), Some(vec![(0, 4)]));
    assert!(Matcher::new("(", Mode::Regexp, false).is_err());
}

#[test]
fn empty_regexp_match_has_no_marks() {
    assert_eq!(f("a?", Mode::Regexp, false, "xyz"), Some(vec![]));
}

#[test]
fn chars_not_bytes() {
    assert_eq!(f("fé", Mode::Word, false, "CAFÉ"), Some(vec![(2, 4)]));
    assert_eq!(f("élf", Mode::Free, false, "ÉLF x"), Some(vec![(0, 3)]));
    assert_eq!(f("é", Mode::Regexp, false, "café"), Some(vec![(3, 4)]));
}

/// `vol/coll/{orc/{orc.stl, notes.txt}, elf/elf.stl}`; returns (tree, coll, orc).
fn small() -> (Tree, NodeId, NodeId) {
    let mut t = Tree::new();
    let vol = t.add(0, "vol", NodeKind::Volume);
    let coll = t.add(vol, "coll", NodeKind::Collection);
    let orc = t.add(coll, "orc", NodeKind::Folder);
    t.add(orc, "orc.stl", NodeKind::File);
    t.add(orc, "notes.txt", NodeKind::File);
    let elf = t.add(coll, "elf", NodeKind::Folder);
    t.add(elf, "elf.stl", NodeKind::File);
    (t, coll, orc)
}

#[test]
fn walk_scope_order_keep_and_limit() {
    let (t, coll, orc) = small();
    let m = Matcher::new("o", Mode::Word, false).unwrap();
    let scope = segs(&["vol", "coll"]);
    let all = |_: &stl_core::vtree::Node| true;
    let (hits, truncated) = search(&t, coll, &scope, &m, Field::Name, &all, 100);
    let names: Vec<&str> = hits.iter().map(|h| t.nodes[h.id].name.as_str()).collect();
    assert_eq!(names, ["orc", "orc.stl", "notes.txt"]);
    assert!(!truncated);
    assert_eq!(hits[1].parent, segs(&["vol", "coll", "orc"]));
    assert_eq!(hits[0].parent, scope);
    let no_orc = |n: &stl_core::vtree::Node| n.name != "orc";
    assert!(search(&t, coll, &scope, &m, Field::Name, &no_orc, 100).0.is_empty());
    let (one, truncated) = search(&t, coll, &scope, &m, Field::Name, &all, 1);
    assert_eq!((one.len(), truncated), (1, true));
    assert_eq!(one[0].id, orc);
}

#[test]
fn cap_with_match_all_regexp() {
    let mut t = Tree::new();
    let dir = t.add(0, "dir", NodeKind::Folder);
    for i in 0..1500 {
        t.add(dir, &format!("f{i}.stl"), NodeKind::File);
    }
    let m = Matcher::new(".*", Mode::Regexp, false).unwrap();
    let (hits, truncated) = search(&t, dir, &segs(&["dir"]), &m, Field::Name, &|_| true, 1000);
    assert_eq!((hits.len(), truncated), (1000, true));
}

#[test]
fn label_field_marks_map_to_name() {
    let mut t = Tree::new();
    t.add(0, "Orc Band", NodeKind::Folder);
    let cs = |q: &str| Matcher::new(q, Mode::Word, true).unwrap();
    let (hits, _) = search(&t, 0, &[], &cs("Band"), Field::Label, &|_| true, 10);
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].marks, vec![(4, 8)]);
    assert!(search(&t, 0, &[], &cs("band"), Field::Label, &|_| true, 10).0.is_empty());
}
