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

/// A named SQL query returning one value. `stlx parity` runs each one on both DBs and the check
/// passes when the values are equal.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SpotCheck {
    pub name: String,
    pub sql: String,
}

/// Spot checks are tied to a real collection, so they live outside the repo: a JSON array of
/// `{"name": …, "sql": …}` objects (`spot-checks.json` in the app dir for `stlx parity`).
pub fn load_spot_checks(path: &std::path::Path) -> Result<Vec<SpotCheck>, Box<dyn std::error::Error>> {
    let items: Vec<serde_json::Value> = serde_json::from_str(&std::fs::read_to_string(path)?)?;
    items
        .iter()
        .map(|v| match (v["name"].as_str(), v["sql"].as_str()) {
            (Some(name), Some(sql)) => Ok(SpotCheck { name: name.into(), sql: sql.into() }),
            _ => Err(format!("{}: every spot check needs a \"name\" and an \"sql\" string", path.display()).into()),
        })
        .collect()
}

/// Spot-check values are equal, ignoring JSON list spacing (Python writes `", "`, we write `","`).
pub fn same_value(ours: &str, proto: &str) -> bool {
    let norm = |s: &str| s.replace("\", \"", "\",\"");
    norm(ours) == norm(proto)
}

/// The value of a spot check's SQL, as text (`NULL` for no value).
pub fn spot_value(conn: &Connection, sql: &str) -> rusqlite::Result<String> {
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
    fn spot_checks_load_and_run_on_our_schema() {
        let dir = tempfile::tempdir().unwrap();
        let conn = crate::db::open(&dir.path().join("index.db")).unwrap();
        let file = dir.path().join("spot-checks.json");
        std::fs::write(
            &file,
            r#"[{"name": "variants", "sql": "SELECT group_concat(variants || ' imgs=' || image_count, '; ') FROM models WHERE name = 'Kit'"},
                {"name": "posters", "sql": "SELECT count(*) FROM assets WHERE role = 'poster'"}]"#,
        )
        .unwrap();
        let checks = load_spot_checks(&file).unwrap();
        assert_eq!(checks.iter().map(|c| c.name.as_str()).collect::<Vec<_>>(), ["variants", "posters"]);
        assert_eq!(spot_value(&conn, &checks[0].sql).unwrap(), "NULL");
        assert_eq!(spot_value(&conn, &checks[1].sql).unwrap(), "0");
    }

    #[test]
    fn spot_checks_need_name_and_sql() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("spot-checks.json");
        std::fs::write(&file, r#"[{"name": "no sql"}]"#).unwrap();
        assert!(load_spot_checks(&file).is_err());
    }
}
