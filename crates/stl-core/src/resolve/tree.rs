//! The physical tree (paths as listed, nested archives expanded in place) and the logical tree
//! (variant folders made transparent, image-only folders hoisted). Port of the tree half of
//! `prototype/stlindex/resolve.py`.
use std::collections::{BTreeSet, HashSet};

use indexmap::IndexMap;

use super::{Entry, STRUCTURAL_KINDS};
use crate::names::{self, Kind};

pub(crate) type NodeId = usize;

// ---------------------------------------------------------------- physical tree

pub(crate) struct PDir {
    pub name: String,
    pub children: IndexMap<String, NodeId>,
    /// (index into the entries slice, file name)
    pub files: Vec<(usize, String)>,
    pub structural: bool,
    /// (label, prefix)
    pub variant: Option<(String, String)>,
}

pub(crate) struct PTree {
    pub dirs: Vec<PDir>,
}

impl PTree {
    fn child(&mut self, parent: NodeId, seg: &str) -> NodeId {
        if let Some(&id) = self.dirs[parent].children.get(seg) {
            return id;
        }
        let id = self.dirs.len();
        self.dirs.push(PDir { name: seg.to_string(), children: IndexMap::new(), files: Vec::new(), structural: false, variant: None });
        self.dirs[parent].children.insert(seg.to_string(), id);
        id
    }
}

pub(crate) fn build_physical(entries: &[Entry]) -> PTree {
    let mut t = PTree {
        dirs: vec![PDir { name: String::new(), children: IndexMap::new(), files: Vec::new(), structural: false, variant: None }],
    };
    let mut expanded: HashSet<(NodeId, String)> = HashSet::new();
    for (ei, e) in entries.iter().enumerate() {
        let parts: Vec<&str> = e.path.split('/').filter(|p| !p.is_empty()).collect();
        let Some((last, dirs)) = parts.split_last() else { continue };
        let mut node = 0;
        for seg in dirs {
            let mut seg = seg.to_string();
            if names::kind_of(&seg) == Kind::Archive {
                // nested archive expanded in place: "x.zip/" -> "x/"
                expanded.insert((node, seg.to_lowercase()));
                seg = names::stem(&seg).to_string();
            }
            let seg = names::clean_segment(&seg);
            node = t.child(node, &seg);
        }
        t.dirs[node].files.push((ei, last.to_string()));
    }
    // the archive-file entries whose contents are already present as a folder
    for (id, d) in t.dirs.iter_mut().enumerate() {
        d.files.retain(|(_, f)| !expanded.contains(&(id, f.to_lowercase())));
    }
    mark_structural(&mut t, 0);
    t
}

fn mark_structural(t: &mut PTree, id: NodeId) -> bool {
    let mut s = t.dirs[id].files.iter().any(|(_, f)| STRUCTURAL_KINDS.contains(&names::kind_of(f)));
    let kids: Vec<NodeId> = t.dirs[id].children.values().copied().collect();
    for c in kids {
        s = mark_structural(t, c) || s;
    }
    t.dirs[id].structural = s;
    s
}

pub(crate) fn mark_variants(t: &mut PTree, id: NodeId) {
    let kids: Vec<NodeId> = t.dirs[id].children.values().copied().collect();
    for &c in &kids {
        let d = &mut t.dirs[c];
        d.variant = if d.structural { names::variant_of(&d.name) } else { None };
    }
    let prefixes: HashSet<String> =
        kids.iter().filter_map(|&c| t.dirs[c].variant.as_ref()).filter(|v| !v.1.is_empty()).map(|v| v.1.clone()).collect();
    let hollow = kids.iter().any(|&c| t.dirs[c].variant.as_ref().is_some_and(|v| v.0.contains("hollow")));
    for &c in &kids {
        let d = &mut t.dirs[c];
        if !d.structural || d.variant.is_some() {
            continue;
        }
        let norm = names::normalize(&d.name);
        if prefixes.contains(&norm) {
            // sibling rule: "Command Force" next to "Command Force Supported" is the unsupported variant
            d.variant = Some(("unsupported".into(), norm));
        } else if hollow && (norm == "normal" || norm == "full") {
            d.variant = Some(("solid".into(), String::new())); // "Normal" next to "Hueco" (hollow)
        }
    }
    for c in kids {
        mark_variants(t, c);
    }
}

// ---------------------------------------------------------------- logical tree

#[derive(Clone)]
pub(crate) struct LFile {
    /// Index into the entries slice.
    pub entry: usize,
    pub name: String,
    pub kind: Kind,
    pub variant: BTreeSet<String>,
}

pub(crate) struct LNode {
    pub name: String,
    pub parent: Option<NodeId>,
    pub children: IndexMap<String, NodeId>,
    pub files: Vec<LFile>,
    pub structural: bool,
}

pub(crate) struct LTree {
    pub nodes: Vec<LNode>,
}

impl LTree {
    pub fn new(root_name: &str) -> Self {
        LTree {
            nodes: vec![LNode { name: root_name.to_string(), parent: None, children: IndexMap::new(), files: Vec::new(), structural: false }],
        }
    }

    fn child(&mut self, parent: NodeId, key: &str, display: &str) -> NodeId {
        if let Some(&id) = self.nodes[parent].children.get(key) {
            return id;
        }
        let id = self.nodes.len();
        self.nodes.push(LNode { name: display.to_string(), parent: Some(parent), children: IndexMap::new(), files: Vec::new(), structural: false });
        self.nodes[parent].children.insert(key.to_string(), id);
        id
    }

    /// Pre-order, children in insertion order.
    pub fn walk(&self, id: NodeId) -> Vec<NodeId> {
        let mut out = Vec::new();
        let mut stack = vec![id];
        while let Some(n) = stack.pop() {
            out.push(n);
            stack.extend(self.nodes[n].children.values().rev().copied());
        }
        out
    }

    pub fn path(&self, id: NodeId) -> String {
        let mut out = Vec::new();
        let mut n = id;
        while let Some(p) = self.nodes[n].parent {
            out.push(self.nodes[n].name.as_str());
            n = p;
        }
        out.reverse();
        out.join("/")
    }

    pub fn structural_kids(&self, id: NodeId) -> Vec<NodeId> {
        self.nodes[id].children.values().copied().filter(|&c| self.nodes[c].structural).collect()
    }

    pub fn has_direct(&self, id: NodeId) -> bool {
        self.nodes[id].files.iter().any(|f| STRUCTURAL_KINDS.contains(&f.kind))
    }
}

/// The longest leading part of the original name whose normalized form equals `prefix`.
fn display_prefix(name: &str, prefix: &str) -> String {
    let mut best: Option<&str> = None;
    for (i, c) in name.char_indices() {
        let head = &name[..i + c.len_utf8()];
        if names::normalize(head) == prefix {
            best = Some(head);
        }
    }
    best.unwrap_or(prefix).trim_matches([' ', '_', '-', '.']).to_string()
}

fn add(labels: &BTreeSet<String>, label: &str) -> BTreeSet<String> {
    labels.iter().cloned().chain(names::variant_atoms(label)).collect()
}

fn push_files(p: &PTree, pdir: NodeId, l: &mut LTree, lnode: NodeId, labels: &BTreeSet<String>) {
    for (e, f) in &p.dirs[pdir].files {
        l.nodes[lnode].files.push(LFile { entry: *e, name: f.clone(), kind: names::kind_of(f), variant: labels.clone() });
    }
}

fn hoist(p: &PTree, pdir: NodeId, l: &mut LTree, lnode: NodeId, labels: &BTreeSet<String>) {
    push_files(p, pdir, l, lnode, labels);
    for &c in p.dirs[pdir].children.values() {
        hoist(p, c, l, lnode, labels);
    }
}

fn tail<T>(v: &[T], n: usize) -> &[T] {
    &v[v.len().saturating_sub(n)..]
}

pub(crate) fn build_logical(p: &PTree, pdir: NodeId, l: &mut LTree, lnode: NodeId, labels: &BTreeSet<String>, ancestors: &[String]) {
    push_files(p, pdir, l, lnode, labels);
    let kids: Vec<NodeId> = p.dirs[pdir].children.values().copied().collect();
    let only_variant_siblings = kids.iter().filter(|&&c| p.dirs[c].structural && p.dirs[c].variant.is_none()).count() == 1;
    let with = |name: &str| -> Vec<String> { ancestors.iter().cloned().chain([name.to_string()]).collect() };
    for c in kids {
        let d = &p.dirs[c];
        if !d.structural {
            // Renders/, Poster/, images/ ...
            hoist(p, c, l, lnode, labels);
            continue;
        }
        let norm = names::normalize(&d.name);
        if let Some((label, prefix)) = &d.variant {
            let labels = add(labels, label);
            if tail(ancestors, 4).iter().any(|a| names::contained_in(prefix, a)) {
                build_logical(p, c, l, lnode, &labels, ancestors);
            } else {
                // "569 X presupported": model folder + variant
                let target = l.child(lnode, prefix, &display_prefix(&d.name, prefix));
                build_logical(p, c, l, target, &labels, &with(&d.name));
            }
        } else if only_variant_siblings && !norm.is_empty() && tail(ancestors, 2).iter().any(|a| names::contained_in(&norm, a)) {
            // "578-590_Hoplites of Light/Hoplites of Light/" next to "presupported/": a repeat of
            // the parent name is just the default variant's container
            build_logical(p, c, l, lnode, labels, ancestors);
        } else {
            let key = if norm.is_empty() { d.name.clone() } else { norm };
            let target = l.child(lnode, &key, &d.name);
            build_logical(p, c, l, target, labels, &with(&d.name));
        }
    }
}

pub(crate) fn mark_logical_structural(l: &mut LTree, id: NodeId) -> bool {
    let mut s = l.has_direct(id);
    let kids: Vec<NodeId> = l.nodes[id].children.values().copied().collect();
    for c in kids {
        s = mark_logical_structural(l, c) || s;
    }
    l.nodes[id].structural = s;
    s
}
