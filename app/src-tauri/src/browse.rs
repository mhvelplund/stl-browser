//! Folder listings for the UI, served from the cached view trees, with each item's carousel:
//! an optional composite (slot 0) and the pictures nested in it.
use std::collections::{HashMap, HashSet};

use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use stl_core::vtree::{Counts, Node, NodeId, NodeKind, Tree, counts, is_file_kind};
use stl_thumb::{ThumbKind, thumb_kind};
use stl_index::scanstore::IndexError;
use stl_index::views::{files_view, models_view};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum View {
    Models,
    Files,
}

/// One view's tree with the counts of every node.
pub struct ViewTree {
    pub tree: Tree,
    pub counts: Vec<Counts>,
}

impl ViewTree {
    fn new(tree: Tree) -> Self {
        let counts = counts(&tree);
        ViewTree { tree, counts }
    }
}

pub struct Views {
    pub models: ViewTree,
    pub files: ViewTree,
}

impl Views {
    pub fn get(&self, view: View) -> &ViewTree {
        match view {
            View::Models => &self.models,
            View::Files => &self.files,
        }
    }
}

pub fn build_views(conn: &Connection) -> Result<Views, IndexError> {
    Ok(Views { models: ViewTree::new(models_view(conn)?), files: ViewTree::new(files_view(conn)?) })
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Item {
    pub name: String,
    pub kind: &'static str,
    pub key: String,
    pub real: Vec<String>,
    pub badge: Option<String>,
    pub file_kind: Option<String>,
    pub openable: bool,
    /// Visible children.
    pub children: u32,
    pub models: u32,
    pub files: u32,
    pub images: u32,
    /// The composite's cells (2–4); empty when the item has no composite slot.
    pub composite: Vec<Thumb>,
    /// The pictures after the composite: nested images, else nested meshes; a file shows itself.
    pub pics: Vec<Thumb>,
    /// The chosen slot in `[composite?] ++ pics`.
    pub slot: usize,
}

/// A file that has (or will get) a thumbnail.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Thumb {
    pub entry_id: i64,
    /// Its `f:` key: what a saved selection stores.
    pub key: String,
}

/// The saved selection that means "slot 0, the composite".
pub const COMPOSITE: &str = "composite";
const MAX_IMAGES: usize = 11;
const MAX_MESHES: usize = 6;
const CELLS: usize = 4;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Listing {
    pub view: View,
    /// The segments actually listed: a prefix of the request when part of it no longer exists.
    pub path: Vec<String>,
    /// `""` at the root.
    pub name: String,
    pub real: Vec<String>,
    pub models: u32,
    pub files: u32,
    pub images: u32,
    pub items: Vec<Item>,
}

fn kind_name(kind: NodeKind) -> &'static str {
    match kind {
        NodeKind::Root => "root",
        NodeKind::Volume => "volume",
        NodeKind::Collection => "collection",
        NodeKind::Group => "group",
        NodeKind::Model => "model",
        NodeKind::Images => "images",
        NodeKind::Docs => "docs",
        NodeKind::Variant => "variant",
        NodeKind::Part => "part",
        NodeKind::Folder => "folder",
        NodeKind::Archive => "archive",
        NodeKind::File => "file",
        NodeKind::Image => "image",
        NodeKind::Locked => "locked",
    }
}

/// The Files view shows folders, images and object files only; the Models view shows all.
pub(crate) fn visible(view: View, n: &Node) -> bool {
    view == View::Models
        || n.kind != NodeKind::File
        || matches!(n.info.file_kind.as_deref(), Some("model" | "scene" | "image"))
}

fn leaf_thumb(n: &Node) -> Option<(ThumbKind, Thumb)> {
    if !is_file_kind(n.kind) {
        return None;
    }
    Some((thumb_kind(&n.name)?, Thumb { entry_id: n.info.entry_id?, key: n.key.clone() }))
}

/// Up to `cap` files of `kind` below `id`, depth-first in tree order, one per entry.
fn nested(t: &Tree, id: NodeId, kind: ThumbKind, cap: usize) -> Vec<Thumb> {
    let (mut out, mut seen) = (Vec::new(), HashSet::new());
    let mut stack = vec![id];
    while let Some(n) = stack.pop() {
        if out.len() >= cap {
            break;
        }
        let node = &t.nodes[n];
        match leaf_thumb(node) {
            Some((k, th)) => {
                if k == kind && seen.insert(th.entry_id) {
                    out.push(th);
                }
            }
            None => stack.extend(node.children.iter().rev()),
        }
    }
    out
}

/// A node's pictures: a file shows itself; a folder its nested images (the hero first if it
/// lives elsewhere), or its nested meshes when it has no images.
fn pics(t: &Tree, id: NodeId) -> Vec<Thumb> {
    let n = &t.nodes[id];
    if is_file_kind(n.kind) {
        return leaf_thumb(n).map(|(_, th)| vec![th]).unwrap_or_default();
    }
    let mut images = nested(t, id, ThumbKind::Image, MAX_IMAGES);
    if let (Some(entry_id), Some(key)) = (n.info.hero_entry_id, &n.info.hero_key) {
        if !images.iter().any(|p| p.entry_id == entry_id) {
            images.insert(0, Thumb { entry_id, key: key.clone() });
            images.truncate(MAX_IMAGES);
        }
    }
    if images.is_empty() { nested(t, id, ThumbKind::Mesh, MAX_MESHES) } else { images }
}

fn hero_index(n: &Node, pics: &[Thumb]) -> Option<usize> {
    n.info.hero_entry_id.and_then(|e| pics.iter().position(|p| p.entry_id == e))
}

/// The one picture a node shows as a cell of its parent's composite: its chosen picture, else
/// its hero, else its first.
fn shown_pic(t: &Tree, id: NodeId, selected: &HashMap<String, String>) -> Option<Thumb> {
    let n = &t.nodes[id];
    let pics = pics(t, id);
    let chosen = selected.get(&n.key).and_then(|k| pics.iter().position(|p| &p.key == k));
    chosen.or_else(|| hero_index(n, &pics)).or((!pics.is_empty()).then_some(0)).map(|i| pics[i].clone())
}

/// `(composite cells, pics, slot)` for one item.
fn carousel(t: &Tree, id: NodeId, selected: &HashMap<String, String>) -> (Vec<Thumb>, Vec<Thumb>, usize) {
    let n = &t.nodes[id];
    let pics = pics(t, id);
    if is_file_kind(n.kind) {
        return (Vec::new(), pics, 0);
    }
    let mut cells: Vec<Thumb> = Vec::new();
    for &c in &n.children {
        if cells.len() == CELLS {
            break;
        }
        if is_file_kind(t.nodes[c].kind) {
            continue;
        }
        if let Some(p) = shown_pic(t, c, selected) {
            if !cells.iter().any(|x| x.entry_id == p.entry_id) {
                cells.push(p);
            }
        }
    }
    if !cells.is_empty() {
        for p in &pics {
            if cells.len() == CELLS {
                break;
            }
            if !cells.iter().any(|x| x.entry_id == p.entry_id) {
                cells.push(p.clone());
            }
        }
    }
    // a "2×2" of one picture is just a worse copy of that picture
    if cells.len() < 2 {
        cells.clear();
    }
    let offset = usize::from(!cells.is_empty());
    let default = hero_index(n, &pics).map_or(0, |i| i + offset);
    let slot = match selected.get(&n.key).map(String::as_str) {
        Some(COMPOSITE) if offset == 1 => 0,
        Some(k) => pics.iter().position(|p| p.key == k).map_or(default, |i| i + offset),
        None => default,
    };
    (cells, pics, slot)
}

/// List the deepest folder along `path` that exists. `selected` maps node keys to their saved
/// carousel choice (an image key or `composite`).
pub fn list(views: &Views, view: View, path: &[String], selected: &HashMap<String, String>) -> Listing {
    let vt = views.get(view);
    let t = &vt.tree;
    let (id, used) = t.walk(path);
    let kids = |id: usize| t.nodes[id].children.iter().copied().filter(move |&c| visible(view, &t.nodes[c]));
    let items = kids(id)
        .map(|c| {
            let n = &t.nodes[c];
            let k = vt.counts[c];
            let (composite, pics, slot) = carousel(t, c, selected);
            Item {
                name: n.name.clone(),
                kind: kind_name(n.kind),
                key: n.key.clone(),
                real: n.real.clone(),
                badge: n.info.badge.clone(),
                file_kind: n.info.file_kind.clone(),
                openable: !is_file_kind(n.kind),
                children: kids(c).count() as u32,
                models: k.models,
                files: k.files,
                images: k.images,
                composite,
                pics,
                slot,
            }
        })
        .collect();
    let (n, k) = (&t.nodes[id], vt.counts[id]);
    Listing {
        view,
        path: path[..used].to_vec(),
        name: n.name.clone(),
        real: n.real.clone(),
        models: k.models,
        files: k.files,
        images: k.images,
        items,
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;
    use crate::commands::list_folder_impl;
    use crate::state::AppState;
    use crate::testutil::library;

    fn segs(p: &[&str]) -> Vec<String> {
        p.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn root_lists_volumes() {
        let dir = tempfile::tempdir().unwrap();
        let state = library(dir.path());
        let l = list_folder_impl(&state, View::Models, vec![]).unwrap();
        assert_eq!(l.path, Vec::<String>::new());
        assert_eq!(l.name, "");
        assert_eq!(l.items.iter().map(|i| i.name.as_str()).collect::<Vec<_>>(), ["vol"]);
        assert_eq!(l.items[0].kind, "volume");
        assert!(l.items[0].openable);
        assert_eq!(l.items[0].models, 2);
        assert_eq!(l.items[0].children, 1);
    }

    #[test]
    fn models_view_shows_docs_and_counts() {
        let dir = tempfile::tempdir().unwrap();
        let state = library(dir.path());
        let l = list_folder_impl(&state, View::Models, segs(&["vol", "coll", "orc"])).unwrap();
        let names: Vec<_> = l.items.iter().map(|i| i.name.as_str()).collect();
        assert_eq!(names, ["images", "docs", "orc.stl"]);
        assert_eq!((l.models, l.files, l.images), (1, 1, 1));
        let coll = list_folder_impl(&state, View::Models, segs(&["vol", "coll"])).unwrap();
        assert_eq!(coll.items[1].kind, "model");
        assert_eq!((coll.items[1].images, coll.items[1].files), (1, 1));
    }

    #[test]
    fn files_view_hides_other_files_and_shows_real() {
        let dir = tempfile::tempdir().unwrap();
        let state = library(dir.path());
        let l = list_folder_impl(&state, View::Files, segs(&["vol", "coll", "orc"])).unwrap();
        let names: Vec<_> = l.items.iter().map(|i| i.name.as_str()).collect();
        assert_eq!(names, ["orc.jpg", "orc.stl"]); // notes.txt hidden
        assert!(l.items[1].real[0].ends_with("Coll\\Orc\\orc.stl"), "{:?}", l.items[1].real);
        assert_eq!(l.items[1].file_kind.as_deref(), Some("model"));
        assert!(!l.items[1].openable);
        let coll = list_folder_impl(&state, View::Files, segs(&["vol", "coll"])).unwrap();
        assert_eq!(coll.items.iter().find(|i| i.name == "orc").unwrap().children, 2);
    }

    #[test]
    fn list_falls_back_to_deepest_existing_folder() {
        let dir = tempfile::tempdir().unwrap();
        let state = library(dir.path());
        let l = list_folder_impl(&state, View::Files, segs(&["vol", "coll", "gone", "x"])).unwrap();
        assert_eq!(l.path, ["vol", "coll"]);
        assert_eq!(l.name, "coll");
    }

    #[test]
    fn list_on_a_file_lists_its_folder() {
        let dir = tempfile::tempdir().unwrap();
        let state = library(dir.path());
        let l = list_folder_impl(&state, View::Files, segs(&["vol", "coll", "elf", "elf.stl"])).unwrap();
        assert_eq!(l.path, ["vol", "coll", "elf"]);
    }

    #[test]
    fn empty_db_lists_nothing() {
        let dir = tempfile::tempdir().unwrap();
        let state = AppState::open(dir.path().to_path_buf()).unwrap();
        for v in [View::Models, View::Files] {
            assert!(list_folder_impl(&state, v, vec![]).unwrap().items.is_empty());
        }
    }

    #[test]
    fn views_are_cached_until_replaced() {
        let dir = tempfile::tempdir().unwrap();
        let state = library(dir.path());
        let a = state.views().unwrap();
        assert!(Arc::ptr_eq(&a, &state.views().unwrap()));
        let conn = stl_index::db::open(&state.db_path()).unwrap();
        state.set_views(build_views(&conn).unwrap());
        assert!(!Arc::ptr_eq(&a, &state.views().unwrap()));
    }

    fn item<'a>(l: &'a Listing, name: &str) -> &'a Item {
        l.items.iter().find(|i| i.name == name).unwrap_or_else(|| panic!("no item {name}"))
    }

    fn select(state: &AppState, node_key: &str, image_key: &str) {
        crate::commands::select_thumb_impl(state, node_key.into(), image_key.into()).unwrap();
    }

    #[test]
    fn model_with_image_defaults_to_it() {
        let dir = tempfile::tempdir().unwrap();
        let state = library(dir.path());
        let l = list_folder_impl(&state, View::Models, segs(&["vol", "coll"])).unwrap();
        let orc = item(&l, "orc");
        assert!(orc.pics[0].key.ends_with("orc.jpg"), "{:?}", orc.pics);
        assert!(orc.composite.is_empty()); // one child folder with a picture is not a composite
        assert_eq!(orc.slot, 0);
        let elf = item(&l, "elf");
        assert_eq!(elf.pics.len(), 1);
        assert!(elf.pics[0].key.ends_with("elf.stl")); // meshes when there are no images
    }

    #[test]
    fn collection_composite_from_children() {
        let dir = tempfile::tempdir().unwrap();
        let state = library(dir.path());
        let l = list_folder_impl(&state, View::Models, segs(&["vol"])).unwrap();
        let coll = item(&l, "coll");
        let cells: Vec<&str> = coll.composite.iter().map(|t| t.key.rsplit('|').next().unwrap()).collect();
        assert_eq!(cells, ["Elf/elf.stl", "Orc/orc.jpg"]);
        assert_eq!(coll.pics.len(), 1); // the one nested image
        assert_eq!(coll.slot, 0);
    }

    #[test]
    fn stale_selection_falls_back_to_default() {
        let dir = tempfile::tempdir().unwrap();
        let state = library(dir.path());
        let vol = list_folder_impl(&state, View::Models, segs(&["vol"])).unwrap();
        let coll = item(&vol, "coll").clone();
        select(&state, &coll.key, &coll.pics[0].key);
        assert_eq!(item(&list_folder_impl(&state, View::Models, segs(&["vol"])).unwrap(), "coll").slot, 1);
        select(&state, &coll.key, "composite");
        assert_eq!(item(&list_folder_impl(&state, View::Models, segs(&["vol"])).unwrap(), "coll").slot, 0);
        select(&state, &coll.key, "f:gone|x.jpg");
        assert_eq!(item(&list_folder_impl(&state, View::Models, segs(&["vol"])).unwrap(), "coll").slot, 0);
        // a model with no composite ignores "composite"
        let l = list_folder_impl(&state, View::Models, segs(&["vol", "coll"])).unwrap();
        select(&state, &item(&l, "orc").key, "composite");
        assert_eq!(item(&list_folder_impl(&state, View::Models, segs(&["vol", "coll"])).unwrap(), "orc").slot, 0);
    }

    #[test]
    fn parent_cell_shows_the_childs_picture() {
        let dir = tempfile::tempdir().unwrap();
        let state = library(dir.path());
        std::fs::write(dir.path().join("lib/Vol/Coll/Elf/elf.png"), b"p").unwrap();
        let (done, views) = crate::indexer::run_index(&state.db_path(), &mut |_| {});
        assert!(done.ok, "{:?}", done.error);
        state.set_views(views.unwrap());
        let l = list_folder_impl(&state, View::Files, segs(&["vol", "coll"])).unwrap();
        let elf = item(&l, "elf").clone();
        assert_eq!(elf.pics.len(), 1); // images win over meshes
        let cell = |s: &AppState| {
            let vol = list_folder_impl(s, View::Files, segs(&["vol"])).unwrap();
            item(&vol, "coll").composite.iter().map(|t| t.key.rsplit('|').next().unwrap().to_string()).collect::<Vec<_>>()
        };
        assert_eq!(cell(&state)[0], "Elf/elf.png");
    }

    #[test]
    fn leaf_files_show_themselves() {
        let dir = tempfile::tempdir().unwrap();
        let state = library(dir.path());
        let l = list_folder_impl(&state, View::Files, segs(&["vol", "coll", "orc"])).unwrap();
        let stl = item(&l, "orc.stl");
        assert_eq!(stl.pics, [Thumb { entry_id: stl.pics[0].entry_id, key: stl.key.clone() }]);
        assert!(stl.composite.is_empty());
        let jpg = item(&l, "orc.jpg");
        assert_eq!(jpg.pics.len(), 1);
    }

    #[test]
    fn serializes_camel_case() {
        let dir = tempfile::tempdir().unwrap();
        let state = library(dir.path());
        let l = list_folder_impl(&state, View::Files, segs(&["vol", "coll", "orc"])).unwrap();
        let v = serde_json::to_value(&l).unwrap();
        assert_eq!(v["view"], "files");
        assert_eq!(v["items"][1]["fileKind"], "model");
        assert_eq!(v["items"][1]["openable"], false);
        let view: View = serde_json::from_str("\"models\"").unwrap();
        assert_eq!(view, View::Models);
    }
}
