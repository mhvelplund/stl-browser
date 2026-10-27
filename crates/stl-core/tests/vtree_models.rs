//! Port of `prototype/tests/test_vtree.py` (names now lowercase), plus the naming and hover rules.
use stl_core::vtree::models::*;
use stl_core::vtree::{NodeId, NodeKind, Tree};

const COLL: &str = "Sci-Fi Collection Vol.2/06. Robots STC";

fn s(x: &str) -> String {
    x.to_string()
}

fn file(model_id: i64, entry_id: i64, name: &str, kind: &str, part_group: &str, variant: &str) -> FileRow {
    FileRow {
        model_id,
        entry_id,
        name: s(name),
        kind: s(kind),
        part_group: s(part_group),
        variant: s(variant),
        real: format!("D:\\V\\x.rar!f{entry_id}"),
        key: format!("f:D:\\V\\x.rar|f{entry_id}"),
    }
}

fn asset(id: i64, model_id: i64, entry_id: i64, name: &str, kind: &str, role: &str) -> AssetRow {
    AssetRow {
        id,
        model_id,
        entry_id,
        name: s(name),
        kind: s(kind),
        role: s(role),
        real: format!("D:\\V\\x.rar!f{entry_id}"),
        key: format!("f:D:\\V\\x.rar|f{entry_id}"),
    }
}

fn model(id: i64, group_id: Option<i64>, name: &str, raw: &str, path: &str, edition: Option<&str>, hero: Option<i64>) -> ModelRow {
    ModelRow {
        id,
        collection: s(COLL),
        group_id,
        name: s(name),
        raw_name: s(raw),
        path: s(path),
        edition: edition.map(s),
        hero_asset_id: hero,
        file_count: 0,
        image_count: 0,
        variants: Vec::new(),
    }
}

fn group(id: i64, parent_id: Option<i64>, name: &str, path: &str, poster: Option<i64>, posters: u32) -> GroupRow {
    GroupRow { id, collection: s(COLL), parent_id, name: s(name), path: s(path), poster_asset_id: poster, posters }
}

fn input() -> ModelsInput {
    ModelsInput {
        collections: vec![CollectionRow {
            volume: s("Sci-Fi Collection Vol.2"),
            collection: s(COLL),
            real: vec![s("D:\\V\\06. Robots STC")],
            root_real: s("D:\\V"),
        }],
        // child group before its parent in id order
        groups: vec![
            group(1, None, "06. Robots STC", "", None, 0),
            group(2, Some(3), "Sub", "Elites/Sub", None, 0),
            group(3, Some(1), "Elites", "Elites", Some(9), 1),
        ],
        models: vec![
            model(1, Some(3), "Archivist Zarvos", "Archivist Zarvos (5E)", "Elites/a", Some("5E"), Some(1)),
            model(2, Some(3), "Archivist Zarvos", "Archivist Zarvos (8E)", "Elites/b", Some("8E"), Some(9)),
            model(3, Some(2), "Spinecrawlers", "Spinecrawlers", "Elites/Sub/c", None, None),
        ],
        files: vec![
            file(1, 1, "body.stl", "model", "", ""),
            file(1, 2, "README.pdf", "doc", "", ""),
            file(3, 3, "head.stl", "model", "", ""),
            file(3, 4, "head.stl", "model", "Kit/Heads", "supported"),
            file(3, 5, "head.lys", "scene", "Kit/Heads", "supported+lychee"),
        ],
        assets: vec![
            asset(2, 1, 11, "a.jpg", "image", "preview"),
            asset(1, 1, 10, "b.jpg", "image", "preview"),
            asset(3, 1, 2, "README.pdf", "doc", "doc"),
        ],
        locked: Vec::new(),
    }
}

fn names(t: &Tree, id: NodeId) -> Vec<String> {
    t.nodes[id].children.iter().map(|&c| t.nodes[c].name.clone()).collect()
}

fn at(t: &Tree, path: &str) -> NodeId {
    t.find(path).unwrap_or_else(|| panic!("no node at {path}"))
}

#[test]
fn volume_and_collection_levels() {
    let t = build_models(&input(), true);
    let coll = at(&t, COLL);
    assert_eq!(t.nodes[coll].kind, NodeKind::Collection);
    assert_eq!(names(&t, coll), ["elites"]); // root group is not repeated
}

#[test]
fn child_group_listed_before_parent_is_attached_once() {
    let t = build_models(&input(), true);
    let elites = at(&t, &format!("{COLL}/Elites"));
    let groups: Vec<String> =
        t.nodes[elites].children.iter().filter(|&&c| t.nodes[c].kind == NodeKind::Group).map(|&c| t.nodes[c].name.clone()).collect();
    assert_eq!(groups, ["sub"]);
    assert_eq!(t.nodes[elites].info.posters, 1);
}

#[test]
fn name_clash_uses_edition() {
    let t = build_models(&input(), true);
    let elites = at(&t, &format!("{COLL}/Elites"));
    let models: Vec<NodeId> = t.nodes[elites].children.iter().copied().filter(|&c| t.nodes[c].kind == NodeKind::Model).collect();
    assert_eq!(models.iter().map(|&m| t.nodes[m].name.as_str()).collect::<Vec<_>>(), ["archivist zarvos (5e)", "archivist zarvos (8e)"]);
    assert_eq!(t.nodes[models[1]].info.hero_asset_id, Some(9)); // fallback poster
}

#[test]
fn single_variant_model_has_images_docs_and_flat_files() {
    let t = build_models(&input(), true);
    let m = at(&t, &format!("{COLL}/Elites/Archivist Zarvos (5E)"));
    let kids: Vec<(NodeKind, String)> = t.nodes[m].children.iter().map(|&c| (t.nodes[c].kind, t.nodes[c].name.clone())).collect();
    assert_eq!(kids, [(NodeKind::Images, s("images")), (NodeKind::Docs, s("docs")), (NodeKind::File, s("body.stl"))]);
    assert_eq!(names(&t, t.nodes[m].children[0]), ["a.jpg", "b.jpg"]); // rank order
    assert_eq!(t.nodes[t.nodes[m].children[1]].children.len(), 1); // doc in both tables shown once
}

#[test]
fn variants_then_nested_part_groups() {
    let t = build_models(&input(), true);
    let m = at(&t, &format!("{COLL}/Elites/Sub/Spinecrawlers"));
    assert_eq!(names(&t, m), ["default", "supported", "supported · lychee"]);
    let heads = at(&t, &format!("{COLL}/Elites/Sub/Spinecrawlers/supported/kit/heads"));
    let leaves: Vec<(String, Option<i64>)> =
        t.nodes[heads].children.iter().map(|&c| (t.nodes[c].name.clone(), t.nodes[c].info.entry_id)).collect();
    assert_eq!(leaves, [(s("head.stl"), Some(4))]);
}

#[test]
fn files_false_stops_at_models() {
    let t = build_models(&input(), false);
    assert!(t.nodes[at(&t, &format!("{COLL}/Elites/Sub/Spinecrawlers"))].children.is_empty());
}

#[test]
fn model_without_group_sits_in_collection() {
    let mut i = input();
    i.models.push(model(4, None, "Solo", "Solo", "", None, None));
    let t = build_models(&i, true);
    assert_eq!(t.nodes[at(&t, &format!("{COLL}/solo"))].kind, NodeKind::Model);
}

#[test]
fn coinciding_collections_merge() {
    let mut i = input();
    let mut twin = i.collections[0].clone();
    twin.real = vec![s("d:\\v\\06. robots stc")];
    i.collections.push(twin);
    let t = build_models(&i, true);
    let vol = at(&t, "sci-fi collection vol.2");
    assert_eq!(names(&t, vol), ["06. robots stc"]);
    assert_eq!(t.nodes[at(&t, COLL)].real, [s("D:\\V\\06. Robots STC"), s("d:\\v\\06. robots stc")]);
}

#[test]
fn locked_archive_in_collection() {
    let mut i = input();
    i.locked.push(LockedRow { collection: s(COLL), name: s("Secret"), real: s("D:\\V\\06. Robots STC\\Secret.zip"), key: s("f:x|") });
    let t = build_models(&i, true);
    let n = at(&t, &format!("{COLL}/secret"));
    assert_eq!(t.nodes[n].kind, NodeKind::Locked);
}

#[test]
fn model_hover_is_common_folder() {
    let mut i = input();
    i.files = vec![
        FileRow { real: s("D:\\V\\A\\k.zip!K/a.stl"), ..file(3, 3, "a.stl", "model", "", "") },
        FileRow { real: s("D:\\V\\A\\k.zip!K/sup/b.stl"), ..file(3, 4, "b.stl", "model", "", "supported") },
    ];
    let t = build_models(&i, false);
    assert_eq!(t.nodes[at(&t, &format!("{COLL}/Elites/Sub/Spinecrawlers"))].real, [s("D:\\V\\A\\k.zip!K")]);
}
