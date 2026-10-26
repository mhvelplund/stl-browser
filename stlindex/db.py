"""SQLite storage. Raw listings (sources, entries) are cached between runs; the resolved tables
(groups, models, model_files, assets) are rebuilt from them on every index run, one collection
(top-level folder, archives treated as folders) at a time."""
import json
import sqlite3
import time
from collections import Counter

SCHEMA = """
CREATE TABLE IF NOT EXISTS sources(
    id INTEGER PRIMARY KEY, kind TEXT NOT NULL, path TEXT NOT NULL UNIQUE, rel_path TEXT,
    name TEXT, collection TEXT, coll_path TEXT, size INTEGER, mtime REAL, nested INTEGER,
    scanned_at TEXT, error TEXT);
CREATE TABLE IF NOT EXISTS entries(
    id INTEGER PRIMARY KEY, source_id INTEGER NOT NULL REFERENCES sources(id) ON DELETE CASCADE,
    path TEXT NOT NULL, chain TEXT NOT NULL, size INTEGER, crc TEXT);
CREATE INDEX IF NOT EXISTS entries_source ON entries(source_id);

CREATE TABLE IF NOT EXISTS groups(
    id INTEGER PRIMARY KEY, collection TEXT NOT NULL, parent_id INTEGER,
    name TEXT, raw_name TEXT, path TEXT, attrs TEXT);
CREATE TABLE IF NOT EXISTS models(
    id INTEGER PRIMARY KEY, collection TEXT NOT NULL, source_id INTEGER, group_id INTEGER,
    name TEXT, raw_name TEXT, path TEXT, breadcrumb TEXT,
    catalog_id TEXT, designer TEXT, edition TEXT, attrs TEXT,
    variants TEXT, part_groups TEXT, file_count INTEGER, image_count INTEGER, hero_asset_id INTEGER,
    hero_fallback_asset_id INTEGER);
CREATE TABLE IF NOT EXISTS model_files(
    id INTEGER PRIMARY KEY, model_id INTEGER NOT NULL, entry_id INTEGER NOT NULL,
    name TEXT, kind TEXT, part_group TEXT, variant TEXT);
CREATE TABLE IF NOT EXISTS assets(
    id INTEGER PRIMARY KEY, collection TEXT NOT NULL, source_id INTEGER, model_id INTEGER, group_id INTEGER,
    entry_id INTEGER NOT NULL, name TEXT, kind TEXT, role TEXT, match TEXT, rank INTEGER);
CREATE INDEX IF NOT EXISTS model_files_model ON model_files(model_id);
CREATE INDEX IF NOT EXISTS assets_model ON assets(model_id);
CREATE INDEX IF NOT EXISTS models_source ON models(source_id);
"""
RESOLVED_TABLES = ('assets', 'model_files', 'models', 'groups')
SCHEMA_VERSION = 3


def connect(path):
    conn = sqlite3.connect(path)
    conn.row_factory = sqlite3.Row
    conn.execute('PRAGMA foreign_keys = ON')
    conn.execute('PRAGMA journal_mode = WAL')
    if conn.execute('PRAGMA user_version').fetchone()[0] < SCHEMA_VERSION:
        for t in RESOLVED_TABLES:                 # derived data: safe to drop, rebuilt by `index`
            conn.execute(f'DROP TABLE IF EXISTS {t}')
        cols = {r[1] for r in conn.execute('PRAGMA table_info(sources)')}
        if cols and 'coll_path' not in cols:      # keep cached listings, metadata is refreshed by `index`
            conn.execute('ALTER TABLE sources ADD COLUMN coll_path TEXT')
        conn.execute(f'PRAGMA user_version = {SCHEMA_VERSION}')
    conn.executescript(SCHEMA)
    return conn


def get_source(conn, path):
    return conn.execute('SELECT * FROM sources WHERE path = ?', (path,)).fetchone()


def _meta(src):
    return (src.kind, src.rel_path, src.name, src.collection, src.coll_path, src.size, src.mtime)


def update_source_meta(conn, src):
    """Refresh naming/placement of a cached source without re-listing it."""
    conn.execute('UPDATE sources SET kind=?, rel_path=?, name=?, collection=?, coll_path=?, size=?, mtime=? '
                 'WHERE path=?', _meta(src) + (src.path,))


def store_listing(conn, src, entries, nested, error=None):
    """Replace the cached listing for a source. Returns source id."""
    row = get_source(conn, src.path)
    now = time.strftime('%Y-%m-%d %H:%M:%S')
    vals = _meta(src) + (int(nested), now, error)
    if row:
        sid = row['id']
        conn.execute('UPDATE sources SET kind=?, rel_path=?, name=?, collection=?, coll_path=?, size=?, mtime=?, '
                     'nested=?, scanned_at=?, error=? WHERE id=?', vals + (sid,))
        conn.execute('DELETE FROM entries WHERE source_id=?', (sid,))
    else:
        sid = conn.execute('INSERT INTO sources(kind, rel_path, name, collection, coll_path, size, mtime, nested, '
                           'scanned_at, error, path) VALUES (?,?,?,?,?,?,?,?,?,?,?)', vals + (src.path,)).lastrowid
    conn.executemany('INSERT INTO entries(source_id, path, chain, size, crc) VALUES (?,?,?,?,?)',
                     [(sid, e.path, json.dumps(e.chain), e.size, e.crc) for e in entries])
    return sid


def remove_missing_sources(conn, keep_paths, under_collections):
    """Drop cached sources of the scanned roots that are no longer on disk."""
    gone = [r['id'] for r in conn.execute('SELECT id, path, collection FROM sources')
            if r['path'] not in keep_paths and (r['collection'] or '').split('/')[0] in under_collections]
    for sid in gone:
        conn.execute('DELETE FROM sources WHERE id=?', (sid,))
    return len(gone)


def clear_resolved(conn):
    for t in RESOLVED_TABLES:
        conn.execute(f'DELETE FROM {t}')


def store_result(conn, collection, res, entry_source):
    """Insert a resolve.Result for one collection. entry_source: entry id -> source id."""
    gid = {}
    for g in res.groups:                                   # parents are created before children
        gid[g] = conn.execute(
            'INSERT INTO groups(collection, parent_id, name, raw_name, path, attrs) VALUES (?,?,?,?,?,?)',
            (collection, gid.get(g.parent), g.name, g.raw_name, g.path, json.dumps(g.attrs))).lastrowid

    def insert_asset(a, mid, gid_, rank):
        return conn.execute(
            'INSERT INTO assets(collection, source_id, model_id, group_id, entry_id, name, kind, role, match, rank) '
            'VALUES (?,?,?,?,?,?,?,?,?,?)',
            (collection, entry_source.get(a.entry.id), mid, gid_, a.entry.id, a.name, a.kind, a.role, a.match,
             rank)).lastrowid

    group_poster = {}                                      # group -> first poster image asset id
    for rank, a in enumerate(x for x in res.assets if x.model is None):
        aid = insert_asset(a, None, gid.get(a.group), rank)
        if a.kind == 'image' and a.role == 'poster':
            group_poster.setdefault(a.group, aid)
    vol = collection.split('/')[0]
    for m in res.models:
        crumbs, g = [], m.group
        while g is not None:
            crumbs.append(g.name)
            g = g.parent
        crumbs = [vol, *reversed(crumbs)]
        if len(crumbs) == 1 and collection != vol:          # model is the whole collection folder
            crumbs.append(collection.split('/', 1)[1])
        parts = sorted({f.part_group for f in m.files if f.part_group})
        images = [a for a in m.assets if a.kind == 'image']
        owner = Counter(entry_source.get(f.entry.id) for f in m.files).most_common(1)
        mid = conn.execute(
            'INSERT INTO models(collection, source_id, group_id, name, raw_name, path, breadcrumb, catalog_id, '
            'designer, edition, attrs, variants, part_groups, file_count, image_count) '
            'VALUES (?,?,?,?,?,?,?,?,?,?,?,?,?,?,?)',
            (collection, owner[0][0] if owner else None, gid.get(m.group), m.name, m.raw_name, m.path,
             ' / '.join(crumbs), m.attrs.get('catalog_id'), m.attrs.get('designer'), m.attrs.get('edition'),
             json.dumps(m.attrs), json.dumps(m.variants), json.dumps(parts),
             sum(f.kind in ('model', 'scene', 'archive') for f in m.files), len(images))).lastrowid
        conn.executemany(
            'INSERT INTO model_files(model_id, entry_id, name, kind, part_group, variant) VALUES (?,?,?,?,?,?)',
            [(mid, f.entry.id, f.name, f.kind, f.part_group, f.variant) for f in m.files])
        hero = None
        for rank, a in enumerate(m.assets):
            aid = insert_asset(a, mid, None, rank)
            if hero is None and a.kind == 'image':
                hero = aid
        fallback, g = hero, m.group
        while fallback is None and g is not None:          # nearest group poster
            fallback, g = group_poster.get(g), g.parent
        if fallback is None:
            fallback = group_poster.get(None)              # collection-level poster
        conn.execute('UPDATE models SET hero_asset_id=?, hero_fallback_asset_id=? WHERE id=?',
                     (hero, fallback, mid))
