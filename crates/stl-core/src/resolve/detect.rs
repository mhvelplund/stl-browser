//! Model detection over the logical tree, and display names. Port of `_Resolver`,
//! `_finalize_names` and `_strip_shared_prefix` in `prototype/stlindex/resolve.py`.
use std::collections::{BTreeSet, HashMap, HashSet};

use indexmap::IndexMap;

use super::tree::{LTree, NodeId};
use super::{Entry, Group, Model, ModelFile};
use crate::names::{self, Kind};

const ROLE_EXCLUDE_DESIGNER: &[&str] = &[
    "male", "female", "leader", "banner", "musician", "champion", "sergeant", "command", "unit", "squad", "supported",
    "unsupported", "presupported",
];

/// One resolver pass: groups and models plus which logical node each one came from.
pub(crate) struct Pass<'a> {
    pub tree: &'a LTree,
    pub entries: &'a [Entry],
    pub source_name: &'a str,
    /// Group nodes to treat as one model (units, P18).
    pub force: &'a HashSet<NodeId>,
    pub groups: Vec<Group>,
    pub models: Vec<Model>,
    pub group_node: Vec<NodeId>,
    pub model_node: Vec<NodeId>,
    /// Model owning each logical node (set on the model node and its subtree).
    pub model_of: Vec<Option<usize>>,
    /// Group created for each logical node.
    pub group_of: Vec<Option<usize>>,
}

impl<'a> Pass<'a> {
    pub fn new(tree: &'a LTree, entries: &'a [Entry], source_name: &'a str, force: &'a HashSet<NodeId>) -> Self {
        let n = tree.nodes.len();
        Pass {
            tree,
            entries,
            source_name,
            force,
            groups: Vec::new(),
            models: Vec::new(),
            group_node: Vec::new(),
            model_node: Vec::new(),
            model_of: vec![None; n],
            group_of: vec![None; n],
        }
    }

    fn name_of(&self, node: NodeId) -> String {
        let n = &self.tree.nodes[node];
        if n.parent.is_some() { n.name.clone() } else { self.source_name.to_string() }
    }

    pub fn walk(&mut self, node: NodeId, chain: Option<NodeId>, group: Option<usize>) {
        let t = self.tree;
        let kids = t.structural_kids(node);
        // kit prefix shared by all siblings ("TEOTH_Heads", "TEOTH_Legs_DiscKnees") is ignored
        let shared: HashSet<String> = if kids.len() >= 3 {
            let mut sets = kids.iter().map(|&c| names::tokens(&t.nodes[c].name).into_iter().collect::<HashSet<_>>());
            let first = sets.next().unwrap_or_default();
            sets.fold(first, |acc, s| acc.intersection(&s).cloned().collect())
        } else {
            HashSet::new()
        };
        let partish = kids
            .iter()
            .filter(|&&c| {
                let name = &t.nodes[c].name;
                let rest: Vec<String> = names::tokens(name).into_iter().filter(|x| !shared.contains(x)).collect();
                let rest = rest.join(" ");
                names::is_part_group(if rest.is_empty() { name } else { &rest })
            })
            .count();
        if self.force.contains(&node) || t.has_direct(node) || (kids.len() > 1 && 2 * partish >= kids.len()) {
            self.make_model(node, chain.unwrap_or(node), group);
        } else if kids.len() == 1 {
            self.walk(kids[0], Some(chain.unwrap_or(node)), group);
        } else if !kids.is_empty() {
            let name = self.name_of(chain.unwrap_or(node));
            let g = self.groups.len();
            self.groups.push(Group { name: name.clone(), raw_name: name, path: t.path(node), parent: group, attrs: Default::default() });
            self.group_node.push(node);
            self.group_of[node] = Some(g);
            if let Some(c) = chain {
                self.group_of[c] = Some(g);
            }
            for c in kids {
                self.walk(c, None, Some(g));
            }
        }
    }

    fn make_model(&mut self, node: NodeId, name_node: NodeId, group: Option<usize>) {
        let t = self.tree;
        let raw = self.name_of(name_node);
        let mi = self.models.len();
        let subtree = t.walk(node);
        for &n in &subtree {
            self.model_of[n] = Some(mi);
        }
        let model_norm = names::normalize(&raw);
        let mut files = Vec::new();
        for &n in &subtree {
            let mut rel = Vec::new();
            let mut p = n;
            while p != node {
                let pn = &t.nodes[p];
                if !names::contained_in(&names::normalize(&pn.name), &model_norm) {
                    rel.push(pn.name.as_str());
                }
                p = pn.parent.expect("below the model node");
            }
            rel.reverse();
            let part_group = rel.join("/");
            for f in &t.nodes[n].files {
                if matches!(f.kind, Kind::Image | Kind::Junk | Kind::Meta) {
                    continue;
                }
                files.push(ModelFile {
                    entry_id: self.entries[f.entry].id,
                    name: f.name.clone(),
                    kind: f.kind,
                    part_group: part_group.clone(),
                    variant: names::variant_label(&f.variant),
                });
            }
        }
        self.models.push(Model {
            name: raw.clone(),
            raw_name: raw,
            path: t.path(node),
            group,
            attrs: Default::default(),
            files,
            assets: Vec::new(),
        });
        self.model_node.push(node);
    }
}

/// Designer tags, parsed names and attributes, then the D6 shared-prefix strip per group.
pub(crate) fn finalize_names(groups: &mut [Group], models: &mut [Model], source_name: &str) {
    let mut tags: IndexMap<String, usize> = IndexMap::new();
    for m in models.iter() {
        let uniq: BTreeSet<String> = names::parenthetical_tags(&m.raw_name).into_iter().collect();
        for t in uniq {
            // designer handles look like "VirusesofDeath", "HappyDuck3D", not "(Drone)"
            let handle = t.chars().count() >= 8 || t.chars().skip(1).any(|ch| ch.is_ascii_digit() || ch.is_uppercase());
            let first_digit = t.chars().next().is_some_and(|c| c.is_ascii_digit());
            if handle && !ROLE_EXCLUDE_DESIGNER.contains(&t.to_lowercase().as_str()) && !first_digit {
                *tags.entry(t).or_default() += 1;
            }
        }
    }
    let designer_tags: Vec<String> = tags.into_iter().filter(|(_, c)| *c >= 2).map(|(t, _)| t).collect();
    for m in models.iter_mut() {
        (m.name, m.attrs) = names::parse_name(&m.raw_name, Some(source_name), &designer_tags);
    }
    for g in groups.iter_mut() {
        (g.name, g.attrs) = names::parse_name(&g.raw_name, Some(source_name), &designer_tags);
    }
    let mut by_group: IndexMap<Option<usize>, Vec<usize>> = IndexMap::new();
    for (i, m) in models.iter().enumerate() {
        by_group.entry(m.group).or_default().push(i);
    }
    for sibs in by_group.values() {
        strip_shared_prefix(models, sibs);
    }
}

/// D6: `Warhammer 40k - Wargames Terrain - Ancient Alien Pylon` ×153 → `Ancient Alien Pylon`.
/// Strips the longest ` - `-terminated prefix shared by enough siblings (display name only).
fn strip_shared_prefix(models: &mut [Model], sibs: &[usize]) {
    if sibs.len() < 5 {
        return;
    }
    let split: Vec<Vec<String>> = sibs.iter().map(|&i| models[i].name.split(' ').map(str::to_string).collect()).collect();
    let key = |toks: &[String], k: usize| -> Vec<String> { toks[..=k].iter().map(|x| x.to_lowercase()).collect() };
    let mut counts: HashMap<Vec<String>, usize> = HashMap::new();
    for toks in &split {
        for k in 0..toks.len().saturating_sub(1) {
            if toks[k] == "-" {
                *counts.entry(key(toks, k)).or_default() += 1;
            }
        }
    }
    let need = 3.max((sibs.len() * 4).div_ceil(5)); // >= 80% of the siblings
    for (&i, toks) in sibs.iter().zip(&split) {
        let mut best = 0;
        for k in 0..toks.len().saturating_sub(1) {
            if toks[k] == "-" && counts.get(&key(toks, k)).copied().unwrap_or(0) >= need {
                best = k + 1;
            }
        }
        if best > 0 {
            let rest = toks[best..].join(" ");
            let rest = rest.trim_matches([' ', '-']);
            if !rest.is_empty() {
                models[i].name = rest.to_string();
            }
        }
    }
}
