//! Build the Models and Files views from the DB. A root that sits inside another root has no
//! sources of its own; it becomes a volume showing the same content as its folder in the outer one.
use std::collections::HashMap;

use rusqlite::Connection;
use stl_core::vtree::files::{EntryRow, RootRow, SourceRow, build_files};
use stl_core::vtree::models::{AssetRow, CollectionRow, FileRow, GroupRow, LockedRow, ModelRow, ModelsInput, build_models};
use stl_core::vtree::{Node, NodeId, NodeKind, Tree, archive_real, folder_real};

use crate::scanstore::{IndexError, covering_root};

/// Hover path of an entry: its chain inside an archive, or its path below a folder source.
fn entry_real(source_kind: &str, source_path: &str, chain: &[String]) -> String {
    if source_kind == "folder" {
        folder_real(source_path, chain.first().map_or("", String::as_str))
    } else {
        archive_real(source_path, chain)
    }
}

fn chain_of(json: &str) -> Vec<String> {
    serde_json::from_str(json).unwrap_or_default()
}

fn models_alt(n: &Node) -> Option<String> {
    n.info.edition.clone().or_else(|| n.info.raw_name.clone().filter(|r| r.to_lowercase() != n.name))
}

fn roots(conn: &Connection) -> Result<Vec<RootRow>, IndexError> {
    let mut st = conn.prepare("SELECT id, name, path FROM roots ORDER BY id")?;
    let rows = st.query_map([], |r| Ok(RootRow { id: r.get(0)?, name: r.get(1)?, path: r.get(2)? }))?.collect::<Result<_, _>>()?;
    Ok(rows)
}

/// Rows of model files or assets joined to their entry and source: (row, real, key).
const ENTRY_JOIN: &str = "JOIN entries e ON e.id = x.entry_id JOIN sources s ON s.id = e.source_id";

pub fn models_view(conn: &Connection) -> Result<Tree, IndexError> {
    let roots = roots(conn)?;
    let root_path: HashMap<i64, &str> = roots.iter().map(|r| (r.id, r.path.as_str())).collect();
    let mut input = ModelsInput::default();
    {
        let mut st = conn.prepare("SELECT DISTINCT root_id, collection FROM sources WHERE collection IS NOT NULL ORDER BY collection")?;
        let rows: Vec<(i64, String)> = st.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?.collect::<Result<_, _>>()?;
        for (root_id, collection) in rows {
            let Some(&rp) = root_path.get(&root_id) else { continue };
            let (volume, rel) = collection.split_once('/').map_or((collection.as_str(), ""), |(v, r)| (v, r));
            input.collections.push(CollectionRow {
                volume: volume.to_string(),
                collection: collection.clone(),
                real: vec![folder_real(rp, rel)],
                root_real: rp.to_string(),
            });
        }
    }
    {
        let mut st = conn.prepare(
            "SELECT g.id, g.collection, g.parent_id, g.name, g.path,
                (SELECT min(a.id) FROM assets a WHERE a.group_id = g.id AND a.model_id IS NULL AND a.kind = 'image'),
                (SELECT count(*) FROM assets a WHERE a.group_id = g.id AND a.model_id IS NULL AND a.kind = 'image')
             FROM groups g ORDER BY g.id",
        )?;
        input.groups = st
            .query_map([], |r| {
                Ok(GroupRow {
                    id: r.get(0)?,
                    collection: r.get(1)?,
                    parent_id: r.get(2)?,
                    name: r.get(3)?,
                    path: r.get(4)?,
                    poster_asset_id: r.get(5)?,
                    posters: r.get(6)?,
                })
            })?
            .collect::<Result<_, _>>()?;
    }
    {
        let mut st = conn.prepare(
            "SELECT id, collection, group_id, name, raw_name, path, edition, coalesce(hero_asset_id, hero_fallback_asset_id),
                file_count, image_count, variants FROM models ORDER BY id",
        )?;
        input.models = st
            .query_map([], |r| {
                Ok(ModelRow {
                    id: r.get(0)?,
                    collection: r.get(1)?,
                    group_id: r.get(2)?,
                    name: r.get(3)?,
                    raw_name: r.get(4)?,
                    path: r.get(5)?,
                    edition: r.get(6)?,
                    hero_asset_id: r.get(7)?,
                    file_count: r.get(8)?,
                    image_count: r.get(9)?,
                    variants: serde_json::from_str(&r.get::<_, String>(10)?).unwrap_or_default(),
                })
            })?
            .collect::<Result<_, _>>()?;
    }
    {
        let mut st = conn.prepare(&format!(
            "SELECT x.model_id, x.entry_id, x.name, x.kind, x.part_group, x.variant, e.path, e.chain, s.kind, s.path
             FROM model_files x {ENTRY_JOIN} ORDER BY x.model_id, x.id"
        ))?;
        input.files = st
            .query_map([], |r| {
                let (epath, chain, skind, spath): (String, String, String, String) = (r.get(6)?, r.get(7)?, r.get(8)?, r.get(9)?);
                Ok(FileRow {
                    model_id: r.get(0)?,
                    entry_id: r.get(1)?,
                    name: r.get(2)?,
                    kind: r.get(3)?,
                    part_group: r.get(4)?,
                    variant: r.get(5)?,
                    real: entry_real(&skind, &spath, &chain_of(&chain)),
                    key: format!("f:{spath}|{epath}"),
                })
            })?
            .collect::<Result<_, _>>()?;
    }
    {
        let mut st = conn.prepare(&format!(
            "SELECT x.id, x.model_id, x.entry_id, x.name, x.kind, x.role, e.path, e.chain, s.kind, s.path
             FROM assets x {ENTRY_JOIN} WHERE x.model_id IS NOT NULL ORDER BY x.model_id, x.rank, x.id"
        ))?;
        input.assets = st
            .query_map([], |r| {
                let (epath, chain, skind, spath): (String, String, String, String) = (r.get(6)?, r.get(7)?, r.get(8)?, r.get(9)?);
                Ok(AssetRow {
                    id: r.get(0)?,
                    model_id: r.get(1)?,
                    entry_id: r.get(2)?,
                    name: r.get(3)?,
                    kind: r.get(4)?,
                    role: r.get(5)?,
                    real: entry_real(&skind, &spath, &chain_of(&chain)),
                    key: format!("f:{spath}|{epath}"),
                })
            })?
            .collect::<Result<_, _>>()?;
    }
    {
        let mut st = conn.prepare("SELECT collection, name, path FROM sources WHERE locked = 1 AND collection IS NOT NULL ORDER BY path")?;
        input.locked = st
            .query_map([], |r| {
                let path: String = r.get(2)?;
                Ok(LockedRow { collection: r.get(0)?, name: r.get(1)?, key: format!("f:{path}|"), real: path })
            })?
            .collect::<Result<_, _>>()?;
    }
    let mut t = build_models(&input, true);
    set_heroes(conn, &mut t)?;
    add_inner_volumes(conn, &mut t, &roots, &models_alt)?;
    Ok(t)
}

/// Point models at their hero image (or fallback poster) and groups at their poster.
fn set_heroes(conn: &Connection, t: &mut Tree) -> Result<(), IndexError> {
    let mut st = conn.prepare("SELECT a.id, a.entry_id, s.path, e.path FROM assets a JOIN entries e ON e.id = a.entry_id JOIN sources s ON s.id = e.source_id")?;
    let assets: HashMap<i64, (i64, String)> = st
        .query_map([], |r| Ok((r.get(0)?, (r.get(1)?, format!("f:{}|{}", r.get::<_, String>(2)?, r.get::<_, String>(3)?)))))?
        .collect::<Result<_, _>>()?;
    for n in &mut t.nodes {
        let asset = match n.kind {
            NodeKind::Model => n.info.hero_asset_id,
            NodeKind::Group | NodeKind::Collection => n.info.poster_asset_id,
            _ => None,
        };
        if let Some((entry, key)) = asset.and_then(|a| assets.get(&a)) {
            (n.info.hero_entry_id, n.info.hero_key) = (Some(*entry), Some(key.clone()));
        }
    }
    Ok(())
}

pub fn files_view(conn: &Connection) -> Result<Tree, IndexError> {
    let roots = roots(conn)?;
    let sources: Vec<SourceRow> = {
        let mut st = conn.prepare("SELECT id, root_id, kind, path, coalesce(rel_path, ''), locked FROM sources ORDER BY rel_path")?;
        st.query_map([], |r| {
            Ok(SourceRow { id: r.get(0)?, root_id: r.get(1)?, kind: r.get(2)?, path: r.get(3)?, rel_path: r.get(4)?, locked: r.get(5)? })
        })?
        .collect::<Result<_, _>>()?
    };
    let entries: Vec<EntryRow> = {
        let mut st = conn.prepare("SELECT id, source_id, path, chain, locked FROM entries ORDER BY source_id, id")?;
        st.query_map([], |r| {
            Ok(EntryRow { id: r.get(0)?, source_id: r.get(1)?, path: r.get(2)?, chain: chain_of(&r.get::<_, String>(3)?), locked: r.get(4)? })
        })?
        .collect::<Result<_, _>>()?
    };
    let mut t = build_files(&roots, &sources, &entries);
    add_inner_volumes(conn, &mut t, &roots, &|_| None)?;
    Ok(t)
}

/// Comparison form of a real path: lowercase, `\` separators, no trailing separator.
fn real_key(p: &str) -> String {
    p.replace('/', "\\").trim_end_matches('\\').to_lowercase()
}

fn under(real: &str, dir: &str) -> bool {
    let r = real_key(real);
    r == dir || r.strip_prefix(dir).is_some_and(|rest| rest.starts_with(['\\', '!']))
}

/// One volume per covered root, showing the content of the node whose real path is that root's
/// folder; if the view has no such node (the resolver collapsed it), the topmost nodes below it.
fn add_inner_volumes(
    conn: &Connection,
    t: &mut Tree,
    roots: &[RootRow],
    alt: &dyn Fn(&Node) -> Option<String>,
) -> Result<(), IndexError> {
    let mut added = false;
    for root in roots {
        if covering_root(conn, root.id)?.is_none() {
            continue;
        }
        let dir = real_key(&root.path);
        let mut exact: Option<NodeId> = None;
        let mut frontier: Vec<NodeId> = Vec::new();
        let mut stack: Vec<NodeId> = t.nodes[0].children.clone();
        while let Some(id) = stack.pop() {
            let n = &t.nodes[id];
            if n.kind != NodeKind::Volume && n.real.iter().any(|r| real_key(r) == dir) {
                exact = Some(id);
                break;
            }
            if n.real.first().is_some_and(|r| under(r, &dir)) {
                frontier.push(id);
            } else {
                stack.extend(n.children.iter().copied());
            }
        }
        let children = match exact {
            Some(id) => t.nodes[id].children.clone(),
            None => frontier,
        };
        let v = t.add(0, &root.name, NodeKind::Volume);
        t.nodes[v].real = vec![root.path.clone()];
        t.nodes[v].key = format!("f:{}|", root.path);
        t.nodes[v].children = children;
        added = true;
    }
    if added {
        t.finish_children(0, alt);
    }
    Ok(())
}

