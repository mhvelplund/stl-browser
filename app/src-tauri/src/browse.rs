//! Folder listings for the UI, served from the cached view trees.
use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use stl_core::vtree::{Counts, Node, NodeKind, Tree, counts, is_file_kind};
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
}

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
fn visible(view: View, n: &Node) -> bool {
    view == View::Models
        || n.kind != NodeKind::File
        || matches!(n.info.file_kind.as_deref(), Some("model" | "scene" | "image"))
}

/// List the deepest folder along `path` that exists.
pub fn list(views: &Views, view: View, path: &[String]) -> Listing {
    let vt = match view {
        View::Models => &views.models,
        View::Files => &views.files,
    };
    let t = &vt.tree;
    let (id, used) = t.walk(path);
    let kids = |id: usize| t.nodes[id].children.iter().copied().filter(move |&c| visible(view, &t.nodes[c]));
    let items = kids(id)
        .map(|c| {
            let n = &t.nodes[c];
            let k = vt.counts[c];
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
