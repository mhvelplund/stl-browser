//! Export plans: the folders and files that a drag-out or Extract to… writes for some nodes of
//! a view, with relative paths that mirror the view and names Windows can create.
use std::collections::HashSet;

use super::{Node, NodeId, NodeKind, Tree, is_file_kind, numbered};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExportKind {
    Dir,
    File { entry_id: i64 },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExportItem {
    /// `/`-separated, every segment passed through `safe_name`.
    pub rel: String,
    pub kind: ExportKind,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ExportPlan {
    /// Depth-first in tree order; a folder comes before its contents.
    pub items: Vec<ExportItem>,
    /// Locked archives and files without an entry: never written.
    pub skipped: u32,
}

/// The plan for `roots` (taken as they are) and everything below them that `include` accepts.
/// The tree is a DAG (inner-root volumes share children), so a shared subtree is written once
/// per parent.
pub fn export_plan(t: &Tree, roots: &[NodeId], include: &dyn Fn(&Node) -> bool) -> ExportPlan {
    let mut plan = ExportPlan::default();
    let mut stack: Vec<(NodeId, String)> = Vec::new();
    push_named(t, roots, "", &mut stack, &mut plan);
    while let Some((id, rel)) = stack.pop() {
        let n = &t.nodes[id];
        if is_file_kind(n.kind) {
            plan.items.push(ExportItem { rel, kind: ExportKind::File { entry_id: n.info.entry_id.expect("checked in push_named") } });
            continue;
        }
        plan.items.push(ExportItem { rel: rel.clone(), kind: ExportKind::Dir });
        let kids: Vec<NodeId> = n.children.iter().copied().filter(|&c| include(&t.nodes[c])).collect();
        push_named(t, &kids, &format!("{rel}/"), &mut stack, &mut plan);
    }
    plan
}

/// Push siblings (in reverse, so they pop in order) under safe, unique names; count the ones
/// that can't be written.
fn push_named(t: &Tree, ids: &[NodeId], prefix: &str, stack: &mut Vec<(NodeId, String)>, plan: &mut ExportPlan) {
    let mut taken = HashSet::new();
    let mut named = Vec::new();
    for &id in ids {
        let n = &t.nodes[id];
        if n.kind == NodeKind::Locked || (is_file_kind(n.kind) && n.info.entry_id.is_none()) {
            plan.skipped += 1;
            continue;
        }
        let base = safe_name(&n.name);
        let mut name = base.clone();
        let mut k = 1;
        while !taken.insert(name.to_lowercase()) {
            k += 1;
            name = numbered(&base, k, is_file_kind(n.kind));
        }
        named.push((id, format!("{prefix}{name}")));
    }
    stack.extend(named.into_iter().rev());
}

const DEVICES: [&str; 22] = [
    "con", "prn", "aux", "nul", "com1", "com2", "com3", "com4", "com5", "com6", "com7", "com8", "com9", "lpt1", "lpt2",
    "lpt3", "lpt4", "lpt5", "lpt6", "lpt7", "lpt8", "lpt9",
];

/// A name Windows can create: reserved characters and control characters become `_`, trailing
/// dots and spaces go, and DOS device names (`con`, `com1.stl`…) get a leading `_`.
pub fn safe_name(name: &str) -> String {
    let mut s: String =
        name.chars().map(|c| if c < ' ' || matches!(c, '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*') { '_' } else { c }).collect();
    s.truncate(s.trim_end_matches(['.', ' ']).len());
    if s.is_empty() {
        return "_".into();
    }
    let stem = s.split('.').next().unwrap_or("").trim_end().to_lowercase();
    if DEVICES.contains(&stem.as_str()) {
        s.insert(0, '_');
    }
    s
}
