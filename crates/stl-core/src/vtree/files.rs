//! The Files view: the real disk and archive layout per root. An archive is a folder named after
//! its stem with a format badge; nested archives work the same way; password-protected archives
//! are single padlock items.
use std::collections::{HashMap, HashSet};

use super::{NodeId, NodeKind, Tree, archive_real, folder_real};
use crate::names;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RootRow {
    pub id: i64,
    /// The volume name.
    pub name: String,
    pub path: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceRow {
    pub id: i64,
    pub root_id: i64,
    /// `archive` or `folder`.
    pub kind: String,
    pub path: String,
    /// `<volume>/<path relative to the root>`.
    pub rel_path: String,
    pub locked: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EntryRow {
    pub id: i64,
    pub source_id: i64,
    pub path: String,
    pub chain: Vec<String>,
    pub locked: bool,
}

fn badge(file_name: &str) -> String {
    names::ext(file_name).trim_start_matches('.').to_string()
}

struct Builder {
    t: Tree,
    /// (parent, merge key, kind) → node. The merge key is the lowercase real segment, so case
    /// variants of one folder merge while `x/`, `x.zip` and `x.rar` stay apart.
    folders: HashMap<(NodeId, String, NodeKind), NodeId>,
}

impl Builder {
    fn folder(&mut self, parent: NodeId, segment: &str, display: &str, kind: NodeKind, real: &str, key: &str) -> NodeId {
        let k = (parent, segment.to_lowercase(), kind);
        let id = match self.folders.get(&k) {
            Some(&id) => id,
            None => {
                let id = self.t.add(parent, display, kind);
                self.t.nodes[id].key = key.to_string();
                self.folders.insert(k, id);
                id
            }
        };
        self.t.add_real(id, real);
        id
    }

    fn leaf(&mut self, parent: NodeId, display: &str, kind: NodeKind, real: String, key: String) -> NodeId {
        let id = self.t.add(parent, display, kind);
        self.t.nodes[id].real = vec![real];
        self.t.nodes[id].key = key;
        id
    }

    /// The disk folder `rel` (segments below the root), creating each level.
    fn disk_folder(&mut self, vol: NodeId, root_path: &str, rel: &[&str]) -> NodeId {
        let mut node = vol;
        for i in 0..rel.len() {
            let real = folder_real(root_path, &rel[..=i].join("/"));
            node = self.folder(node, rel[i], rel[i], NodeKind::Folder, &real, &format!("f:{real}|"));
        }
        node
    }
}

/// Real path of the folder made by the first `n` path segments of an archive entry, cut from
/// its chain (each chain member covers some of the segments).
fn archive_prefix_real(source_path: &str, chain: &[String], n: usize) -> String {
    let members: Vec<Vec<String>> =
        chain.iter().map(|m| m.replace('\\', "/").split('/').filter(|s| !s.is_empty()).map(str::to_string).collect()).collect();
    let mut out = source_path.to_string();
    let mut left = n;
    for m in &members {
        let take = left.min(m.len());
        out.push('!');
        out.push_str(&m[..take].join("/"));
        left -= take;
        if left == 0 {
            break;
        }
    }
    out
}

/// Build the Files view: one volume per root that owns sources.
pub fn build_files(roots: &[RootRow], sources: &[SourceRow], entries: &[EntryRow]) -> Tree {
    let mut b = Builder { t: Tree::new(), folders: HashMap::new() };
    let mut by_source: HashMap<i64, Vec<&EntryRow>> = HashMap::new();
    for e in entries {
        by_source.entry(e.source_id).or_default().push(e);
    }
    let roots: HashMap<i64, &RootRow> = roots.iter().map(|r| (r.id, r)).collect();
    for src in sources {
        let Some(root) = roots.get(&src.root_id) else { continue };
        let vol = b.folder(0, &root.path, &root.name, NodeKind::Volume, &root.path, &format!("f:{}|", root.path));
        let rel: Vec<&str> = src.rel_path.split('/').skip(1).filter(|s| !s.is_empty()).collect();
        let entries = by_source.remove(&src.id).unwrap_or_default();
        if src.kind == "folder" {
            let dir = b.disk_folder(vol, &root.path, &rel);
            add_loose(&mut b, dir, src, &entries);
            continue;
        }
        let Some((file_name, dirs)) = rel.split_last() else { continue };
        let parent = b.disk_folder(vol, &root.path, dirs);
        let key = format!("f:{}|", src.path);
        if src.locked {
            let id = b.leaf(parent, &names::archive_stem(file_name), NodeKind::Locked, src.path.clone(), key);
            b.t.nodes[id].info.badge = Some(badge(file_name));
            continue;
        }
        let node = b.folder(parent, file_name, &names::archive_stem(file_name), NodeKind::Archive, &src.path, &key);
        b.t.nodes[node].info.badge = Some(badge(file_name));
        add_archive_entries(&mut b, node, src, &entries);
    }
    b.t.finish(&|_| None);
    b.t
}

fn add_loose(b: &mut Builder, dir: NodeId, src: &SourceRow, entries: &[&EntryRow]) {
    for e in entries {
        let segs: Vec<&str> = e.path.split('/').filter(|s| !s.is_empty()).collect();
        let Some((name, dirs)) = segs.split_last() else { continue };
        let mut node = dir;
        for i in 0..dirs.len() {
            let real = folder_real(&src.path, &dirs[..=i].join("/"));
            node = b.folder(node, dirs[i], dirs[i], NodeKind::Folder, &real, &format!("f:{real}|"));
        }
        let id = b.leaf(node, name, NodeKind::File, folder_real(&src.path, &e.path), format!("f:{}|{}", src.path, e.path));
        let info = &mut b.t.nodes[id].info;
        (info.entry_id, info.file_kind) = (Some(e.id), Some(names::kind_of(name).as_str().to_string()));
    }
}

fn add_archive_entries(b: &mut Builder, top: NodeId, src: &SourceRow, entries: &[&EntryRow]) {
    // nested archives whose contents were listed (a directory segment named like an archive only
    // comes from expansion): shown as folders, their own file entry dropped
    let expanded: HashSet<&str> = entries
        .iter()
        .flat_map(|e| e.path.match_indices('/').map(|(i, _)| &e.path[..i]))
        .filter(|p| names::kind_of(p) == names::Kind::Archive)
        .collect();
    for e in entries {
        if expanded.contains(e.path.as_str()) {
            continue;
        }
        let segs: Vec<&str> = e.path.split('/').filter(|s| !s.is_empty()).collect();
        let Some((name, dirs)) = segs.split_last() else { continue };
        let mut node = top;
        for i in 0..dirs.len() {
            let prefix = dirs[..=i].join("/");
            let real = archive_prefix_real(&src.path, &e.chain, i + 1);
            let key = format!("f:{}|{prefix}", src.path);
            if expanded.contains(prefix.as_str()) {
                node = b.folder(node, dirs[i], &names::archive_stem(dirs[i]), NodeKind::Archive, &real, &key);
                b.t.nodes[node].info.badge = Some(badge(dirs[i]));
            } else {
                node = b.folder(node, dirs[i], dirs[i], NodeKind::Folder, &real, &key);
            }
        }
        let (real, key) = (archive_real(&src.path, &e.chain), format!("f:{}|{}", src.path, e.path));
        if e.locked {
            let id = b.leaf(node, &names::archive_stem(name), NodeKind::Locked, real, key);
            b.t.nodes[id].info.badge = Some(badge(name));
            b.t.nodes[id].info.entry_id = Some(e.id);
        } else {
            let id = b.leaf(node, name, NodeKind::File, real, key);
            let info = &mut b.t.nodes[id].info;
            (info.entry_id, info.file_kind) = (Some(e.id), Some(names::kind_of(name).as_str().to_string()));
        }
    }
}
