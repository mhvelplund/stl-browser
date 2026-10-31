use std::collections::HashMap;

use stl_core::vtree::tags::*;
use stl_core::vtree::{NodeKind, Tree};

fn tag(kind: TagKind, name: &str, by_user: bool) -> Tag {
    Tag { kind, name: name.into(), by_user }
}

fn node_with(real: &[&str], generated: Vec<Tag>) -> stl_core::vtree::Node {
    let mut t = Tree::new();
    let id = t.add(0, "Orc", NodeKind::Model);
    t.nodes[id].real = real.iter().map(|s| s.to_string()).collect();
    t.nodes[id].info.tags = generated;
    t.nodes[id].clone()
}

#[test]
fn normalize_trims_lowercases_and_collapses() {
    assert_eq!(normalize("  Robot   Legion "), Some("robot legion".to_string()));
    assert_eq!(normalize("   "), None);
}

#[test]
fn tag_key_normalises_separators() {
    let want = r"r:d:\m\pack.zip!legion\arms";
    assert_eq!(tag_key(r"D:\M\Pack.zip!Legion/Arms/"), want);
    assert_eq!(tag_key(r"d:\m\pack.zip!legion\arms"), want);
}

#[test]
fn variant_atoms() {
    assert_eq!(
        variant_tags("supported+25mm"),
        vec![tag(TagKind::Support, "supported", false), tag(TagKind::Scale, "25mm", false)]
    );
    assert_eq!(variant_tags("lychee"), vec![tag(TagKind::Support, "lychee", false)]);
    assert!(variant_tags("").is_empty());
    assert!(variant_tags("finished").is_empty());
}

#[test]
fn exclusive_drops_conflicts() {
    let s = |n: &str| tag(TagKind::Support, n, false);
    let sc = |n: &str| tag(TagKind::Scale, n, false);
    assert_eq!(exclusive(vec![s("supported"), s("unsupported"), s("lychee")]), vec![s("lychee")]);
    assert_eq!(exclusive(vec![sc("25mm"), sc("32mm")]), vec![]);
    assert_eq!(exclusive(vec![sc("25mm"), sc("25mm")]), vec![sc("25mm")]);
}

#[test]
fn parse_query_basic() {
    let p = parse_query(r#"warrior tag:necrons  tag:"Robot  Legion" arm"#);
    assert_eq!(p.text, "warrior arm");
    assert_eq!(p.tags, vec!["necrons", "robot legion"]);
}

#[test]
fn parse_query_edge_cases() {
    let p = parse_query("tag:");
    assert!(p.tags.is_empty());
    assert_eq!(p.text, "");
    assert_eq!(parse_query(r#"tag:"robot legion"#).tags, vec!["robot legion"]);
    assert_eq!(parse_query("TAG:Necrons").tags, vec!["necrons"]);
    let p = parse_query("stag:x");
    assert!(p.tags.is_empty());
    assert_eq!(p.text, "stag:x");
}

#[test]
fn override_and_restore() {
    let supported = tag(TagKind::Support, "supported", false);
    let scale = tag(TagKind::Scale, "25mm", false);
    let n = node_with(&[r"D:\M\Orc"], vec![supported.clone(), scale.clone()]);
    let mut u = UserTags::default();
    assert_eq!(u.effective(&n), vec![supported.clone(), scale]);

    let user_scale = tag(TagKind::Scale, "32mm", true);
    u.set.insert(tag_key(r"D:\M\Orc"), vec![user_scale.clone()]);
    assert_eq!(u.effective(&n), vec![supported.clone(), user_scale.clone()]);

    u.hidden.insert(tag_key(r"D:\M\Orc"), vec![(TagKind::Support, "supported".into())]);
    assert_eq!(u.effective(&n), vec![user_scale]);
    assert_eq!(u.hidden_of(&n), vec![supported]);

    let u = UserTags::default();
    assert_eq!(u.effective(&n).len(), 2);
}

#[test]
fn merged_node_reads_the_union() {
    let n = node_with(&[r"D:\M\A", r"D:\M\B"], vec![]);
    let mut set = HashMap::new();
    set.insert(tag_key(r"D:\M\A"), vec![tag(TagKind::User, "alpha", true)]);
    set.insert(tag_key(r"D:\M\B"), vec![tag(TagKind::User, "beta", true)]);
    let u = UserTags { set, hidden: HashMap::new() };
    assert_eq!(u.effective(&n), vec![tag(TagKind::User, "alpha", true), tag(TagKind::User, "beta", true)]);
}

fn tree_with(real: &str, key: &str, kind: NodeKind, tags: Vec<Tag>) -> Tree {
    let mut t = Tree::new();
    let id = t.add(0, "x", kind);
    t.nodes[id].real = vec![real.to_string()];
    t.nodes[id].key = key.to_string();
    t.nodes[id].info.tags = tags;
    t
}

#[test]
fn copy_tags_by_real_path_and_key() {
    let sup = vec![tag(TagKind::Support, "supported", false)];
    let models = tree_with(r"D:\M\Orc", "m:c/orc", NodeKind::Model, sup.clone());
    let mut files = tree_with(r"d:\m\orc\", "f:folder", NodeKind::Folder, vec![]);
    copy_tags(&models, &mut files);
    assert_eq!(files.nodes[1].info.tags, sup);

    let leaf = vec![tag(TagKind::Scale, "25mm", false)];
    let models = tree_with(r"D:.zip!x.stl", "f:k1", NodeKind::File, leaf.clone());
    let mut files = tree_with(r"D:\other.zip!y.stl", "f:k1", NodeKind::File, vec![]);
    copy_tags(&models, &mut files);
    assert_eq!(files.nodes[1].info.tags, leaf);
}

#[test]
fn copy_tags_takes_the_common_tags() {
    let mut models = Tree::new();
    for (name, tags) in [
        ("a", vec![tag(TagKind::Support, "supported", false), tag(TagKind::Scale, "25mm", false)]),
        ("b", vec![tag(TagKind::Scale, "25mm", false)]),
    ] {
        let id = models.add(0, name, NodeKind::Model);
        models.nodes[id].real = vec![r"D:\M\Orc".to_string()];
        models.nodes[id].info.tags = tags;
    }
    let mut files = tree_with(r"d:\m\orc", "f:folder", NodeKind::Folder, vec![]);
    copy_tags(&models, &mut files);
    assert_eq!(files.nodes[1].info.tags, vec![tag(TagKind::Scale, "25mm", false)]);
}

fn add_at(t: &mut Tree, name: &str, kind: NodeKind, tags: Vec<Tag>) {
    let id = t.add(0, name, kind);
    t.nodes[id].real = vec!["D:/M/Orc".to_string()];
    t.nodes[id].info.tags = tags;
}

#[test]
fn copy_tags_ignores_untagged_structural_nodes() {
    let sup = vec![tag(TagKind::Support, "supported", false)];
    let mut models = Tree::new();
    add_at(&mut models, "orc", NodeKind::Model, sup.clone());
    add_at(&mut models, "arms", NodeKind::Part, vec![]);
    add_at(&mut models, "images", NodeKind::Images, vec![]);
    add_at(&mut models, "docs", NodeKind::Docs, vec![]);
    add_at(&mut models, "grp", NodeKind::Group, vec![]);
    let mut files = tree_with(r"d:\m\orc", "f:folder", NodeKind::Folder, vec![]);
    copy_tags(&models, &mut files);
    assert_eq!(files.nodes[1].info.tags, sup);
}

#[test]
fn copy_tags_untagged_sibling_model_empties_it() {
    let mut models = Tree::new();
    add_at(&mut models, "a", NodeKind::Model, vec![tag(TagKind::Support, "supported", false)]);
    add_at(&mut models, "b", NodeKind::Model, vec![]);
    let mut files = tree_with(r"d:\m\orc", "f:folder", NodeKind::Folder, vec![]);
    copy_tags(&models, &mut files);
    assert!(files.nodes[1].info.tags.is_empty());
}
