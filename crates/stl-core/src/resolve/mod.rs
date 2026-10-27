//! Turn the flat file list of one collection (archives treated as folders) into groups, models,
//! model files and assets. Port of `prototype/stlindex/resolve.py`; see `docs/archive-patterns.md`.
//!
//! Pipeline: physical tree → variant marking (incl. sibling rule) → logical tree (variant folders
//! made transparent, image-only folders hoisted) → model detection (wrappers collapsed, part kits
//! recognised) → asset attachment → unit collapse (P18, a second pass).
mod detect;
mod tree;

use std::collections::{BTreeSet, HashSet};

use crate::names::{Attrs, Kind};
use detect::Pass;
use tree::LTree;

/// An unexpanded archive stands in for models.
pub(crate) const STRUCTURAL_KINDS: [Kind; 3] = [Kind::Model, Kind::Scene, Kind::Archive];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    pub id: i64,
    /// `/`-separated; nested archives appear as a path segment (`a/x.zip/b.stl`).
    pub path: String,
    pub size: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Group {
    pub name: String,
    pub raw_name: String,
    pub path: String,
    pub parent: Option<usize>,
    pub attrs: Attrs,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Model {
    pub name: String,
    pub raw_name: String,
    pub path: String,
    pub group: Option<usize>,
    pub attrs: Attrs,
    pub files: Vec<ModelFile>,
    /// Indices into [`Resolution::assets`], in rank order.
    pub assets: Vec<usize>,
}

impl Model {
    /// Sorted unique variants of the model and scene files.
    pub fn variants(&self) -> Vec<String> {
        let v: BTreeSet<&String> =
            self.files.iter().filter(|f| matches!(f.kind, Kind::Model | Kind::Scene)).map(|f| &f.variant).collect();
        v.into_iter().cloned().collect()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModelFile {
    pub entry_id: i64,
    pub name: String,
    pub kind: Kind,
    pub part_group: String,
    pub variant: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AssetRole {
    Preview,
    Thumb,
    Instructions,
    Doc,
    Poster,
}

impl AssetRole {
    pub fn as_str(self) -> &'static str {
        match self {
            AssetRole::Preview => "preview",
            AssetRole::Thumb => "thumb",
            AssetRole::Instructions => "instructions",
            AssetRole::Doc => "doc",
            AssetRole::Poster => "poster",
        }
    }
}

/// How an asset was attached.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Match {
    Inside,
    Thumb,
    Sidecar,
    Only,
    Id,
    Name,
    Poster,
}

impl Match {
    pub fn as_str(self) -> &'static str {
        match self {
            Match::Inside => "inside",
            Match::Thumb => "thumb",
            Match::Sidecar => "sidecar",
            Match::Only => "only",
            Match::Id => "id",
            Match::Name => "name",
            Match::Poster => "poster",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Asset {
    pub entry_id: i64,
    pub entry_size: u64,
    pub name: String,
    /// `Kind::Image` or `Kind::Doc`.
    pub kind: Kind,
    pub role: AssetRole,
    pub match_: Match,
    pub model: Option<usize>,
    pub group: Option<usize>,
    pub generic: bool,
    pub depth: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Resolution {
    pub source_name: String,
    pub groups: Vec<Group>,
    pub models: Vec<Model>,
    /// All assets, including group and collection posters.
    pub assets: Vec<Asset>,
}

impl Resolution {
    /// The model's first asset, if it is an image.
    pub fn hero(&self, model: usize) -> Option<&Asset> {
        self.models[model].assets.first().map(|&a| &self.assets[a]).filter(|a| a.kind == Kind::Image)
    }

    pub fn model_assets(&self, model: usize) -> Vec<&Asset> {
        self.models[model].assets.iter().map(|&a| &self.assets[a]).collect()
    }
}

/// Resolve one collection. `source_name` is the collection folder's name.
pub fn resolve(source_name: &str, entries: &[Entry]) -> Resolution {
    let mut p = tree::build_physical(entries);
    tree::mark_variants(&mut p, 0);
    let mut l = LTree::new(source_name);
    tree::build_logical(&p, 0, &mut l, 0, &BTreeSet::new(), &[source_name.to_string()]);
    tree::mark_logical_structural(&mut l, 0);
    resolve_once(source_name, &l, entries, &HashSet::new())
}

fn resolve_once(source_name: &str, l: &LTree, entries: &[Entry], force: &HashSet<usize>) -> Resolution {
    let mut pass = Pass::new(l, entries, source_name, force);
    if l.nodes[0].structural {
        pass.walk(0, None, None);
    }
    detect::finalize_names(&mut pass.groups, &mut pass.models, source_name);
    Resolution { source_name: source_name.to_string(), groups: pass.groups, models: pass.models, assets: Vec::new() }
}
