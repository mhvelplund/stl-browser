//! SQLite schema and migrations. Cached listings (`roots`, `sources`, `entries`) and user data
//! (`settings`, `selected_thumb`, `tags`, `node_tags`) live here; user data is keyed by stable
//! node keys, never by row ids.
use std::path::Path;

use rusqlite::{Connection, OptionalExtension, Result, params};

pub const SCHEMA_VERSION: i32 = 4;

const V1: &str = "
CREATE TABLE roots(
    id INTEGER PRIMARY KEY, path TEXT UNIQUE NOT NULL, name TEXT NOT NULL, added_at TEXT);
CREATE TABLE sources(
    id INTEGER PRIMARY KEY, root_id INTEGER NOT NULL REFERENCES roots(id) ON DELETE CASCADE,
    kind TEXT NOT NULL, path TEXT UNIQUE NOT NULL, rel_path TEXT, name TEXT, collection TEXT,
    coll_path TEXT, size INTEGER, mtime REAL, scanned_at TEXT, error TEXT);
CREATE TABLE entries(
    id INTEGER PRIMARY KEY, source_id INTEGER NOT NULL REFERENCES sources(id) ON DELETE CASCADE,
    path TEXT NOT NULL, chain TEXT NOT NULL, size INTEGER, crc TEXT);
CREATE INDEX entries_source ON entries(source_id);
CREATE TABLE settings(key TEXT PRIMARY KEY, value TEXT);
CREATE TABLE selected_thumb(node_key TEXT PRIMARY KEY, image_key TEXT NOT NULL);
CREATE TABLE tags(id INTEGER PRIMARY KEY, name TEXT UNIQUE);
CREATE TABLE node_tags(
    node_key TEXT NOT NULL, tag_id INTEGER NOT NULL REFERENCES tags(id) ON DELETE CASCADE,
    PRIMARY KEY(node_key, tag_id));
";

/// v2: folder sources remember a fingerprint of their file names (detects renames).
const V2: &str = "ALTER TABLE sources ADD COLUMN fingerprint TEXT;";

/// v3: password-protected archives are flagged instead of scanned.
const V3: &str = "
ALTER TABLE sources ADD COLUMN locked INTEGER NOT NULL DEFAULT 0;
ALTER TABLE entries ADD COLUMN locked INTEGER NOT NULL DEFAULT 0;
";

/// v4: resolved tables, rebuilt per (root, collection) and never holding user data. `resolved`
/// marks the units that have been resolved since their sources last changed.
const V4: &str = "
CREATE TABLE groups(id INTEGER PRIMARY KEY, root_id INTEGER NOT NULL, collection TEXT NOT NULL, parent_id INTEGER,
    name TEXT, raw_name TEXT, path TEXT, attrs TEXT);
CREATE TABLE models(id INTEGER PRIMARY KEY, root_id INTEGER NOT NULL, collection TEXT NOT NULL, source_id INTEGER,
    group_id INTEGER, name TEXT, raw_name TEXT, path TEXT, breadcrumb TEXT, catalog_id TEXT, designer TEXT,
    edition TEXT, attrs TEXT, variants TEXT, part_groups TEXT, file_count INTEGER, image_count INTEGER,
    hero_asset_id INTEGER, hero_fallback_asset_id INTEGER);
CREATE TABLE model_files(id INTEGER PRIMARY KEY, model_id INTEGER NOT NULL, entry_id INTEGER NOT NULL,
    name TEXT, kind TEXT, part_group TEXT, variant TEXT);
CREATE TABLE assets(id INTEGER PRIMARY KEY, root_id INTEGER NOT NULL, collection TEXT NOT NULL, source_id INTEGER,
    model_id INTEGER, group_id INTEGER, entry_id INTEGER NOT NULL, name TEXT, kind TEXT, role TEXT, match TEXT,
    rank INTEGER);
CREATE TABLE resolved(root_id INTEGER NOT NULL, collection TEXT NOT NULL, PRIMARY KEY(root_id, collection));
CREATE INDEX groups_coll ON groups(root_id, collection);
CREATE INDEX models_coll ON models(root_id, collection);
CREATE INDEX model_files_model ON model_files(model_id);
CREATE INDEX assets_model ON assets(model_id);
CREATE INDEX assets_coll ON assets(root_id, collection);
";

/// Migration steps; `MIGRATIONS[i]` takes the schema from version `i` to `i + 1`.
const MIGRATIONS: [&str; SCHEMA_VERSION as usize] = [V1, V2, V3, V4];

/// Open (creating if needed) the index DB and bring its schema up to date.
pub fn open(path: &Path) -> Result<Connection> {
    let mut conn = Connection::open(path)?;
    conn.pragma_update(None, "journal_mode", "WAL")?;
    conn.pragma_update(None, "foreign_keys", "ON")?;
    let mut version: i32 = conn.query_row("PRAGMA user_version", [], |r| r.get(0))?;
    while version < SCHEMA_VERSION {
        let tx = conn.transaction()?;
        tx.execute_batch(MIGRATIONS[version as usize])?;
        version += 1;
        tx.pragma_update(None, "user_version", version)?;
        tx.commit()?;
    }
    Ok(conn)
}

pub fn get_setting(conn: &Connection, key: &str) -> Result<Option<String>> {
    conn.query_row("SELECT value FROM settings WHERE key = ?", [key], |r| {
        r.get(0)
    })
    .optional()
}

pub fn set_setting(conn: &Connection, key: &str, value: &str) -> Result<()> {
    conn.execute(
        "INSERT INTO settings(key, value) VALUES (?1, ?2) ON CONFLICT(key) DO UPDATE SET value = ?2",
        params![key, value],
    )?;
    Ok(())
}
