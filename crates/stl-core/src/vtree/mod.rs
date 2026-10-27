//! The virtual folder tree shown by both views, and the naming rules they share: lowercase
//! names, `_N` postfixes for clashes, hover (real) paths with `!` at archive boundaries.
//! See `docs/virtual-tree.md`.
use indexmap::IndexMap;

use crate::names;

pub type NodeId = usize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum NodeKind {
    Root,
    Volume,
    Collection,
    Group,
    Model,
    Images,
    Docs,
    Variant,
    Part,
    Folder,
    Archive,
    File,
    Image,
    /// A password-protected archive: one item with a padlock, never opened.
    Locked,
}

/// Leaf kinds: a clash postfix goes before their extension, and they sort after folders.
pub fn is_file_kind(kind: NodeKind) -> bool {
    matches!(kind, NodeKind::File | NodeKind::Image | NodeKind::Locked)
}

/// What a node refers to in the index; which fields are set depends on the kind.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Info {
    pub entry_id: Option<i64>,
    pub file_kind: Option<String>,
    pub model_id: Option<i64>,
    pub group_id: Option<i64>,
    pub asset_id: Option<i64>,
    pub hero_asset_id: Option<i64>,
    pub poster_asset_id: Option<i64>,
    pub posters: u32,
    pub file_count: u32,
    pub image_count: u32,
    pub variants: Vec<String>,
    pub edition: Option<String>,
    pub raw_name: Option<String>,
    pub role: Option<String>,
    /// Archive format (`zip`, `rar`, `7z`) for archive folders and locked items.
    pub badge: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Node {
    /// Display name, lowercase.
    pub name: String,
    pub kind: NodeKind,
    pub children: Vec<NodeId>,
    /// Real paths for hovering; more than one for merged folders.
    pub real: Vec<String>,
    /// Stable user-data key (`m:…` / `f:…`).
    pub key: String,
    pub info: Info,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Tree {
    /// `nodes[0]` is the root.
    pub nodes: Vec<Node>,
}

impl Default for Tree {
    fn default() -> Self {
        Self::new()
    }
}

impl Tree {
    pub fn new() -> Self {
        Tree {
            nodes: vec![Node {
                name: String::new(),
                kind: NodeKind::Root,
                children: Vec::new(),
                real: Vec::new(),
                key: String::new(),
                info: Info::default(),
            }],
        }
    }

    /// Append a child; the name is stored lowercased.
    pub fn add(&mut self, parent: NodeId, name: &str, kind: NodeKind) -> NodeId {
        let id = self.nodes.len();
        self.nodes.push(Node {
            name: name.to_lowercase(),
            kind,
            children: Vec::new(),
            real: Vec::new(),
            key: String::new(),
            info: Info::default(),
        });
        self.nodes[parent].children.push(id);
        id
    }

    /// The node at a `/`-separated path from the root, case-insensitive.
    pub fn find(&self, path: &str) -> Option<NodeId> {
        let mut node = 0;
        for part in path.split('/').filter(|p| !p.is_empty()) {
            let part = part.to_lowercase();
            node = *self.nodes[node].children.iter().find(|&&c| self.nodes[c].name == part)?;
        }
        Some(node)
    }

    /// Add `real` to the node's real paths unless it is already there.
    pub fn add_real(&mut self, id: NodeId, real: &str) {
        if !self.nodes[id].real.iter().any(|r| r == real) {
            self.nodes[id].real.push(real.to_string());
        }
    }

    /// Apply the naming rules and the sort order to every node's children. `alt` gives the
    /// bracketed suffix for clashing models (edition, else raw name).
    pub fn finish(&mut self, alt: &dyn Fn(&Node) -> Option<String>) {
        let mut stack = vec![0];
        while let Some(id) = stack.pop() {
            self.finish_children(id, alt);
            stack.extend(self.nodes[id].children.iter().copied());
        }
    }

    /// Dedupe and sort one node's children.
    pub fn finish_children(&mut self, id: NodeId, alt: &dyn Fn(&Node) -> Option<String>) {
        let kids = self.nodes[id].children.clone();
        let clashes = self.clashes(&kids);
        for clash in &clashes {
            if clash.iter().any(|&c| self.nodes[c].kind == NodeKind::Model) {
                for &c in clash {
                    if let Some(extra) = alt(&self.nodes[c]) {
                        let n = &mut self.nodes[c];
                        n.name = format!("{} ({})", n.name, extra.to_lowercase());
                    }
                }
            }
        }
        for mut clash in self.clashes(&kids) {
            // stable order: by first real path, then insertion order
            clash.sort_by(|&a, &b| self.nodes[a].real.first().cmp(&self.nodes[b].real.first()));
            for (i, &c) in clash.iter().enumerate() {
                let n = &mut self.nodes[c];
                n.name = numbered(&n.name, i + 1, is_file_kind(n.kind));
            }
        }
        let nodes = &self.nodes;
        let mut kids = kids;
        kids.sort_by(|&a, &b| {
            let key = |n: &Node| {
                let order = match n.kind {
                    NodeKind::Images => 0,
                    NodeKind::Docs => 1,
                    _ => 2,
                };
                (order, is_file_kind(n.kind))
            };
            let (na, nb) = (&nodes[a], &nodes[b]);
            key(na).cmp(&key(nb)).then_with(|| na.name.cmp(&nb.name))
        });
        self.nodes[id].children = kids;
    }

    /// Groups of children sharing a name (only groups of two or more), in first-seen order.
    fn clashes(&self, kids: &[NodeId]) -> Vec<Vec<NodeId>> {
        let mut by_name: IndexMap<&str, Vec<NodeId>> = IndexMap::new();
        for &c in kids {
            by_name.entry(self.nodes[c].name.as_str()).or_default().push(c);
        }
        by_name.into_values().filter(|v| v.len() > 1).collect()
    }
}

/// `orc.stl` → `orc_2.stl` for files, `orc` → `orc_2` for folders.
fn numbered(name: &str, n: usize, file: bool) -> String {
    if file {
        let stem = names::stem(name);
        format!("{stem}_{n}{}", &name[stem.len()..])
    } else {
        format!("{name}_{n}")
    }
}

/// Hover path of an archive entry: `D:\x\a.zip!dir/b.zip!c.stl`.
pub fn archive_real(source_path: &str, chain: &[String]) -> String {
    let mut out = source_path.to_string();
    for member in chain {
        out.push('!');
        out.push_str(&member.replace('\\', "/"));
    }
    out
}

/// Hover path of a loose file below a folder: `D:\x\Coll` + `sub/a.stl` → `D:\x\Coll\sub\a.stl`.
pub fn folder_real(source_path: &str, rel: &str) -> String {
    if rel.is_empty() {
        return source_path.to_string();
    }
    format!("{}\\{}", source_path.trim_end_matches('\\'), rel.replace('/', "\\"))
}

fn is_sep(c: char) -> bool {
    matches!(c, '\\' | '/' | '!')
}

/// Strip the last component (a `\`, `/` or `!` boundary).
pub fn parent_real(path: &str) -> &str {
    path.rfind(is_sep).map_or("", |i| &path[..i])
}

/// The longest common prefix of the paths at component boundaries.
pub fn common_real(paths: &[&str]) -> String {
    let Some((first, rest)) = paths.split_first() else {
        return String::new();
    };
    // the byte offsets where each component of `first` ends
    let mut ends: Vec<usize> = first.char_indices().filter(|&(_, c)| is_sep(c)).map(|(i, _)| i).collect();
    ends.push(first.len());
    let mut best = first.len();
    for p in rest {
        best = ends
            .iter()
            .copied()
            .filter(|&e| e <= best && p.starts_with(&first[..e]) && (p.len() == e || p[e..].starts_with(is_sep)))
            .max()
            .unwrap_or(0);
    }
    first[..best].to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tree_with(names: &[(&str, NodeKind, &str)]) -> Tree {
        let mut t = Tree::new();
        for (name, kind, real) in names {
            let id = t.add(0, name, *kind);
            t.nodes[id].real = vec![real.to_string()];
        }
        t
    }

    fn child_names(t: &Tree, id: NodeId) -> Vec<String> {
        t.nodes[id].children.iter().map(|&c| t.nodes[c].name.clone()).collect()
    }

    #[test]
    fn add_lowercases() {
        let mut t = Tree::new();
        let id = t.add(0, "Orc Band", NodeKind::Folder);
        assert_eq!(t.nodes[id].name, "orc band");
    }

    #[test]
    fn find_is_case_insensitive() {
        let mut t = Tree::new();
        let a = t.add(0, "Vol", NodeKind::Volume);
        let b = t.add(a, "Coll", NodeKind::Collection);
        assert_eq!(t.find("VOL/coll/"), Some(b));
        assert_eq!(t.find(""), Some(0));
        assert_eq!(t.find("vol/nope"), None);
    }

    #[test]
    fn file_clash_gets_numbered_before_extension() {
        let mut t = tree_with(&[("ORC.STL", NodeKind::File, "b"), ("Orc.stl", NodeKind::File, "a")]);
        t.finish(&|_| None);
        assert_eq!(child_names(&t, 0), ["orc_1.stl", "orc_2.stl"]);
        assert_eq!(t.nodes[t.find("orc_1.stl").unwrap()].real, ["a"]);
    }

    #[test]
    fn folder_clash_numbered_at_end() {
        let mut t = tree_with(&[("x", NodeKind::Archive, "D:\\x.zip"), ("X", NodeKind::Folder, "D:\\X")]);
        t.finish(&|_| None);
        assert_eq!(t.nodes[t.find("x_1").unwrap()].real, ["D:\\X"]);
        assert_eq!(t.nodes[t.find("x_2").unwrap()].real, ["D:\\x.zip"]);
    }

    #[test]
    fn model_clash_uses_alt_then_numbers() {
        let mut t = tree_with(&[
            ("Zarvos", NodeKind::Model, "a"),
            ("Zarvos", NodeKind::Model, "b"),
            ("Orc", NodeKind::Model, "d"),
            ("Orc", NodeKind::Model, "c"),
        ]);
        let ids = t.nodes[0].children.clone();
        t.nodes[ids[0]].info.edition = Some("5E".into());
        t.nodes[ids[1]].info.edition = Some("8E".into());
        t.finish(&|n| n.info.edition.clone());
        assert_eq!(child_names(&t, 0), ["orc_1", "orc_2", "zarvos (5e)", "zarvos (8e)"]);
        assert_eq!(t.nodes[t.find("orc_1").unwrap()].real, ["c"]);
    }

    #[test]
    fn sort_order() {
        let mut t = tree_with(&[
            ("b.stl", NodeKind::File, ""),
            ("a", NodeKind::Variant, ""),
            ("docs", NodeKind::Docs, ""),
            ("a.png", NodeKind::Image, ""),
            ("images", NodeKind::Images, ""),
            ("c", NodeKind::Part, ""),
        ]);
        t.finish(&|_| None);
        assert_eq!(child_names(&t, 0), ["images", "docs", "a", "c", "a.png", "b.stl"]);
    }

    #[test]
    fn archive_real_format() {
        let chain = ["Grimlax/842.zip".to_string(), "S\\body.stl".to_string()];
        assert_eq!(archive_real("D:\\V\\G.rar", &chain), "D:\\V\\G.rar!Grimlax/842.zip!S/body.stl");
    }

    #[test]
    fn folder_real_format() {
        assert_eq!(folder_real("D:\\x\\Coll", "sub/a.stl"), "D:\\x\\Coll\\sub\\a.stl");
        assert_eq!(folder_real("D:\\", "a.stl"), "D:\\a.stl");
        assert_eq!(folder_real("D:\\x", ""), "D:\\x");
    }

    #[test]
    fn common_real_cases() {
        assert_eq!(common_real(&["D:\\a\\x.zip!d/e", "D:\\a\\x.zip!d/f"]), "D:\\a\\x.zip!d");
        assert_eq!(common_real(&["D:\\a\\b", "D:\\a\\c"]), "D:\\a");
        assert_eq!(common_real(&["D:\\a\\b"]), "D:\\a\\b");
        assert_eq!(common_real(&["D:\\ab", "D:\\ac"]), "D:");
        assert_eq!(common_real(&["D:\\a\\x.zip!b", "D:\\a\\x.zip"]), "D:\\a\\x.zip");
        assert_eq!(common_real(&[]), "");
    }

    #[test]
    fn parent_real_cases() {
        assert_eq!(parent_real("D:\\a\\x.zip!c.stl"), "D:\\a\\x.zip");
        assert_eq!(parent_real("D:\\a\\x.zip!d/c.stl"), "D:\\a\\x.zip!d");
        assert_eq!(parent_real("D:\\a\\c.stl"), "D:\\a");
    }
}
