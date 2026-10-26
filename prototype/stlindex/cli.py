"""Command line: index | report | vtree | extract."""
import argparse
import json
import os
import sys
import time
from collections import defaultdict

from . import db as D
from . import scan as S
from . import vtree as V
from .resolve import Entry, resolve


def cmd_index(args):
    sz = S.SevenZip(S.find_7z(args.seven_zip))
    conn = D.connect(args.db)
    nested = not args.no_nested
    seen, vols, t0 = set(), set(), time.time()
    sources = []
    for root in args.roots:
        found = list(S.discover(root))
        vols.update(s.collection.split('/')[0] for s in found)
        sources += [s for s in found if not args.only or args.only.lower() in s.rel_path.lower()]
    for i, src in enumerate(sources, 1):
        seen.add(src.path)
        row = D.get_source(conn, src.path)
        fresh = (row and not args.rescan and row['error'] is None and row['size'] == src.size
                 and abs((row['mtime'] or 0) - src.mtime) < 1 and (row['nested'] or not nested))
        if fresh:
            D.update_source_meta(conn, src)
            print(f'[{i}/{len(sources)}] cached  {src.rel_path}')
            continue
        t = time.time()
        print(f'[{i}/{len(sources)}] listing {src.rel_path} ...', flush=True)
        try:
            entries = S.scan_source(sz, src, nested, warn=lambda m: print(m, flush=True))
            D.store_listing(conn, src, entries, nested)
            print(f'          {len(entries)} files in {time.time() - t:.1f}s', flush=True)
        except S.ScanError as e:
            D.store_listing(conn, src, [], nested, error=str(e))
            print(f'          ERROR {e}', flush=True)
        conn.commit()
    if not args.only:
        removed = D.remove_missing_sources(conn, seen, vols)
        if removed:
            print(f'removed {removed} sources no longer on disk')
    resolve_all(conn)
    conn.commit()
    n_models = conn.execute('SELECT count(*) FROM models').fetchone()[0]
    print(f'done: {len(sources)} sources, {n_models} models, {time.time() - t0:.1f}s -> {args.db}')
    return 0


def collection_entries(conn):
    """collection -> (entries with paths relative to the collection folder, entry id -> source id).
    Archives become folders: '<archive path in collection>/<inner path>'."""
    out = defaultdict(lambda: ([], {}))
    for src in conn.execute('SELECT * FROM sources ORDER BY rel_path').fetchall():
        entries, owner = out[src['collection'] or src['name']]
        prefix = (src['coll_path'] + '/') if src['kind'] == 'archive' and src['coll_path'] else ''
        for r in conn.execute('SELECT id, path, size FROM entries WHERE source_id=?', (src['id'],)):
            entries.append(Entry(r['id'], prefix + r['path'], r['size']))
            owner[r['id']] = src['id']
    return out


def resolve_all(conn):
    D.clear_resolved(conn)
    for coll, (entries, owner) in sorted(collection_entries(conn).items()):
        if entries:
            name = coll.split('/', 1)[-1]
            D.store_result(conn, coll, resolve(name, entries), owner)


def cmd_report(args):
    conn = D.connect(args.db)
    like = f'%{args.source}%' if args.source else '%'
    tot = dict(models=0, noimg=0, images=0, posters=0)
    print(f'{"source":64} {"models":>6} {"no-img":>6} {"imgs":>5} {"poster":>6}')
    for coll in [r[0] for r in conn.execute('SELECT DISTINCT collection FROM sources ORDER BY collection')]:
        rows = conn.execute('SELECT * FROM sources WHERE collection=? AND rel_path LIKE ? ORDER BY rel_path',
                            (coll, like)).fetchall()
        if not rows:
            continue
        posters = conn.execute("SELECT count(*) FROM assets WHERE collection=? AND model_id IS NULL AND kind='image'",
                               (coll,)).fetchone()[0]
        print(f'[{coll}]  collection-level / group posters: {posters}')
        tot['posters'] += posters
        for s in rows:
            sid = s['id']
            nm = conn.execute('SELECT count(*) FROM models WHERE source_id=?', (sid,)).fetchone()[0]
            noimg = conn.execute('SELECT count(*) FROM models WHERE source_id=? AND image_count=0',
                                 (sid,)).fetchone()[0]
            imgs = conn.execute("SELECT count(*) FROM assets WHERE source_id=? AND model_id IS NOT NULL "
                                "AND kind='image'", (sid,)).fetchone()[0]
            ps = conn.execute("SELECT count(*) FROM assets WHERE source_id=? AND model_id IS NULL AND kind='image'",
                              (sid,)).fetchone()[0]
            for k, v in zip(('models', 'noimg', 'images'), (nm, noimg, imgs)):
                tot[k] += v
            flag = f'  ERROR: {s["error"]}' if s['error'] else ''
            name = s['rel_path'].split('/', 1)[-1]
            print(f'  {name[:62]:62} {nm:6} {noimg:6} {imgs:5} {ps:6}{flag}')
        if args.tree:
            print_tree(conn, coll)
    print(f'{"TOTAL":64} {tot["models"]:6} {tot["noimg"]:6} {tot["images"]:5} {tot["posters"]:6}')
    return 0


def print_tree(conn, coll):
    groups = {g['id']: g for g in conn.execute('SELECT * FROM groups WHERE collection=?', (coll,))}

    def depth(gid):
        d = 0
        while gid:
            d, gid = d + 1, groups[gid]['parent_id']
        return d

    models = conn.execute('SELECT m.*, a.name AS hero FROM models m LEFT JOIN assets a ON a.id = m.hero_asset_id '
                          'WHERE m.collection=? ORDER BY m.id', (coll,)).fetchall()
    shown = set()
    for m in models:
        chain, gid = [], m['group_id']
        while gid and gid not in shown:
            chain.append(gid)
            gid = groups[gid]['parent_id']
        for g in reversed(chain):
            shown.add(g)
            posters = conn.execute("SELECT count(*) FROM assets WHERE group_id=? AND kind='image'", (g,)).fetchone()[0]
            print('    ' + '  ' * depth(g) + f'[{groups[g]["name"]}]' + (f'  posters={posters}' if posters else ''))
        attrs = json.loads(m['attrs'])
        extra = ' '.join(f'{k}={v}' for k, v in attrs.items())
        print('    ' + '  ' * (depth(m['group_id']) + 1)
              + f'* {m["name"]}  files={m["file_count"]} imgs={m["image_count"]} '
              + f'variants={",".join(json.loads(m["variants"])) or "-"} '
              + (f'parts={len(json.loads(m["part_groups"]))} ' if m['part_groups'] != '[]' else '')
              + (f'hero={m["hero"]} ' if m['hero'] else ('fallback ' if m['hero_fallback_asset_id'] else ''))
              + extra)


def cmd_vtree(args):
    conn = D.connect(args.db)
    root = V.build(conn, files=args.files)
    node = root.find(args.path or '')
    if node is None:
        print(f'no such path: {args.path}', file=sys.stderr)
        return 1
    if args.json:
        print(json.dumps(node.to_dict(), ensure_ascii=False, indent=1))
        return 0
    for depth, n in V.walk(node, max_depth=args.depth):
        r = n.ref
        if n.kind == 'model':
            extra = f'  [{r["file_count"]} files, {r["image_count"]} imgs{"" if r["hero_asset_id"] else ", no image"}]'
        elif n.kind in ('group', 'collection') and r.get('posters'):
            extra = f'  [{r["posters"]} posters]'
        elif n.kind in ('file', 'image'):
            extra = f'  #{r["entry_id"]}'
        else:
            extra = ''
        leaf = n.kind in ('file', 'image')
        print('  ' * depth + n.name + ('' if leaf else '/') + extra)
    return 0


def cmd_extract(args):
    conn = D.connect(args.db)
    row = conn.execute('SELECT e.*, s.kind AS skind, s.path AS spath FROM entries e '
                       'JOIN sources s ON s.id = e.source_id WHERE e.id=?', (args.entry_id,)).fetchone()
    if not row:
        print(f'no entry {args.entry_id}', file=sys.stderr)
        return 1
    os.makedirs(args.out, exist_ok=True)
    dest = os.path.join(args.out, f'{row["id"]}_{os.path.basename(row["path"])}')
    if not os.path.exists(dest):
        S.extract_entry(S.SevenZip(S.find_7z(args.seven_zip)), row['skind'], row['spath'],
                        json.loads(row['chain']), dest)
    print(dest)
    return 0


def main(argv=None):
    ap = argparse.ArgumentParser(prog='stlindex', description=__doc__)
    ap.add_argument('--db', default='stl-index.db', help='SQLite database (default: %(default)s)')
    ap.add_argument('--7z', dest='seven_zip', help='path to 7z executable')
    sub = ap.add_subparsers(dest='cmd', required=True)
    p = sub.add_parser('index', help='scan folder trees and (re)build the model index')
    p.add_argument('roots', nargs='+', help='one or more collection roots')
    p.add_argument('--no-nested', action='store_true', help='do not look inside archives within archives')
    p.add_argument('--rescan', action='store_true', help='ignore cached listings')
    p.add_argument('--only', help='only (re)scan sources whose path contains this text')
    p.set_defaults(func=cmd_index)
    p = sub.add_parser('report', help='per-collection / per-archive summary of the index')
    p.add_argument('--source', help='filter sources by path substring')
    p.add_argument('--tree', action='store_true', help='print groups and models')
    p.set_defaults(func=cmd_report)
    p = sub.add_parser('vtree', help='print the virtual folder tree (volume/collection/group/model/...)')
    p.add_argument('path', nargs='?', help="start at this virtual path, e.g. 'Warhammer 40K Collection Vol.2/04. Primarchs'")
    p.add_argument('--depth', type=int, help='levels to print below the start path')
    p.add_argument('--files', action='store_true', help='expand models into Images/Docs/variants/parts/files')
    p.add_argument('--json', action='store_true', help='print the subtree as JSON')
    p.set_defaults(func=cmd_vtree)
    p = sub.add_parser('extract', help='extract one indexed file (e.g. a preview image)')
    p.add_argument('entry_id', type=int)
    p.add_argument('--out', default='.preview-cache')
    p.set_defaults(func=cmd_extract)
    args = ap.parse_args(argv)
    if hasattr(sys.stdout, 'reconfigure'):
        sys.stdout.reconfigure(encoding='utf-8', errors='replace')
    return args.func(args)
