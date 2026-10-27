//! Compare the resolved tables with the prototype's DB (`stlx parity`). Every query uses only
//! columns the two schemas share, so it runs unchanged on both.
use std::collections::BTreeMap;

use rusqlite::Connection;
use rusqlite::types::Value;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Stats {
    pub models: u64,
    pub groups: u64,
    /// Models with `image_count = 0`.
    pub no_own_image: u64,
    /// Models with neither a hero image nor a fallback poster.
    pub no_image_or_fallback: u64,
    /// Images attached to no model (group and collection posters).
    pub unattached_images: u64,
    pub assets_by_role: BTreeMap<String, u64>,
}

/// Per-collection counts of the resolved tables.
pub fn collection_stats(conn: &Connection) -> rusqlite::Result<BTreeMap<String, Stats>> {
    let mut out: BTreeMap<String, Stats> = BTreeMap::new();
    let mut st = conn.prepare(
        "SELECT collection, count(*), coalesce(sum(image_count = 0), 0),
             coalesce(sum(hero_asset_id IS NULL AND hero_fallback_asset_id IS NULL), 0)
         FROM models GROUP BY collection",
    )?;
    for row in st.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))? {
        let (c, models, no_own, none) = row?;
        let s = out.entry(c).or_default();
        (s.models, s.no_own_image, s.no_image_or_fallback) = (models, no_own, none);
    }
    let mut st = conn.prepare("SELECT collection, count(*) FROM groups GROUP BY collection")?;
    for row in st.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get(1)?)))? {
        let (c, n) = row?;
        out.entry(c).or_default().groups = n;
    }
    let mut st = conn.prepare(
        "SELECT collection, role, count(*), coalesce(sum(model_id IS NULL AND kind = 'image'), 0)
         FROM assets GROUP BY collection, role",
    )?;
    for row in st.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get(2)?, r.get::<_, u64>(3)?)))? {
        let (c, role, n, unattached) = row?;
        let s = out.entry(c).or_default();
        s.assets_by_role.insert(role, n);
        s.unattached_images += unattached;
    }
    Ok(out)
}

/// The spot checks from `docs/archive-patterns.md`: (name, SQL returning one value). A check
/// passes when our value equals the prototype's.
pub const SPOT_CHECKS: &[(&str, &str)] = &[
    ("Asgardian Trikers", "SELECT group_concat(variants || ' imgs=' || image_count, '; ') FROM models WHERE name = 'Asgardian Trikers'"),
    (
        "VOIDWALKERS",
        "SELECT group_concat(name || ' ' || variants || ' ' || part_groups, '; ') FROM (SELECT * FROM models WHERE name LIKE 'VOIDWALKERS%' ORDER BY name)",
    ),
    (
        "Ghamak 842 Tarantula",
        "SELECT group_concat(m.name || ' hero=' || coalesce(a.name, '-'), '; ') FROM models m LEFT JOIN assets a ON a.id = m.hero_asset_id WHERE m.catalog_id = '842'",
    ),
    (
        "One Page Rules from nested .7z",
        "SELECT count(DISTINCT mf.model_id) FROM model_files mf JOIN entries e ON e.id = mf.entry_id JOIN models m ON m.id = mf.model_id
         WHERE e.path LIKE '%.7z/%' AND m.breadcrumb LIKE '%One Page Rules%'",
    ),
    ("Heldrake", "SELECT count(*) FROM models WHERE name = 'Heldrake'"),
    ("Imperial Fists Builder", "SELECT count(*) FROM models WHERE name LIKE '%Imperial%Fists%Builder%'"),
    ("Jetpack boys pk1", "SELECT group_concat(variants || ' imgs=' || image_count, '; ') FROM models WHERE name = 'Jetpack boys pk1'"),
    (
        "RoP AGG/CCW/COM",
        "SELECT group_concat(name || ' ' || variants || ' imgs=' || image_count, '; ') FROM (SELECT * FROM models WHERE name IN ('RoP AGG', 'RoP CCW', 'RoP COM') ORDER BY name)",
    ),
    ("Sanguinus", "SELECT group_concat(variants, '; ') FROM models WHERE name = 'Sanguinus'"),
    (
        "40K Empress final",
        "SELECT group_concat(a.name, '; ') FROM models m JOIN assets a ON a.id = m.hero_asset_id WHERE m.name = '40K Empress final'",
    ),
    (
        "TYRANIDS poster",
        "SELECT group_concat(a.role || ' -> ' || coalesce(g.name, '-'), '; ') FROM assets a LEFT JOIN groups g ON g.id = a.group_id
         WHERE a.name = 'VOD_-_TYRANIDS_Complete.jpg'",
    ),
    (
        "TYRANIDS units with own picture",
        "SELECT sum(m.image_count > 0) || '/' || count(*) FROM models m
         WHERE m.group_id IN (SELECT group_id FROM assets WHERE name = 'VOD_-_TYRANIDS_Complete.jpg' AND model_id IS NULL)",
    ),
    (
        "Illuminor Szeras editions",
        "SELECT group_concat(coalesce(edition, '-'), ',') FROM (SELECT edition FROM models WHERE name = 'Illuminor Szeras' ORDER BY edition)",
    ),
    ("Ancient Alien Pylon", "SELECT count(*) FROM models WHERE name = 'Ancient Alien Pylon'"),
];

/// Spot-check values are equal, ignoring JSON list spacing (Python writes `", "`, we write `","`).
pub fn same_value(ours: &str, proto: &str) -> bool {
    let norm = |s: &str| s.replace("\", \"", "\",\"");
    norm(ours) == norm(proto)
}

/// The value of a spot check, as text (`NULL` for no value).
pub fn spot_value(conn: &Connection, name: &str) -> rusqlite::Result<String> {
    let sql = SPOT_CHECKS.iter().find(|(n, _)| *n == name).map(|(_, s)| *s).ok_or(rusqlite::Error::InvalidQuery)?;
    let v: Value = conn.query_row(sql, [], |r| r.get(0))?;
    Ok(match v {
        Value::Null => "NULL".into(),
        Value::Integer(i) => i.to_string(),
        Value::Real(f) => f.to_string(),
        Value::Text(s) => s,
        Value::Blob(_) => "<blob>".into(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stats_count_models_and_unattached() {
        let dir = tempfile::tempdir().unwrap();
        let conn = crate::db::open(&dir.path().join("index.db")).unwrap();
        conn.execute_batch(
            "INSERT INTO groups(id, root_id, collection, name) VALUES (1, 1, 'V/A', 'A');
             INSERT INTO models(id, root_id, collection, group_id, name, image_count, hero_asset_id, hero_fallback_asset_id)
                 VALUES (1, 1, 'V/A', 1, 'm1', 2, 10, NULL), (2, 1, 'V/A', 1, 'm2', 0, NULL, 11),
                        (3, 1, 'V/A', 1, 'm3', 0, NULL, NULL);
             INSERT INTO assets(id, root_id, collection, model_id, group_id, entry_id, name, kind, role)
                 VALUES (10, 1, 'V/A', 1, NULL, 1, 'a.jpg', 'image', 'preview'),
                        (11, 1, 'V/A', NULL, 1, 2, 'p.jpg', 'image', 'poster');",
        )
        .unwrap();
        let stats = collection_stats(&conn).unwrap();
        let s = &stats["V/A"];
        assert_eq!((s.models, s.groups, s.no_own_image, s.no_image_or_fallback, s.unattached_images), (3, 1, 2, 1, 1));
        assert_eq!(s.assets_by_role.get("poster"), Some(&1));
        assert_eq!(s.assets_by_role.get("preview"), Some(&1));
    }

    #[test]
    fn spot_values_compare_ignoring_json_spacing() {
        assert!(same_value(r#"["a","b"] imgs=6"#, r#"["a", "b"] imgs=6"#));
        assert!(!same_value(r#"["a","b"]"#, r#"["a", "c"]"#));
    }

    #[test]
    fn spot_checks_run_on_our_schema() {
        let dir = tempfile::tempdir().unwrap();
        let conn = crate::db::open(&dir.path().join("index.db")).unwrap();
        for (name, _) in SPOT_CHECKS {
            spot_value(&conn, name).unwrap();
        }
    }
}
