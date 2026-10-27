//! Resolve collections into the resolved tables (`groups`, `models`, `model_files`, `assets`).
//! The unit is one (root, collection); its rows are deleted and rebuilt whenever one of its
//! sources changes. Port of `resolve_all` / `collection_entries` (`cli.py`) and `store_result`
//! (`db.py`) in the prototype.
use std::collections::{BTreeSet, HashMap};

use indexmap::IndexMap;
use rusqlite::{Connection, Transaction, params};
use stl_core::names::Kind;
use stl_core::resolve::{AssetRole, Entry, Resolution, resolve};
use stl_scan::nested::ListOptions;

use crate::scanstore::{self, IndexError, Progress, ScanStats};

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct IndexStats {
    /// Summed over all scanned roots.
    pub scan: ScanStats,
    /// Collections resolved this run.
    pub resolved: usize,
    /// Models in the whole index afterwards.
    pub models: usize,
}

/// Scan every root that no other root covers, resolve the collections that changed (or were
/// never resolved), and drop resolved rows whose collection has no sources left.
pub fn index_all(
    conn: &mut Connection,
    opts: &ListOptions,
    progress: &mut dyn FnMut(Progress),
) -> Result<IndexStats, IndexError> {
    let roots: Vec<i64> = {
        let mut st = conn.prepare("SELECT id FROM roots ORDER BY id")?;
        st.query_map([], |r| r.get(0))?.collect::<Result<_, _>>()?
    };
    let mut stats = IndexStats::default();
    for root_id in roots {
        if scanstore::covering_root(conn, root_id)?.is_some() {
            continue;
        }
        let s = match scanstore::scan_root(conn, root_id, opts, progress) {
            Ok(s) => s,
            Err(IndexError::RootMissing(p)) => {
                progress(Progress::Warning(&format!("folder not found: {}", p.display())));
                continue;
            }
            Err(e) => return Err(e),
        };
        let mut todo: BTreeSet<String> = s.dirty.clone();
        {
            let mut st = conn.prepare(
                "SELECT DISTINCT collection FROM sources s WHERE root_id = ?1 AND collection IS NOT NULL
                 AND NOT EXISTS (SELECT 1 FROM resolved r WHERE r.root_id = ?1 AND r.collection = s.collection)",
            )?;
            let never: Vec<String> = st.query_map([root_id], |r| r.get(0))?.collect::<Result<_, _>>()?;
            todo.extend(never);
        }
        for coll in &todo {
            resolve_collection(conn, root_id, coll)?;
            stats.resolved += 1;
        }
        stats.scan.add(&s);
    }
    purge_orphans(conn)?;
    stats.models = conn.query_row("SELECT count(*) FROM models", [], |r| r.get::<_, i64>(0))? as usize;
    Ok(stats)
}

const UNIT_GONE: &str =
    "NOT EXISTS (SELECT 1 FROM sources s WHERE s.root_id = t.root_id AND s.collection = t.collection)";

fn purge_orphans(conn: &Connection) -> Result<(), IndexError> {
    conn.execute_batch(&format!(
        "DELETE FROM model_files WHERE model_id IN (SELECT id FROM models t WHERE {UNIT_GONE});
         DELETE FROM models AS t WHERE {UNIT_GONE};
         DELETE FROM groups AS t WHERE {UNIT_GONE};
         DELETE FROM assets AS t WHERE {UNIT_GONE};
         DELETE FROM resolved AS t WHERE {UNIT_GONE};"
    ))?;
    Ok(())
}

fn delete_unit(tx: &Transaction, root_id: i64, collection: &str) -> rusqlite::Result<()> {
    tx.execute(
        "DELETE FROM model_files WHERE model_id IN (SELECT id FROM models WHERE root_id = ?1 AND collection = ?2)",
        params![root_id, collection],
    )?;
    for t in ["models", "groups", "assets", "resolved"] {
        tx.execute(&format!("DELETE FROM {t} WHERE root_id = ?1 AND collection = ?2"), params![root_id, collection])?;
    }
    Ok(())
}

/// The unit's entries with paths relative to the collection folder (archives become folders:
/// `<archive path in collection>/<inner path>`), and entry id → source id.
fn collection_entries(tx: &Transaction, root_id: i64, collection: &str) -> rusqlite::Result<(Vec<Entry>, HashMap<i64, i64>)> {
    let sources: Vec<(i64, String, String)> = {
        let mut st = tx.prepare(
            "SELECT id, kind, coalesce(coll_path, '') FROM sources WHERE root_id = ?1 AND collection = ?2 ORDER BY rel_path",
        )?;
        st.query_map(params![root_id, collection], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?.collect::<Result<_, _>>()?
    };
    let mut entries = Vec::new();
    let mut owner = HashMap::new();
    let mut st = tx.prepare("SELECT id, path, coalesce(size, 0) FROM entries WHERE source_id = ? ORDER BY id")?;
    for (sid, kind, coll_path) in sources {
        let prefix = if kind == "archive" && !coll_path.is_empty() { format!("{coll_path}/") } else { String::new() };
        let rows = st.query_map([sid], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?, r.get::<_, i64>(2)?)))?;
        for row in rows {
            let (id, path, size) = row?;
            entries.push(Entry { id, path: format!("{prefix}{path}"), size: size as u64 });
            owner.insert(id, sid);
        }
    }
    Ok((entries, owner))
}

/// Rebuild one unit's resolved rows. Returns the number of models it now has.
pub fn resolve_collection(conn: &mut Connection, root_id: i64, collection: &str) -> Result<usize, IndexError> {
    let tx = conn.transaction()?;
    delete_unit(&tx, root_id, collection)?;
    let (entries, owner) = collection_entries(&tx, root_id, collection)?;
    let mut models = 0;
    if !entries.is_empty() {
        let name = collection.split_once('/').map_or(collection, |(_, rest)| rest);
        let res = resolve(name, &entries);
        models = res.models.len();
        store(&tx, root_id, collection, &res, &owner)?;
    }
    tx.execute("INSERT INTO resolved(root_id, collection) VALUES (?1, ?2)", params![root_id, collection])?;
    tx.commit()?;
    Ok(models)
}

fn store(tx: &Transaction, root_id: i64, collection: &str, res: &Resolution, owner: &HashMap<i64, i64>) -> rusqlite::Result<()> {
    let mut gid: Vec<i64> = Vec::with_capacity(res.groups.len());
    {
        let mut ins = tx.prepare(
            "INSERT INTO groups(root_id, collection, parent_id, name, raw_name, path, attrs) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        )?;
        // parents are created before children
        for g in &res.groups {
            ins.execute(params![root_id, collection, g.parent.map(|p| gid[p]), g.name, g.raw_name, g.path, g.attrs.to_json()])?;
            gid.push(tx.last_insert_rowid());
        }
    }
    let mut ins_asset = tx.prepare(
        "INSERT INTO assets(root_id, collection, source_id, model_id, group_id, entry_id, name, kind, role, match, rank)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
    )?;
    let mut insert_asset = |a: &stl_core::resolve::Asset, mid: Option<i64>, group: Option<i64>, rank: usize| -> rusqlite::Result<i64> {
        ins_asset.execute(params![
            root_id,
            collection,
            owner.get(&a.entry_id),
            mid,
            group,
            a.entry_id,
            a.name,
            a.kind.as_str(),
            a.role.as_str(),
            a.match_.as_str(),
            rank as i64
        ])?;
        Ok(tx.last_insert_rowid())
    };
    // group (or None: collection level) -> first poster image
    let mut group_poster: HashMap<Option<usize>, i64> = HashMap::new();
    for (rank, a) in res.assets.iter().filter(|a| a.model.is_none()).enumerate() {
        let aid = insert_asset(a, None, a.group.map(|g| gid[g]), rank)?;
        if a.kind == Kind::Image && a.role == AssetRole::Poster {
            group_poster.entry(a.group).or_insert(aid);
        }
    }
    let vol = collection.split('/').next().unwrap_or(collection);
    let mut ins_model = tx.prepare(
        "INSERT INTO models(root_id, collection, source_id, group_id, name, raw_name, path, breadcrumb, catalog_id, designer,
             edition, attrs, variants, part_groups, file_count, image_count)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16)",
    )?;
    let mut ins_file = tx.prepare(
        "INSERT INTO model_files(model_id, entry_id, name, kind, part_group, variant) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
    )?;
    for (mi, m) in res.models.iter().enumerate() {
        let mut crumbs = Vec::new();
        let mut g = m.group;
        while let Some(x) = g {
            crumbs.push(res.groups[x].name.as_str());
            g = res.groups[x].parent;
        }
        crumbs.push(vol);
        crumbs.reverse();
        if crumbs.len() == 1 && collection != vol {
            // the model is the whole collection folder
            crumbs.push(collection.split_once('/').map_or(collection, |(_, rest)| rest));
        }
        let parts: BTreeSet<&str> = m.files.iter().map(|f| f.part_group.as_str()).filter(|p| !p.is_empty()).collect();
        let images = res.model_assets(mi).iter().filter(|a| a.kind == Kind::Image).count();
        let mut owners: IndexMap<Option<i64>, usize> = IndexMap::new();
        for f in &m.files {
            *owners.entry(owner.get(&f.entry_id).copied()).or_default() += 1;
        }
        // most common owner, first seen wins a tie
        let source = owners.iter().fold(None, |best: Option<(Option<i64>, usize)>, (s, c)| match best {
            Some((_, bc)) if bc >= *c => best,
            _ => Some((*s, *c)),
        });
        let file_count = m.files.iter().filter(|f| matches!(f.kind, Kind::Model | Kind::Scene | Kind::Archive)).count();
        ins_model.execute(params![
            root_id,
            collection,
            source.and_then(|(s, _)| s),
            m.group.map(|g| gid[g]),
            m.name,
            m.raw_name,
            m.path,
            crumbs.join(" / "),
            m.attrs.catalog_id,
            m.attrs.designer,
            m.attrs.edition,
            m.attrs.to_json(),
            serde_json::to_string(&m.variants()).expect("strings serialize"),
            serde_json::to_string(&parts).expect("strings serialize"),
            file_count as i64,
            images as i64
        ])?;
        let mid = tx.last_insert_rowid();
        for f in &m.files {
            ins_file.execute(params![mid, f.entry_id, f.name, f.kind.as_str(), f.part_group, f.variant])?;
        }
        let mut hero = None;
        for (rank, &ai) in m.assets.iter().enumerate() {
            let a = &res.assets[ai];
            let aid = insert_asset(a, Some(mid), None, rank)?;
            if hero.is_none() && a.kind == Kind::Image {
                hero = Some(aid);
            }
        }
        // nearest group poster, else the collection-level one
        let mut fallback = hero;
        let mut g = m.group;
        while fallback.is_none()
            && let Some(x) = g
        {
            fallback = group_poster.get(&Some(x)).copied();
            g = res.groups[x].parent;
        }
        if fallback.is_none() {
            fallback = group_poster.get(&None).copied();
        }
        tx.execute("UPDATE models SET hero_asset_id = ?1, hero_fallback_asset_id = ?2 WHERE id = ?3", params![hero, fallback, mid])?;
    }
    Ok(())
}
