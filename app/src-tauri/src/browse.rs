//! Folder listings for the UI, served from the cached view trees, with each item's carousel:
//! an optional composite (slot 0) and the pictures nested in it.
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;

use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use stl_core::vtree::filter::Shown;
use stl_core::vtree::search::{Field, Inherited, Matcher, Mode, TagFilter, search as vtree_search};
use stl_core::vtree::tags::{Tag, UserTags, copy_tags, link_tags, parse_query};
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
    /// Every generated tag in the Models tree (distinct, sorted).
    pub generated: Vec<Tag>,
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
    let mut models = models_view(conn)?;
    let mut files = files_view(conn)?;
    copy_tags(&models, &mut files);
    link_tags(&mut models);
    link_tags(&mut files);
    let generated = generated_tags(&models);
    Ok(Views { models: ViewTree::new(models), files: ViewTree::new(files), generated })
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
    /// The item's own effective tags (inherited ones count in search only).
    pub tags: Vec<TagView>,
}

/// A tag as the UI sees it.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct TagView {
    pub kind: String,
    pub name: String,
    pub by_user: bool,
}

impl From<&Tag> for TagView {
    fn from(t: &Tag) -> Self {
        TagView { kind: t.kind.as_str().to_string(), name: t.name.clone(), by_user: t.by_user }
    }
}

/// A tag a node gets from the folder `from` (path segments).
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct InheritedTag {
    pub tag: TagView,
    pub from: Vec<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
pub struct NodeTags {
    pub own: Vec<TagView>,
    pub inherited: Vec<InheritedTag>,
    /// Generated tags the user hid on this node.
    pub hidden: Vec<TagView>,
    /// For a node that shares its folder with the node above, the one to tag instead.
    pub owner: Option<Owner>,
}

/// An item the UI points at: `name` in the folder at `path` (`name == ""` is that folder).
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Target {
    pub path: Vec<String>,
    pub name: String,
}

/// A node by its name and the path of the folder that lists it.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Owner {
    pub name: String,
    pub path: Vec<String>,
}

fn tag_views(tags: &[Tag]) -> Vec<TagView> {
    tags.iter().map(TagView::from).collect()
}

/// The node at `(path, name)`; `name == ""` is the folder at `path` itself. `ancestors` are the
/// nodes above it from the root, each with the path segments that lead to it.
pub struct Located {
    pub id: NodeId,
    pub ancestors: Vec<(NodeId, Vec<String>)>,
}

pub fn locate_node(views: &Views, view: View, path: &[String], name: &str) -> Result<Located, String> {
    let t = &views.get(view).tree;
    let gone = || format!("{} no longer exists", if name.is_empty() { path.last().map_or("the library", String::as_str) } else { name });
    let mut chain = vec![(0, Vec::new())];
    for (i, part) in path.iter().enumerate() {
        let part = part.to_lowercase();
        let cur = chain[i].0;
        match t.nodes[cur].children.iter().find(|&&c| t.nodes[c].name == part && !is_file_kind(t.nodes[c].kind)) {
            Some(&c) => chain.push((c, path[..=i].to_vec())),
            None => return Err(gone()),
        }
    }
    if name.is_empty() {
        let (id, _) = chain.pop().expect("chain has the root");
        return Ok(Located { id, ancestors: chain });
    }
    let name = name.to_lowercase();
    let id = t.nodes[chain[chain.len() - 1].0]
        .children
        .iter()
        .copied()
        .find(|&c| t.nodes[c].name == name)
        .ok_or_else(gone)?;
    Ok(Located { id, ancestors: chain })
}

/// A node's own, inherited and hidden tags.
pub fn node_tags(views: &Views, view: View, path: &[String], name: &str, user: &UserTags) -> Result<NodeTags, String> {
    let t = &views.get(view).tree;
    let at = locate_node(views, view, path, name)?;
    let mut inherited = Vec::new();
    for (id, from) in &at.ancestors {
        for tag in user.effective(&t.nodes[*id]) {
            inherited.push(InheritedTag { tag: TagView::from(&tag), from: from.clone() });
        }
    }
    Ok(NodeTags {
        own: tag_views(&user.effective(&t.nodes[at.id])),
        inherited,
        hidden: tag_views(&user.hidden_of(&t.nodes[at.id])),
        owner: t.nodes[at.id].info.tag_owner.map(|o| owner(t, &at, o)),
    })
}

/// The owner `o` of the located node: found among its ancestors (the path the user came by),
/// else along the first path from the root.
fn owner(t: &Tree, at: &Located, o: NodeId) -> Owner {
    let path = match at.ancestors.iter().find(|(id, _)| *id == o) {
        Some((_, from)) => from[..from.len() - 1].to_vec(),
        None => path_to(t, o).unwrap_or_default(),
    };
    Owner { name: t.nodes[o].name.clone(), path }
}

/// The segments of the folder that lists `target`, on the first path from the root.
fn path_to(t: &Tree, target: NodeId) -> Option<Vec<String>> {
    fn go(t: &Tree, id: NodeId, target: NodeId, path: &mut Vec<String>) -> bool {
        for &c in &t.nodes[id].children {
            if c == target {
                return true;
            }
            path.push(t.nodes[c].name.clone());
            if go(t, c, target, path) {
                return true;
            }
            path.pop();
        }
        false
    }
    let mut path = Vec::new();
    go(t, 0, target, &mut path).then_some(path)
}

/// Every generated tag in a tree (distinct, `by_user` false).
fn generated_tags(t: &Tree) -> Vec<Tag> {
    let mut all: Vec<Tag> = t.nodes.iter().flat_map(|n| n.info.tags.iter().cloned()).collect();
    all.sort();
    all.dedup();
    all
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
    /// The listed folder's node key (what a saved thumbnail choice is stored under).
    pub key: String,
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

/// Both views list folders, archives and the files whose ending is shown (`config.yaml`).
pub(crate) fn visible(shown: &Shown, n: &Node) -> bool {
    shown.shows(n)
}

fn leaf_thumb(n: &Node) -> Option<(ThumbKind, Thumb)> {
    if !is_file_kind(n.kind) {
        return None;
    }
    Some((thumb_kind(&n.name)?, Thumb { entry_id: n.info.entry_id?, key: n.key.clone() }))
}

/// The file with this key below `id`, if it has a thumbnail.
fn find_leaf(t: &Tree, id: NodeId, key: &str) -> Option<(ThumbKind, Thumb)> {
    let mut stack = vec![id];
    while let Some(n) = stack.pop() {
        let node = &t.nodes[n];
        if is_file_kind(node.kind) {
            if node.key == key {
                return leaf_thumb(node);
            }
        } else {
            stack.extend(node.children.iter().rev());
        }
    }
    None
}

/// The picture saved for folder `id`, if it is still a file below it (`composite` is not one).
fn chosen_pic(t: &Tree, id: NodeId, selected: &HashMap<String, String>) -> Option<(ThumbKind, Thumb)> {
    let key = selected.get(&t.nodes[id].key).filter(|k| *k != COMPOSITE)?;
    find_leaf(t, id, key)
}

/// The images and meshes below `id` (up to their caps), depth-first in tree order, one per
/// entry. A folder below with a saved picture stands for itself with just that picture.
fn nested(t: &Tree, id: NodeId, selected: &HashMap<String, String>) -> (Vec<Thumb>, Vec<Thumb>) {
    let (mut images, mut meshes, mut seen) = (Vec::new(), Vec::new(), HashSet::new());
    let mut stack: Vec<NodeId> = t.nodes[id].children.iter().rev().copied().collect();
    while let Some(n) = stack.pop() {
        if images.len() >= MAX_IMAGES && meshes.len() >= MAX_MESHES {
            break;
        }
        let node = &t.nodes[n];
        let found = if is_file_kind(node.kind) { leaf_thumb(node) } else { chosen_pic(t, n, selected) };
        match found {
            Some((kind, th)) => {
                let (list, cap) = match kind {
                    ThumbKind::Image => (&mut images, MAX_IMAGES),
                    ThumbKind::Mesh => (&mut meshes, MAX_MESHES),
                };
                if list.len() < cap && seen.insert(th.entry_id) {
                    list.push(th);
                }
            }
            None if !is_file_kind(node.kind) => stack.extend(node.children.iter().rev()),
            None => {}
        }
    }
    (images, meshes)
}

/// A node's pictures: a file shows itself; a folder its nested images (the hero first if it
/// lives elsewhere), then its nested meshes. A saved picture deeper down than those comes first.
fn pics(t: &Tree, id: NodeId, selected: &HashMap<String, String>) -> Vec<Thumb> {
    let n = &t.nodes[id];
    if is_file_kind(n.kind) {
        return leaf_thumb(n).map(|(_, th)| vec![th]).unwrap_or_default();
    }
    let (mut images, meshes) = nested(t, id, selected);
    if let (Some(entry_id), Some(key)) = (n.info.hero_entry_id, &n.info.hero_key) {
        if !images.iter().any(|p| p.entry_id == entry_id) {
            images.insert(0, Thumb { entry_id, key: key.clone() });
            images.truncate(MAX_IMAGES);
        }
    }
    images.extend(meshes);
    if let Some((_, th)) = chosen_pic(t, id, selected) {
        if !images.iter().any(|p| p.entry_id == th.entry_id) {
            images.insert(0, th);
        }
    }
    images
}

fn hero_index(n: &Node, pics: &[Thumb]) -> Option<usize> {
    n.info.hero_entry_id.and_then(|e| pics.iter().position(|p| p.entry_id == e))
}

/// The one picture a node shows as a cell of its parent's composite: its chosen picture, else
/// its hero, else its first.
fn shown_pic(t: &Tree, id: NodeId, selected: &HashMap<String, String>) -> Option<Thumb> {
    let n = &t.nodes[id];
    let pics = pics(t, id, selected);
    let chosen = selected.get(&n.key).and_then(|k| pics.iter().position(|p| &p.key == k));
    chosen.or_else(|| hero_index(n, &pics)).or((!pics.is_empty()).then_some(0)).map(|i| pics[i].clone())
}

/// `(composite cells, pics, slot)` for one item.
fn carousel(t: &Tree, id: NodeId, selected: &HashMap<String, String>) -> (Vec<Thumb>, Vec<Thumb>, usize) {
    let n = &t.nodes[id];
    let pics = pics(t, id, selected);
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
        // fill with images, or with meshes when there are none (a mesh cell next to the
        // folder's one image is not worth a composite)
        let is_image = |p: &Thumb| thumb_kind(&p.key) == Some(ThumbKind::Image);
        let any_image = pics.iter().any(is_image);
        for p in pics.iter().filter(|p| !any_image || is_image(p)) {
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

/// The tile for node `c`, with its carousel.
fn item(vt: &ViewTree, c: NodeId, shown: &Shown, selected: &HashMap<String, String>, user: &UserTags) -> Item {
    let t = &vt.tree;
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
        children: n.children.iter().filter(|&&g| visible(shown, &t.nodes[g])).count() as u32,
        models: k.models,
        files: k.files,
        images: k.images,
        composite,
        pics,
        slot,
        tags: tag_views(&user.effective(n)),
    }
}

/// List the deepest folder along `path` that exists. `selected` maps node keys to their saved
/// carousel choice (an image key or `composite`).
pub fn list(
    views: &Views,
    view: View,
    path: &[String],
    selected: &HashMap<String, String>,
    user: &UserTags,
    shown: &Shown,
) -> Listing {
    let vt = views.get(view);
    let t = &vt.tree;
    let (id, used) = t.walk(path);
    let items = t.nodes[id]
        .children
        .iter()
        .filter(|&&c| visible(shown, &t.nodes[c]))
        .map(|&c| item(vt, c, shown, selected, user))
        .collect();
    let (n, k) = (&t.nodes[id], vt.counts[id]);
    Listing {
        view,
        path: path[..used].to_vec(),
        name: n.name.clone(),
        key: n.key.clone(),
        real: n.real.clone(),
        models: k.models,
        files: k.files,
        images: k.images,
        items,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SearchMode {
    Free,
    Word,
    Regexp,
}

/// One search result: the tile its folder would show, and `parent`, the folder that lists it,
/// so the per-folder commands (open, drag, extract) work on it.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Hit {
    #[serde(flatten)]
    pub item: Item,
    pub parent: Vec<String>,
    /// Char ranges `[start, end)` into `name` to highlight.
    pub marks: Vec<(usize, usize)>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Found {
    /// The scope actually searched: a prefix of the request when part of it no longer exists.
    pub path: Vec<String>,
    pub hits: Vec<Hit>,
    pub truncated: bool,
}

pub const SEARCH_LIMIT: usize = 1000;

/// What a search asks for.
pub struct Query<'a> {
    pub text: &'a str,
    pub mode: SearchMode,
    pub case_sensitive: bool,
}

/// The shown folders and files below `path` whose names match the query. The Files view matches
/// names with their case; the Models view's names are lowercase, so it always ignores case.
pub fn search(
    views: &Views,
    view: View,
    path: &[String],
    q: &Query,
    selected: &HashMap<String, String>,
    user: &UserTags,
    shown: &Shown,
) -> Result<Found, String> {
    let (mode, case_sensitive) = (q.mode, q.case_sensitive);
    let vt = views.get(view);
    let t = &vt.tree;
    let (id, used) = t.walk(path);
    let path = path[..used].to_vec();
    // `tag:` terms are split off; a query without any keeps its text exactly
    let parsed = parse_query(q.text);
    let text = if parsed.tags.is_empty() && parsed.text.split_whitespace().eq(q.text.split_whitespace()) {
        q.text
    } else {
        parsed.text.as_str()
    };
    if text.is_empty() && parsed.tags.is_empty() {
        return Ok(Found { path, hits: Vec::new(), truncated: false });
    }
    let mode = match mode {
        SearchMode::Free => Mode::Free,
        SearchMode::Word => Mode::Word,
        SearchMode::Regexp => Mode::Regexp,
    };
    let (field, case_sensitive) = match view {
        View::Files => (Field::Label, case_sensitive),
        View::Models => (Field::Name, false),
    };
    let m = if text.is_empty() { None } else { Some(Matcher::new(text, mode, case_sensitive)?) };
    let own = |id: NodeId| user.effective(&t.nodes[id]);
    let filter = TagFilter { terms: &parsed.tags, own: &own, start: Inherited::along(t, &path, &own) };
    let (found, truncated) = vtree_search(t, id, &path, m.as_ref(), field, &|n| visible(shown, n), &filter, SEARCH_LIMIT);
    let hits = found
        .into_iter()
        .map(|h| Hit { item: item(vt, h.id, shown, selected, user), parent: h.parent, marks: h.marks })
        .collect();
    Ok(Found { path, hits, truncated })
}

/// The disk part of a hover path: up to and including the first archive (`…\Pack.rar!a/b.stl`
/// gives `…\Pack.rar`). A `!` that doesn't follow an archive name is part of a folder name.
pub fn disk_part(real: &str) -> &str {
    for (i, _) in real.match_indices('!') {
        let before = &real[..i];
        let name = before.rsplit(['\\', '/']).next().unwrap_or(before);
        if stl_core::names::kind_of(name) == stl_core::names::Kind::Archive {
            return before;
        }
    }
    real
}

/// What a rescan of the folder at `path` must look at: the disk folders and archives behind it
/// (several for a merged Models folder), none inside another. `None` at the library root
/// (everything); an error when the folder is gone or has no real place.
pub fn scope(views: &Views, view: View, path: &[String]) -> Result<Option<Vec<PathBuf>>, String> {
    if path.is_empty() {
        return Ok(None);
    }
    let t = &views.get(view).tree;
    let (id, used) = t.walk(path);
    if used != path.len() {
        return Err(format!("the folder {} no longer exists", path.join("/")));
    }
    let mut paths: Vec<String> = t.nodes[id].real.iter().map(|r| disk_part(r).to_string()).collect();
    paths.sort_by_key(|p| p.len());
    let mut out: Vec<String> = Vec::new();
    for p in paths {
        let k = p.to_lowercase();
        let covered = out.iter().any(|o| {
            let o = o.to_lowercase();
            k == o || k.strip_prefix(&o).is_some_and(|r| r.starts_with('\\'))
        });
        if !covered {
            out.push(p);
        }
    }
    if out.is_empty() {
        return Err(format!("{} has no folder on disk to rescan", path.join("/")));
    }
    Ok(Some(out.into_iter().map(PathBuf::from).collect()))
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
    fn files_view_lists_shown_files_and_real_paths() {
        let dir = tempfile::tempdir().unwrap();
        let mut state = library(dir.path());
        let l = list_folder_impl(&state, View::Files, segs(&["vol", "coll", "orc"])).unwrap();
        let names: Vec<_> = l.items.iter().map(|i| i.name.as_str()).collect();
        assert_eq!(names, ["notes.txt", "orc.jpg", "orc.stl"]);
        let stl = item(&l, "orc.stl");
        assert!(stl.real[0].ends_with("Coll\\Orc\\orc.stl"), "{:?}", stl.real);
        assert_eq!(stl.file_kind.as_deref(), Some("model"));
        assert!(!stl.openable);
        let coll = list_folder_impl(&state, View::Files, segs(&["vol", "coll"])).unwrap();
        assert_eq!(item(&coll, "orc").children, 3);
        // without `txt` in the config the notes are hidden, and not counted as children
        state.shown = Shown::new(["stl", "jpg"]);
        let l = list_folder_impl(&state, View::Files, segs(&["vol", "coll", "orc"])).unwrap();
        assert_eq!(l.items.iter().map(|i| i.name.as_str()).collect::<Vec<_>>(), ["orc.jpg", "orc.stl"]);
        let coll = list_folder_impl(&state, View::Files, segs(&["vol", "coll"])).unwrap();
        assert_eq!(item(&coll, "orc").children, 2);
        let l = list_folder_impl(&state, View::Models, segs(&["vol", "coll", "orc"])).unwrap();
        assert_eq!(item(&l, "docs").children, 0);
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
        assert_eq!(coll.pics.len(), 3); // the one nested image, then the meshes
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
        let (done, views) = crate::indexer::run_index(&state.db_path(), None, &Default::default(), &mut |_| {});
        assert!(done.ok, "{:?}", done.error);
        state.set_views(views.unwrap());
        let l = list_folder_impl(&state, View::Files, segs(&["vol", "coll"])).unwrap();
        let elf = item(&l, "elf").clone();
        assert!(elf.pics[0].key.ends_with("elf.png")); // images before meshes
        let cell = |s: &AppState| {
            let vol = list_folder_impl(s, View::Files, segs(&["vol"])).unwrap();
            item(&vol, "coll").composite.iter().map(|t| t.key.rsplit('|').next().unwrap().to_string()).collect::<Vec<_>>()
        };
        assert_eq!(cell(&state)[0], "Elf/elf.png");
    }

    #[test]
    fn meshes_follow_the_images() {
        let dir = tempfile::tempdir().unwrap();
        let state = library(dir.path());
        let l = list_folder_impl(&state, View::Files, segs(&["vol", "coll"])).unwrap();
        let orc = item(&l, "orc");
        let names: Vec<&str> = orc.pics.iter().map(|t| t.key.rsplit('/').next().unwrap()).collect();
        assert_eq!(names, ["orc.jpg", "orc.stl"]);
        assert_eq!(orc.slot, 0); // the image stays the default
        // the mesh can be chosen
        select(&state, &orc.key, &orc.pics[1].key);
        let l = list_folder_impl(&state, View::Files, segs(&["vol", "coll"])).unwrap();
        assert_eq!(item(&l, "orc").slot, 1);
    }

    #[test]
    fn a_folders_choice_stands_for_it_in_its_parents() {
        let dir = tempfile::tempdir().unwrap();
        let state = library(dir.path());
        let names = |s: &AppState| {
            let l = list_folder_impl(s, View::Files, segs(&["vol"])).unwrap();
            item(&l, "coll").pics.iter().map(|t| t.key.rsplit('/').next().unwrap().to_string()).collect::<Vec<_>>()
        };
        assert_eq!(names(&state), ["orc.jpg", "elf.stl", "orc.stl"]);
        // Orc's choice is its mesh: Coll sees only that from Orc
        let l = list_folder_impl(&state, View::Files, segs(&["vol", "coll"])).unwrap();
        let orc = item(&l, "orc").clone();
        select(&state, &orc.key, &orc.pics[1].key);
        assert_eq!(names(&state), ["elf.stl", "orc.stl"]);
        // a stale choice is ignored
        select(&state, &orc.key, "f:gone|x.stl");
        assert_eq!(names(&state), ["orc.jpg", "elf.stl", "orc.stl"]);
    }

    #[test]
    fn a_deep_choice_comes_first() {
        let dir = tempfile::tempdir().unwrap();
        let state = library(dir.path());
        let l = list_folder_impl(&state, View::Files, segs(&["vol", "coll", "orc"])).unwrap();
        let stl = item(&l, "orc.stl").key.clone();
        // chosen for Vol (two levels up) from the right-click menu
        let vol = list_folder_impl(&state, View::Files, vec![]).unwrap();
        select(&state, &item(&vol, "vol").key, &stl);
        let vol = list_folder_impl(&state, View::Files, vec![]).unwrap();
        let v = item(&vol, "vol");
        let slot = v.slot - usize::from(!v.composite.is_empty());
        assert_eq!(v.pics[slot].key, stl);
        assert_eq!(l.key, item(&list_folder_impl(&state, View::Files, segs(&["vol", "coll"])).unwrap(), "orc").key);
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
    fn disk_part_stops_at_the_first_archive() {
        assert_eq!(disk_part(r"D:\Models\Vol\Grimlax.rar!Grimlax/842.zip!a/body.stl"), r"D:\Models\Vol\Grimlax.rar");
        assert_eq!(disk_part(r"D:\Models\Wow!\Orc"), r"D:\Models\Wow!\Orc");
        assert_eq!(disk_part(r"D:\Models\Wow!\P.zip!x.stl"), r"D:\Models\Wow!\P.zip");
    }

    #[test]
    fn scope_of_a_folder() {
        let dir = tempfile::tempdir().unwrap();
        let state = library(dir.path());
        let views = state.views().unwrap();
        assert_eq!(scope(&views, View::Models, &[]).unwrap(), None);
        let s = scope(&views, View::Files, &segs(&["vol", "coll", "orc"])).unwrap().unwrap();
        assert_eq!(s.len(), 1);
        assert!(s[0].ends_with("Vol/Coll/Orc"), "{s:?}");
        let s = scope(&views, View::Models, &segs(&["vol", "coll"])).unwrap().unwrap();
        assert!(s.iter().all(|p| !p.to_string_lossy().contains('!')), "{s:?}");
        assert!(scope(&views, View::Files, &segs(&["vol", "gone"])).is_err());
    }

    #[test]
    fn serializes_camel_case() {
        let dir = tempfile::tempdir().unwrap();
        let state = library(dir.path());
        let l = list_folder_impl(&state, View::Files, segs(&["vol", "coll", "orc"])).unwrap();
        let v = serde_json::to_value(&l).unwrap();
        assert_eq!(v["view"], "files");
        assert_eq!(v["items"][2]["name"], "orc.stl");
        assert_eq!(v["items"][2]["fileKind"], "model");
        assert_eq!(v["items"][2]["openable"], false);
        assert!(v["items"][2]["tags"].is_array());
        let view: View = serde_json::from_str("\"models\"").unwrap();
        assert_eq!(view, View::Models);
    }
    fn find(state: &AppState, view: View, path: &[&str], q: &str, mode: SearchMode, cs: bool) -> Found {
        crate::commands::search_impl(state, view, segs(path), q.into(), mode, cs).unwrap()
    }

    fn hit_names(f: &Found) -> Vec<&str> {
        f.hits.iter().map(|h| h.item.name.as_str()).collect()
    }

    #[test]
    fn search_scope_excludes_itself() {
        let dir = tempfile::tempdir().unwrap();
        let state = library(dir.path());
        let f = find(&state, View::Files, &["vol", "coll"], "o", SearchMode::Word, false);
        assert_eq!(hit_names(&f), ["orc", "notes.txt", "orc.jpg", "orc.stl"]);
        assert_eq!(f.path, ["vol", "coll"]);
    }

    /// The regression: `.exe$` found exes in Models but not in Files. Both views now list (and
    /// search) the same endings.
    #[test]
    fn both_views_search_the_shown_endings() {
        let dir = tempfile::tempdir().unwrap();
        let mut state = library(dir.path());
        std::fs::write(dir.path().join("lib/Vol/Coll/Orc/setup.exe"), b"x").unwrap();
        let (done, views) = crate::indexer::run_index(&state.db_path(), None, &Default::default(), &mut |_| {});
        assert!(done.ok, "{:?}", done.error);
        state.set_views(views.unwrap());
        let exes = |s: &AppState, v| find(s, v, &[], r"\.exe$", SearchMode::Regexp, false).hits.len();
        assert_eq!((exes(&state, View::Models), exes(&state, View::Files)), (0, 0));
        assert_eq!(find(&state, View::Models, &[], "notes", SearchMode::Word, false).hits.len(), 1);
        assert_eq!(find(&state, View::Files, &[], "notes", SearchMode::Word, false).hits.len(), 1);
        state.shown = Shown::new(["exe"]);
        assert_eq!((exes(&state, View::Models), exes(&state, View::Files)), (1, 1));
        assert!(find(&state, View::Files, &[], "notes", SearchMode::Word, false).hits.is_empty());
    }

    #[test]
    fn hits_carry_their_carousel() {
        let dir = tempfile::tempdir().unwrap();
        let state = library(dir.path());
        let f = find(&state, View::Models, &[], "orc", SearchMode::Word, false);
        let orc = f.hits.iter().find(|h| h.item.name == "orc").unwrap();
        let listed = list_folder_impl(&state, View::Models, orc.parent.clone()).unwrap();
        let same = item(&listed, "orc");
        assert_eq!((orc.item.pics.clone(), orc.item.slot, orc.item.children), (same.pics.clone(), same.slot, same.children));
    }

    #[test]
    fn models_view_ignores_case_flag() {
        let dir = tempfile::tempdir().unwrap();
        let state = library(dir.path());
        assert!(hit_names(&find(&state, View::Models, &[], "ORC", SearchMode::Word, true)).contains(&"orc"));
    }

    #[test]
    fn files_view_case_sensitive_uses_disk_case() {
        let dir = tempfile::tempdir().unwrap();
        let state = library(dir.path());
        assert_eq!(hit_names(&find(&state, View::Files, &[], "Orc", SearchMode::Word, true)), ["orc"]);
        assert!(find(&state, View::Files, &[], "ORC", SearchMode::Word, true).hits.is_empty());
    }

    #[test]
    fn parent_round_trips() {
        let dir = tempfile::tempdir().unwrap();
        let state = library(dir.path());
        let f = find(&state, View::Models, &[], "o", SearchMode::Free, false);
        assert!(!f.hits.is_empty());
        for h in &f.hits {
            let l = list_folder_impl(&state, View::Models, h.parent.clone()).unwrap();
            assert!(l.items.iter().any(|i| i.name == h.item.name), "{} not in {:?}", h.item.name, h.parent);
        }
    }

    #[test]
    fn vanished_scope_falls_back() {
        let dir = tempfile::tempdir().unwrap();
        let state = library(dir.path());
        let f = find(&state, View::Files, &["vol", "gone"], "orc", SearchMode::Word, false);
        assert_eq!(f.path, ["vol"]);
        assert!(!f.hits.is_empty());
    }

    #[test]
    fn bad_regexp_is_an_error() {
        let dir = tempfile::tempdir().unwrap();
        let state = library(dir.path());
        assert!(crate::commands::search_impl(&state, View::Files, vec![], "(".into(), SearchMode::Regexp, false).is_err());
    }

    #[test]
    fn empty_query_finds_nothing() {
        let dir = tempfile::tempdir().unwrap();
        let state = library(dir.path());
        assert!(find(&state, View::Files, &[], "", SearchMode::Free, false).hits.is_empty());
    }

    fn tv(kind: &str, name: &str) -> TagView {
        TagView { kind: kind.into(), name: name.into(), by_user: true }
    }

    fn tags_of(state: &AppState, view: View, path: &[&str], name: &str) -> Vec<TagView> {
        item(&list_folder_impl(state, view, segs(path)).unwrap(), name).tags.clone()
    }

    fn tag(state: &AppState, view: View, path: &[&str], name: &str, kind: &str, tag: &str) {
        crate::commands::add_tag_impl(state, view, segs(path), name.into(), kind.into(), tag.into()).unwrap();
    }

    #[test]
    fn tag_shows_in_both_views() {
        let dir = tempfile::tempdir().unwrap();
        let state = library(dir.path());
        tag(&state, View::Models, &["vol", "coll"], "orc", "user", "Tinmen");
        assert_eq!(tags_of(&state, View::Models, &["vol", "coll"], "orc"), [tv("user", "tinmen")]);
        assert_eq!(tags_of(&state, View::Files, &["vol", "coll"], "orc"), [tv("user", "tinmen")]);
        assert!(tags_of(&state, View::Files, &["vol", "coll"], "elf").is_empty());
    }

    #[test]
    fn remove_clears_every_real_path() {
        let dir = tempfile::tempdir().unwrap();
        let state = library(dir.path());
        tag(&state, View::Models, &["vol", "coll"], "orc", "user", "Tinmen");
        crate::commands::remove_tag_impl(&state, View::Models, segs(&["vol", "coll"]), "orc".into(), "user".into(), "tinmen".into())
            .unwrap();
        assert!(tags_of(&state, View::Models, &["vol", "coll"], "orc").is_empty());
        assert!(tags_of(&state, View::Files, &["vol", "coll"], "orc").is_empty());
    }

    #[test]
    fn tag_search_topmost_and_inside() {
        let dir = tempfile::tempdir().unwrap();
        let state = library(dir.path());
        tag(&state, View::Models, &["vol", "coll"], "orc", "user", "Tinmen");
        let run = |q: &str| find(&state, View::Models, &[], q, SearchMode::Word, false);
        assert_eq!(hit_names(&run("tag:tinmen")), ["orc"]);
        assert_eq!(hit_names(&run("tag:tinmen orc.stl")), ["orc.stl"]);
        assert!(run("tag:tinmen elf").hits.is_empty());
        assert!(run("tag:").hits.is_empty());
    }

    #[test]
    fn node_tags_lists_inherited() {
        let dir = tempfile::tempdir().unwrap();
        let state = library(dir.path());
        tag(&state, View::Models, &["vol", "coll"], "orc", "user", "Tinmen");
        let n = crate::commands::node_tags_impl(&state, View::Files, segs(&["vol", "coll", "orc"]), "orc.stl".into()).unwrap();
        assert!(n.own.is_empty());
        assert_eq!(n.inherited, [InheritedTag { tag: tv("user", "tinmen"), from: segs(&["vol", "coll", "orc"]) }]);
        // the folder itself: its own tag is own, nothing is inherited
        let n = crate::commands::node_tags_impl(&state, View::Files, segs(&["vol", "coll", "orc"]), String::new()).unwrap();
        assert_eq!(n.own, [tv("user", "tinmen")]);
        assert!(n.inherited.is_empty());
    }

    #[test]
    fn setting_a_scale_replaces_it() {
        let dir = tempfile::tempdir().unwrap();
        let state = library(dir.path());
        tag(&state, View::Models, &["vol", "coll"], "orc", "scale", "25mm");
        tag(&state, View::Models, &["vol", "coll"], "orc", "scale", "32mm");
        assert_eq!(tags_of(&state, View::Models, &["vol", "coll"], "orc"), [tv("scale", "32mm")]);
        let all = crate::commands::all_tags_impl(&state).unwrap();
        assert!(all.contains(&tv("scale", "32mm")));
        assert!(!all.contains(&tv("scale", "25mm")));
    }

    #[test]
    fn tag_writes_check_the_node() {
        let dir = tempfile::tempdir().unwrap();
        let state = library(dir.path());
        let add = |path: &[&str], name: &str, kind: &str| {
            crate::commands::add_tag_impl(&state, View::Models, segs(path), name.into(), kind.into(), "x".into())
        };
        assert!(add(&["vol", "coll"], "gone", "user").unwrap_err().contains("no longer exists"));
        assert!(add(&[], "", "user").unwrap_err().contains("can't be tagged"));
        assert!(add(&["vol", "coll"], "orc", "edition").is_err());
        assert!(add(&["vol", "coll"], "orc", "bogus").is_err());
    }

    #[test]
    fn shadowed_folders_name_their_owner() {
        let dir = tempfile::tempdir().unwrap();
        let state = library(dir.path());
        // with a second picture `images` gets the model's folder as its own
        std::fs::write(dir.path().join("lib/Vol/Coll/Orc/orc2.png"), crate::testutil::TINY_PNG).unwrap();
        let (done, views) = crate::indexer::run_index(&state.db_path(), None, &Default::default(), &mut |_| {});
        assert!(done.ok, "{:?}", done.error);
        state.set_views(views.unwrap());
        let orc = segs(&["vol", "coll", "orc"]);
        // `images` shares the model's folder: the model's tag shows only on the model
        tag(&state, View::Models, &["vol", "coll"], "orc", "user", "Tinmen");
        assert_eq!(tags_of(&state, View::Models, &["vol", "coll"], "orc"), [tv("user", "tinmen")]);
        assert!(tags_of(&state, View::Models, &["vol", "coll", "orc"], "images").is_empty());
        let refused = "images shares its folder with orc; tag orc instead";
        let add = crate::commands::add_tag_impl(&state, View::Models, orc.clone(), "images".into(), "user".into(), "x".into());
        assert_eq!(add.unwrap_err(), refused);
        let hide = crate::commands::hide_tag_impl(&state, View::Models, orc.clone(), "images".into(), "support".into(), "x".into());
        assert_eq!(hide.unwrap_err(), refused);
        let n = crate::commands::node_tags_impl(&state, View::Models, orc.clone(), "images".into()).unwrap();
        assert_eq!(n.owner, Some(Owner { name: "orc".into(), path: segs(&["vol", "coll"]) }));
        assert!(n.own.is_empty());
        assert_eq!(n.inherited, [InheritedTag { tag: tv("user", "tinmen"), from: orc.clone() }]);
        let v = serde_json::to_value(&n).unwrap();
        assert_eq!(v["owner"], serde_json::json!({ "name": "orc", "path": ["vol", "coll"] }));
        // the owner itself has none
        let n = crate::commands::node_tags_impl(&state, View::Models, segs(&["vol", "coll"]), "orc".into()).unwrap();
        assert_eq!(n.owner, None);
    }

    #[test]
    fn search_serializes_camel_case() {
        let dir = tempfile::tempdir().unwrap();
        let state = library(dir.path());
        let v = serde_json::to_value(find(&state, View::Files, &[], "orc.stl", SearchMode::Word, false)).unwrap();
        assert_eq!(v["hits"][0]["fileKind"], "model");
        assert_eq!(v["hits"][0]["parent"], serde_json::json!(["vol", "coll", "orc"]));
        assert_eq!(v["hits"][0]["marks"], serde_json::json!([[0, 7]]));
        assert_eq!(serde_json::from_str::<SearchMode>("\"regexp\"").unwrap(), SearchMode::Regexp);
    }
}
