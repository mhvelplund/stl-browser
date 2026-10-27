use stl_index::{db, paths};

fn tables(conn: &rusqlite::Connection) -> Vec<String> {
    let mut st = conn
        .prepare("SELECT name FROM sqlite_master WHERE type = 'table' ORDER BY name")
        .unwrap();
    st.query_map([], |r| r.get(0))
        .unwrap()
        .map(Result::unwrap)
        .collect()
}

#[test]
fn open_creates_schema() {
    let dir = tempfile::tempdir().unwrap();
    let conn = db::open(&dir.path().join("index.db")).unwrap();
    let v: i32 = conn
        .query_row("PRAGMA user_version", [], |r| r.get(0))
        .unwrap();
    assert_eq!(v, 2);
    assert_eq!(db::SCHEMA_VERSION, 2);
    let t = tables(&conn);
    for name in [
        "roots",
        "sources",
        "entries",
        "settings",
        "selected_thumb",
        "tags",
        "node_tags",
    ] {
        assert!(t.iter().any(|x| x == name), "missing table {name}: {t:?}");
    }
}

#[test]
fn open_is_idempotent() {
    let dir = tempfile::tempdir().unwrap();
    let p = dir.path().join("index.db");
    {
        let conn = db::open(&p).unwrap();
        db::set_setting(&conn, "k", "v").unwrap();
    }
    let conn = db::open(&p).unwrap();
    assert_eq!(db::get_setting(&conn, "k").unwrap().as_deref(), Some("v"));
}

#[test]
fn settings_roundtrip() {
    let dir = tempfile::tempdir().unwrap();
    let conn = db::open(&dir.path().join("index.db")).unwrap();
    assert_eq!(db::get_setting(&conn, "x").unwrap(), None);
    db::set_setting(&conn, "x", "1").unwrap();
    db::set_setting(&conn, "x", "2").unwrap();
    assert_eq!(db::get_setting(&conn, "x").unwrap().as_deref(), Some("2"));
}

#[test]
fn app_dir_respects_env() {
    let dir = tempfile::tempdir().unwrap();
    let home = dir.path().join("h");
    // SAFETY: the only test in this binary that touches the environment.
    unsafe { std::env::set_var("STL_BROWSER_HOME", &home) };
    assert_eq!(paths::app_dir(), home);
    assert!(home.is_dir());
    assert_eq!(paths::db_path(), home.join("index.db"));
}

#[test]
fn deleting_root_cascades() {
    let dir = tempfile::tempdir().unwrap();
    let conn = db::open(&dir.path().join("index.db")).unwrap();
    conn.execute(
        "INSERT INTO roots(id, path, name) VALUES (1, 'D:/x', 'x')",
        [],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO sources(id, root_id, kind, path) VALUES (1, 1, 'archive', 'D:/x/a.zip')",
        [],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO entries(source_id, path, chain) VALUES (1, 'a.stl', '[\"a.stl\"]')",
        [],
    )
    .unwrap();
    conn.execute("DELETE FROM roots WHERE id = 1", []).unwrap();
    let n: i64 = conn
        .query_row(
            "SELECT (SELECT count(*) FROM sources) + (SELECT count(*) FROM entries)",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(n, 0);
}
