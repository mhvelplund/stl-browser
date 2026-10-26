"""Virtual folder tree over the resolved tables (see folder-structure.md).

/<volume>/<collection>/<group>/…/<model>/{Images, Docs, <variant>/<part group>/…/<file>}
"""
import json
from collections import defaultdict

DOC_KINDS = ('doc', 'meta')


class Node:
    __slots__ = ('name', 'kind', 'children', 'ref')

    def __init__(self, name, kind, **ref):
        self.name, self.kind, self.children, self.ref = name, kind, [], ref

    def add(self, name, kind, **ref):
        node = Node(name, kind, **ref)
        self.children.append(node)
        return node

    def child(self, name, kind, **ref):
        """Existing child folder with this name, or a new one."""
        for c in self.children:
            if c.name == name and c.kind == kind:
                return c
        return self.add(name, kind, **ref)

    def find(self, path):
        """Node at a '/'-separated virtual path (case-insensitive), or None."""
        node = self
        for part in [p for p in path.strip('/').split('/') if p]:
            node = next((c for c in node.children if c.name.casefold() == part.casefold()), None)
            if node is None:
                return None
        return node

    def to_dict(self):
        d = {'name': self.name, 'kind': self.kind, **self.ref}
        if self.children:
            d['children'] = [c.to_dict() for c in self.children]
        return d


def variant_label(v):
    return ' · '.join(a.capitalize() for a in v.split('+')) if v else 'Default'


def _dedupe(node, alt=None):
    """Give same-named siblings distinct names: alt(child) first (e.g. the edition), else a counter."""
    by_name = defaultdict(list)
    for c in node.children:
        by_name[c.name.casefold()].append(c)
    for clash in (cs for cs in by_name.values() if len(cs) > 1):
        if alt:
            for c in clash:
                extra = alt(c)
                if extra:
                    c.name = f'{c.name} ({extra})'
        seen = defaultdict(int)
        for c in clash:
            seen[c.name.casefold()] += 1
            if seen[c.name.casefold()] > 1:
                c.name = f'{c.name} ({seen[c.name.casefold()]})'


def _sort(node):
    order = {'images': 0, 'docs': 1}
    node.children.sort(key=lambda c: (order.get(c.kind, 2), c.kind in ('file', 'image'), c.name.casefold()))


def _model_alt(c):
    return c.ref.get('edition') or (c.ref.get('raw_name') if c.ref.get('raw_name') != c.name else None)


def build(conn, files=True):
    """The whole virtual tree. With files=False it stops at the models."""
    root = Node('', 'root')
    colls = {}
    for coll in [r[0] for r in conn.execute('SELECT DISTINCT collection FROM groups ORDER BY collection')]:
        vol, _, name = coll.partition('/')
        colls[coll] = root.child(vol, 'volume').add(name or vol, 'collection', collection=coll)

    groups = conn.execute('SELECT g.*, (SELECT min(a.id) FROM assets a WHERE a.group_id = g.id AND '
                          "a.model_id IS NULL AND a.kind = 'image') AS poster, "
                          '(SELECT count(*) FROM assets a WHERE a.group_id = g.id AND a.model_id IS NULL '
                          "AND a.kind = 'image') AS posters FROM groups g ORDER BY g.id").fetchall()
    gnode = {}
    for g in groups:                    # the root group of a collection is the collection folder itself
        if g['parent_id'] is None:
            gnode[g['id']] = colls[g['collection']]
            gnode[g['id']].ref.update(group_id=g['id'], poster_asset_id=g['poster'], posters=g['posters'])
    pending = [g for g in groups if g['parent_id'] is not None]
    while pending:                      # parents may come after their children in id order
        rest = []
        for g in pending:
            if g['parent_id'] in gnode:
                gnode[g['id']] = gnode[g['parent_id']].add(
                    g['name'], 'group', group_id=g['id'], path=g['path'],
                    poster_asset_id=g['poster'], posters=g['posters'])
            else:
                rest.append(g)
        if len(rest) == len(pending):
            raise ValueError(f'groups with missing parents: {[g["id"] for g in rest]}')
        pending = rest

    mfiles, massets = defaultdict(list), defaultdict(list)
    if files:
        for f in conn.execute('SELECT * FROM model_files ORDER BY model_id, id'):
            mfiles[f['model_id']].append(f)
        for a in conn.execute('SELECT * FROM assets WHERE model_id IS NOT NULL ORDER BY model_id, rank, id'):
            massets[a['model_id']].append(a)

    for m in conn.execute('SELECT * FROM models ORDER BY id'):
        node = gnode[m['group_id']].add(
            m['name'], 'model', model_id=m['id'], path=m['path'], raw_name=m['raw_name'], edition=m['edition'],
            hero_asset_id=m['hero_asset_id'] or m['hero_fallback_asset_id'], file_count=m['file_count'],
            image_count=m['image_count'], variants=json.loads(m['variants']))
        if files:
            _fill_model(node, mfiles[m['id']], massets[m['id']])

    def finish(node):
        _dedupe(node, _model_alt if any(c.kind == 'model' for c in node.children) else None)
        _sort(node)
        for c in node.children:
            finish(c)
    finish(root)
    return root


def _fill_model(node, files, assets):
    doc_entries = set()
    if assets:
        images = node.add('Images', 'images')
        for a in assets:
            if a['kind'] == 'image':
                images.add(a['name'], 'image', asset_id=a['id'], entry_id=a['entry_id'], role=a['role'])
    docs = [f for f in files if f['kind'] in DOC_KINDS] + [a for a in assets if a['kind'] != 'image']
    if docs:
        folder = node.add('Docs', 'docs')
        for d in docs:
            if d['entry_id'] not in doc_entries:
                doc_entries.add(d['entry_id'])
                folder.add(d['name'], 'file', entry_id=d['entry_id'], file_kind=d['kind'])
    printable = [f for f in files if f['kind'] not in DOC_KINDS]
    split = len({f['variant'] or '' for f in printable}) > 1
    for f in printable:
        parent = node.child(variant_label(f['variant'] or ''), 'variant', variant=f['variant'] or '') \
            if split else node
        for part in [p for p in (f['part_group'] or '').split('/') if p]:
            parent = parent.child(part, 'part')
        parent.add(f['name'], 'file', entry_id=f['entry_id'], file_kind=f['kind'])


def walk(node, depth=0, max_depth=None):
    """(depth, node) pairs, pre-order, below `node`."""
    for c in node.children:
        yield depth, c
        if max_depth is None or depth + 1 < max_depth:
            yield from walk(c, depth + 1, max_depth)
