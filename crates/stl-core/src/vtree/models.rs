//! The Models view: `/<volume>/<collection>/<group>/…/<model>/{images, docs, <variant>/<part>/…/<file>}`.
//! Port of `prototype/stlindex/vtree.py`, with the lowercase naming rules.
use std::collections::{BTreeSet, HashMap};

use super::tags::{Tag, TagKind, exclusive, normalize, variant_tags};
use super::{NodeId, NodeKind, Tree, common_real, parent_real};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CollectionRow {
    pub volume: String,
    pub collection: String,
    /// The collection folder(s) on disk.
    pub real: Vec<String>,
    pub root_real: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GroupRow {
    pub id: i64,
    pub collection: String,
    pub parent_id: Option<i64>,
    pub name: String,
    pub path: String,
    pub poster_asset_id: Option<i64>,
    pub posters: u32,
    pub designer: Option<String>,
    pub edition: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModelRow {
    pub id: i64,
    pub collection: String,
    pub group_id: Option<i64>,
    pub name: String,
    pub raw_name: String,
    pub path: String,
    pub edition: Option<String>,
    /// The hero image, else the fallback (nearest group poster).
    pub hero_asset_id: Option<i64>,
    pub file_count: u32,
    pub image_count: u32,
    pub variants: Vec<String>,
    pub designer: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileRow {
    pub model_id: i64,
    pub entry_id: i64,
    pub name: String,
    pub kind: String,
    pub part_group: String,
    pub variant: String,
    pub real: String,
    /// `f:…` key of the entry.
    pub key: String,
}

/// A model's asset, in rank order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AssetRow {
    pub id: i64,
    pub model_id: i64,
    pub entry_id: i64,
    pub name: String,
    pub kind: String,
    pub role: String,
    pub real: String,
    pub key: String,
}

/// A password-protected archive source, shown as a padlock item in its collection folder.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LockedRow {
    pub collection: String,
    pub name: String,
    pub real: String,
    pub key: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ModelsInput {
    pub collections: Vec<CollectionRow>,
    pub groups: Vec<GroupRow>,
    pub models: Vec<ModelRow>,
    pub files: Vec<FileRow>,
    pub assets: Vec<AssetRow>,
    pub locked: Vec<LockedRow>,
}

/// Creator and edition tags from the raw designer and edition values.
fn credit_tags(designer: Option<&str>, edition: Option<&str>) -> Vec<Tag> {
    let mk = |kind, v: Option<&str>| v.and_then(normalize).map(|name| Tag { kind, name, by_user: false });
    [mk(TagKind::Creator, designer), mk(TagKind::Edition, edition)].into_iter().flatten().collect()
}

/// The support and scale tags every variant label has.
fn common_variant_tags(variants: &[String]) -> Vec<Tag> {
    let mut labels: Vec<&str> = variants.iter().map(String::as_str).collect();
    labels.sort();
    labels.dedup();
    let sets: Vec<Vec<Tag>> = labels.iter().map(|l| variant_tags(l)).collect();
    let Some((first, rest)) = sets.split_first() else { return Vec::new() };
    first.iter().filter(|t| rest.iter().all(|s| s.contains(t))).cloned().collect()
}

const DOC_KINDS: [&str; 2] = ["doc", "meta"];

/// `supported+lychee` → `supported · lychee`; `''` → `default`.
fn variant_folder(v: &str) -> String {
    if v.is_empty() { "default".into() } else { v.split('+').collect::<Vec<_>>().join(" · ") }
}

struct Builder {
    t: Tree,
    /// (parent, lowercase name, kind) → node, for get-or-create folders
    folders: HashMap<(NodeId, String, NodeKind), NodeId>,
    colls: HashMap<String, NodeId>,
}

impl Builder {
    fn folder(&mut self, parent: NodeId, name: &str, kind: NodeKind) -> NodeId {
        let k = (parent, name.to_lowercase(), kind);
        if let Some(&id) = self.folders.get(&k) {
            return id;
        }
        let id = self.t.add(parent, name, kind);
        self.folders.insert(k, id);
        id
    }

    /// The collection folder for a collection string, creating volume and collection if needed.
    fn collection(&mut self, collection: &str) -> NodeId {
        if let Some(&id) = self.colls.get(collection) {
            return id;
        }
        let (vol, name) = match collection.split_once('/') {
            Some((v, n)) => (v, n),
            None => (collection, collection),
        };
        let v = self.folder(0, vol, NodeKind::Volume);
        let c = self.folder(v, name, NodeKind::Collection);
        self.t.nodes[c].key = format!("m:{collection}/");
        self.colls.insert(collection.to_string(), c);
        c
    }
}

/// Build the Models view. With `with_files` false it stops at the models.
pub fn build_models(input: &ModelsInput, with_files: bool) -> Tree {
    let mut b = Builder { t: Tree::new(), folders: HashMap::new(), colls: HashMap::new() };
    for c in &input.collections {
        let id = b.collection(&c.collection);
        for r in &c.real {
            b.t.add_real(id, r);
        }
        let vol = b.folder(0, c.volume.as_str(), NodeKind::Volume);
        b.t.add_real(vol, &c.root_real);
    }

    // the root group of a collection is the collection folder itself
    let mut gnode: HashMap<i64, NodeId> = HashMap::new();
    for g in input.groups.iter().filter(|g| g.parent_id.is_none()) {
        let id = b.collection(&g.collection);
        gnode.insert(g.id, id);
        let info = &mut b.t.nodes[id].info;
        (info.group_id, info.poster_asset_id, info.posters) = (Some(g.id), g.poster_asset_id, g.posters);
        info.tags = exclusive(credit_tags(g.designer.as_deref(), g.edition.as_deref()));
    }
    // parents may come after their children in id order; groups whose parent never appears are skipped
    let mut pending: Vec<&GroupRow> = input.groups.iter().filter(|g| g.parent_id.is_some()).collect();
    loop {
        let before = pending.len();
        pending.retain(|g| {
            let Some(&parent) = g.parent_id.and_then(|p| gnode.get(&p)) else { return true };
            let id = b.t.add(parent, &g.name, NodeKind::Group);
            let n = &mut b.t.nodes[id];
            n.key = format!("m:{}/{}", g.collection, g.path);
            (n.info.group_id, n.info.poster_asset_id, n.info.posters) = (Some(g.id), g.poster_asset_id, g.posters);
            n.info.tags = exclusive(credit_tags(g.designer.as_deref(), g.edition.as_deref()));
            gnode.insert(g.id, id);
            false
        });
        if pending.is_empty() || pending.len() == before {
            break;
        }
    }

    let mut files: HashMap<i64, Vec<&FileRow>> = HashMap::new();
    for f in &input.files {
        files.entry(f.model_id).or_default().push(f);
    }
    let mut assets: HashMap<i64, Vec<&AssetRow>> = HashMap::new();
    for a in &input.assets {
        assets.entry(a.model_id).or_default().push(a);
    }
    for m in &input.models {
        let parent = match m.group_id {
            None => b.collection(&m.collection),
            Some(g) => match gnode.get(&g) {
                Some(&p) => p,
                None => continue,
            },
        };
        let id = b.t.add(parent, &m.name, NodeKind::Model);
        let (mf, ma) = (files.remove(&m.id).unwrap_or_default(), assets.remove(&m.id).unwrap_or_default());
        // the folder of its files; pictures attached from outside it (sidecars, group images)
        // would widen it to the collection
        let mut dirs: Vec<&str> = mf.iter().map(|f| parent_real(&f.real)).collect();
        if dirs.is_empty() {
            dirs = ma.iter().map(|a| parent_real(&a.real)).collect();
        }
        let n = &mut b.t.nodes[id];
        n.key = format!("m:{}/{}", m.collection, m.path);
        if !dirs.is_empty() {
            n.real = vec![common_real(&dirs)];
        }
        let i = &mut n.info;
        i.model_id = Some(m.id);
        i.raw_name = Some(m.raw_name.clone());
        i.edition = m.edition.clone();
        i.hero_asset_id = m.hero_asset_id;
        (i.file_count, i.image_count) = (m.file_count, m.image_count);
        i.variants = m.variants.clone();
        let mut tags = credit_tags(m.designer.as_deref(), m.edition.as_deref());
        tags.extend(common_variant_tags(&m.variants));
        i.tags = exclusive(tags);
        if with_files {
            fill_model(&mut b, id, &mf, &ma);
        }
    }

    for l in &input.locked {
        let c = b.collection(&l.collection);
        let id = b.t.add(c, &l.name, NodeKind::Locked);
        b.t.nodes[id].real = vec![l.real.clone()];
        b.t.nodes[id].key = l.key.clone();
    }

    let mut t = b.t;
    fill_real(&mut t, 0);
    t.finish(&|n| {
        n.info.edition.clone().or_else(|| n.info.raw_name.clone().filter(|r| r.to_lowercase() != n.name))
    });
    t
}

fn fill_model(b: &mut Builder, node: NodeId, files: &[&FileRow], assets: &[&AssetRow]) {
    let mkey = b.t.nodes[node].key.clone();
    let leaf = |b: &mut Builder, parent: NodeId, name: &str, kind: NodeKind, real: &str, key: &str| -> NodeId {
        let id = b.t.add(parent, name, kind);
        b.t.nodes[id].real = vec![real.to_string()];
        b.t.nodes[id].key = key.to_string();
        id
    };
    if assets.iter().any(|a| a.kind == "image") {
        let images = b.t.add(node, "images", NodeKind::Images);
        b.t.nodes[images].key = format!("{mkey}#images");
        for a in assets.iter().filter(|a| a.kind == "image") {
            let id = leaf(b, images, &a.name, NodeKind::Image, &a.real, &a.key);
            let i = &mut b.t.nodes[id].info;
            (i.asset_id, i.entry_id, i.role) = (Some(a.id), Some(a.entry_id), Some(a.role.clone()));
        }
    }
    let docs: Vec<(i64, &str, &str, &str, &str)> = files
        .iter()
        .filter(|f| DOC_KINDS.contains(&f.kind.as_str()))
        .map(|f| (f.entry_id, f.name.as_str(), f.kind.as_str(), f.real.as_str(), f.key.as_str()))
        .chain(assets.iter().filter(|a| a.kind != "image").map(|a| (a.entry_id, a.name.as_str(), a.kind.as_str(), a.real.as_str(), a.key.as_str())))
        .collect();
    if !docs.is_empty() {
        let folder = b.t.add(node, "docs", NodeKind::Docs);
        b.t.nodes[folder].key = format!("{mkey}#docs");
        let mut seen = BTreeSet::new();
        for (entry, name, kind, real, key) in docs {
            if seen.insert(entry) {
                let id = leaf(b, folder, name, NodeKind::File, real, key);
                (b.t.nodes[id].info.entry_id, b.t.nodes[id].info.file_kind) = (Some(entry), Some(kind.to_string()));
            }
        }
    }
    let printable: Vec<&&FileRow> = files.iter().filter(|f| !DOC_KINDS.contains(&f.kind.as_str())).collect();
    let split = printable.iter().map(|f| f.variant.as_str()).collect::<BTreeSet<_>>().len() > 1;
    for f in printable {
        let mut parent = node;
        if split {
            parent = b.folder(node, &variant_folder(&f.variant), NodeKind::Variant);
            b.t.nodes[parent].key = format!("{mkey}#{}", f.variant);
            b.t.nodes[parent].info.tags = exclusive(variant_tags(&f.variant));
        }
        for part in f.part_group.split('/').filter(|p| !p.is_empty()) {
            let pkey = b.t.nodes[parent].key.clone();
            parent = b.folder(parent, part, NodeKind::Part);
            b.t.nodes[parent].key = format!("{pkey}/{part}");
        }
        let id = leaf(b, parent, &f.name, NodeKind::File, &f.real, &f.key);
        (b.t.nodes[id].info.entry_id, b.t.nodes[id].info.file_kind) = (Some(f.entry_id), Some(f.kind.clone()));
        b.t.nodes[id].info.tags = exclusive(variant_tags(&f.variant));
    }
}

/// Folders without a real path of their own get the common folder of their children.
fn fill_real(t: &mut Tree, id: NodeId) {
    let kids = t.nodes[id].children.clone();
    for &c in &kids {
        fill_real(t, c);
    }
    if id != 0 && t.nodes[id].real.is_empty() {
        let reals: Vec<String> = kids.iter().filter_map(|&c| t.nodes[c].real.first().cloned()).collect();
        if !reals.is_empty() {
            let refs: Vec<&str> = reals.iter().map(String::as_str).collect();
            t.nodes[id].real = vec![common_real(&refs)];
        }
    }
}
