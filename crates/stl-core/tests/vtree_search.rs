//! Name search over a view tree: the three modes, case, char ranges and the walk.
use stl_core::vtree::search::*;
use stl_core::vtree::tags::{Tag, TagKind};
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
    let (hits, truncated) = search(&t, coll, &scope, Some(&m), Field::Name, &all, &none(), 100);
    let names: Vec<&str> = hits.iter().map(|h| t.nodes[h.id].name.as_str()).collect();
    assert_eq!(names, ["orc", "orc.stl", "notes.txt"]);
    assert!(!truncated);
    assert_eq!(hits[1].parent, segs(&["vol", "coll", "orc"]));
    assert_eq!(hits[0].parent, scope);
    let no_orc = |n: &stl_core::vtree::Node| n.name != "orc";
    assert!(search(&t, coll, &scope, Some(&m), Field::Name, &no_orc, &none(), 100).0.is_empty());
    let (one, truncated) = search(&t, coll, &scope, Some(&m), Field::Name, &all, &none(), 1);
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
    let (hits, truncated) = search(&t, dir, &segs(&["dir"]), Some(&m), Field::Name, &|_| true, &none(), 1000);
    assert_eq!((hits.len(), truncated), (1000, true));
}

#[test]
fn label_field_marks_map_to_name() {
    let mut t = Tree::new();
    t.add(0, "Orc Band", NodeKind::Folder);
    let cs = |q: &str| Matcher::new(q, Mode::Word, true).unwrap();
    let (hits, _) = search(&t, 0, &[], Some(&cs("Band")), Field::Label, &|_| true, &none(), 10);
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].marks, vec![(4, 8)]);
    assert!(search(&t, 0, &[], Some(&cs("band")), Field::Label, &|_| true, &none(), 10).0.is_empty());
}

fn none() -> TagFilter<'static> {
    TagFilter { terms: &[], own: &|_| Vec::new(), start: Inherited::default() }
}

fn tag(kind: TagKind, name: &str, by_user: bool) -> Tag {
    Tag { kind, name: name.to_string(), by_user }
}

fn terms(v: &[&str]) -> Vec<String> {
    segs(v)
}

struct Fixture {
    t: Tree,
    legion: NodeId,
    warrior: NodeId,
    arm: NodeId,
    other_arm: NodeId,
}

/// `vol/{legion/{warrior/{arm.stl, leg.stl}}, other/arm.stl}`; `legion` is tagged `tinmen`.
fn tagged() -> Fixture {
    let mut t = Tree::new();
    let vol = t.add(0, "vol", NodeKind::Volume);
    let legion = t.add(vol, "legion", NodeKind::Folder);
    let warrior = t.add(legion, "warrior", NodeKind::Folder);
    let arm = t.add(warrior, "arm.stl", NodeKind::File);
    t.add(warrior, "leg.stl", NodeKind::File);
    let other = t.add(vol, "other", NodeKind::Folder);
    let other_arm = t.add(other, "arm.stl", NodeKind::File);
    t.nodes[legion].info.tags = vec![tag(TagKind::User, "tinmen", true)];
    Fixture { t, legion, warrior, arm, other_arm }
}

fn own_of(t: &Tree) -> impl Fn(NodeId) -> Vec<Tag> + '_ {
    move |id| t.nodes[id].info.tags.clone()
}

fn ids(hits: &[Hit]) -> Vec<NodeId> {
    hits.iter().map(|h| h.id).collect()
}

#[test]
fn tag_only_search_stops_at_topmost() {
    let fx = tagged();
    let own = own_of(&fx.t);
    let want = terms(&["tinmen"]);
    let filter = TagFilter { terms: &want, own: &own, start: Inherited::default() };
    let (hits, truncated) = search(&fx.t, 0, &[], None, Field::Name, &|_| true, &filter, 100);
    assert_eq!(ids(&hits), [fx.legion]);
    assert!(!truncated);
    assert_eq!(hits[0].parent, segs(&["vol"]));
}

#[test]
fn tag_and_name_search_finds_inside() {
    let fx = tagged();
    let own = own_of(&fx.t);
    let want = terms(&["tinmen"]);
    let filter = TagFilter { terms: &want, own: &own, start: Inherited::default() };
    let m = Matcher::new("arm", Mode::Word, false).unwrap();
    let (hits, _) = search(&fx.t, 0, &[], Some(&m), Field::Name, &|_| true, &filter, 100);
    assert_eq!(ids(&hits), [fx.arm]);
    assert_ne!(fx.arm, fx.other_arm);
}

#[test]
fn scope_inside_tagged_folder_lists_children() {
    let fx = tagged();
    let own = own_of(&fx.t);
    let want = terms(&["tinmen"]);
    let scope = segs(&["vol", "legion"]);
    let start = Inherited::along(&fx.t, &scope, &own);
    let filter = TagFilter { terms: &want, own: &own, start };
    let (hits, _) = search(&fx.t, fx.legion, &scope, None, Field::Name, &|_| true, &filter, 100);
    assert_eq!(ids(&hits), [fx.warrior]);
}

#[test]
fn user_override_masks_generated_below() {
    let mut fx = tagged();
    fx.t.nodes[fx.legion].info.tags.push(tag(TagKind::Scale, "32mm", true));
    fx.t.nodes[fx.arm].info.tags = vec![tag(TagKind::Scale, "25mm", false)];
    let own = own_of(&fx.t);
    let m = Matcher::new("arm", Mode::Word, false).unwrap();
    let run = |q: &[&str]| {
        let want = terms(q);
        let filter = TagFilter { terms: &want, own: &own, start: Inherited::default() };
        ids(&search(&fx.t, 0, &[], Some(&m), Field::Name, &|_| true, &filter, 100).0)
    };
    assert!(run(&["25mm"]).is_empty());
    assert_eq!(run(&["32mm"]), [fx.arm]);
}

#[test]
fn all_terms_required() {
    let fx = tagged();
    let own = own_of(&fx.t);
    let want = terms(&["tinmen", "supported"]);
    let filter = TagFilter { terms: &want, own: &own, start: Inherited::default() };
    assert!(search(&fx.t, 0, &[], None, Field::Name, &|_| true, &filter, 100).0.is_empty());
    let m = Matcher::new("arm", Mode::Word, false).unwrap();
    assert!(search(&fx.t, 0, &[], Some(&m), Field::Name, &|_| true, &filter, 100).0.is_empty());
}

#[test]
fn no_matcher_and_no_terms_gives_nothing() {
    let fx = tagged();
    assert!(search(&fx.t, 0, &[], None, Field::Name, &|_| true, &none(), 100).0.is_empty());
}

#[test]
fn own_is_never_read_without_tag_terms() {
    let fx = tagged();
    let calls = std::cell::Cell::new(0);
    let own = |id: NodeId| {
        calls.set(calls.get() + 1);
        fx.t.nodes[id].info.tags.clone()
    };
    let filter = TagFilter { terms: &[], own: &own, start: Inherited::default() };
    let m = Matcher::new("arm", Mode::Word, false).unwrap();
    let (hits, _) = search(&fx.t, 0, &[], Some(&m), Field::Name, &|_| true, &filter, 100);
    assert_eq!(ids(&hits), [fx.arm, fx.other_arm]);
    assert_eq!(calls.get(), 0);
}
