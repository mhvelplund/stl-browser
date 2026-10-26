import unittest

from stlindex import db as D
from stlindex import vtree as V

COLL = 'Warhammer 40K Collection Vol.2/06. Necrons STC'


def make_db():
    conn = D.connect(':memory:')
    conn.execute("INSERT INTO sources(id, kind, path) VALUES (1, 'archive', 'x.rar')")
    for i in range(1, 30):
        conn.execute("INSERT INTO entries(id, source_id, path, chain) VALUES (?, 1, ?, '[]')", (i, f'f{i}'))
    # child group before its parent in id order
    conn.executemany('INSERT INTO groups(id, collection, parent_id, name, path) VALUES (?, ?, ?, ?, ?)', [
        (1, COLL, None, '06. Necrons STC', ''),
        (2, COLL, 3, 'Sub', 'Elites/Sub'),
        (3, COLL, 1, 'Elites', 'Elites'),
    ])
    model = ('INSERT INTO models(id, collection, group_id, name, raw_name, path, edition, variants, part_groups, '
             "file_count, image_count, hero_asset_id, hero_fallback_asset_id, attrs) "
             "VALUES (?, ?, ?, ?, ?, ?, ?, '[]', '[]', 0, 0, ?, ?, '{}')")
    conn.executemany(model, [
        (1, COLL, 3, 'Illuminor Szeras', 'Illuminor Szeras (5E)', 'Elites/a', '5E', 1, None),
        (2, COLL, 3, 'Illuminor Szeras', 'Illuminor Szeras (8E)', 'Elites/b', '8E', None, 9),
        (3, COLL, 2, 'Acanthrites', 'Acanthrites', 'Elites/Sub/c', None, None, None),
    ])
    conn.executemany('INSERT INTO model_files(model_id, entry_id, name, kind, part_group, variant) '
                     'VALUES (?, ?, ?, ?, ?, ?)', [
        (1, 1, 'body.stl', 'model', '', ''),
        (1, 2, 'README.pdf', 'doc', '', ''),
        (3, 3, 'head.stl', 'model', '', ''),
        (3, 4, 'head.stl', 'model', 'Kit/Heads', 'supported'),
        (3, 5, 'head.lys', 'scene', 'Kit/Heads', 'supported+lychee'),
    ])
    conn.executemany('INSERT INTO assets(id, collection, model_id, group_id, entry_id, name, kind, role, rank) '
                     'VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)', [
        (1, COLL, 1, 3, 10, 'b.jpg', 'image', 'preview', 1),
        (2, COLL, 1, 3, 11, 'a.jpg', 'image', 'preview', 0),
        (3, COLL, 1, 3, 2, 'README.pdf', 'doc', 'doc', 2),
        (9, COLL, None, 3, 12, 'poster.jpg', 'image', 'poster', 0),
    ])
    return conn


class VTreeTests(unittest.TestCase):
    def setUp(self):
        conn = make_db()
        self.addCleanup(conn.close)
        self.root = V.build(conn)

    def test_volume_and_collection_levels(self):
        coll = self.root.find('Warhammer 40K Collection Vol.2/06. Necrons STC')
        self.assertEqual(coll.kind, 'collection')
        self.assertEqual([c.name for c in coll.children], ['Elites'])     # root group is not repeated

    def test_child_group_listed_before_parent_is_attached_once(self):
        elites = self.root.find(f'{COLL}/Elites')
        self.assertEqual([c.name for c in elites.children if c.kind == 'group'], ['Sub'])
        self.assertEqual(elites.ref['posters'], 1)

    def test_name_clash_uses_edition(self):
        elites = self.root.find(f'{COLL}/Elites')
        models = [c for c in elites.children if c.kind == 'model']
        self.assertEqual([m.name for m in models], ['Illuminor Szeras (5E)', 'Illuminor Szeras (8E)'])
        self.assertEqual(models[1].ref['hero_asset_id'], 9)               # fallback poster

    def test_single_variant_model_has_images_docs_and_flat_files(self):
        m = self.root.find(f'{COLL}/Elites/Illuminor Szeras (5E)')
        self.assertEqual([(c.kind, c.name) for c in m.children],
                         [('images', 'Images'), ('docs', 'Docs'), ('file', 'body.stl')])
        self.assertEqual([c.name for c in m.children[0].children], ['a.jpg', 'b.jpg'])   # rank order
        self.assertEqual(len(m.children[1].children), 1)                 # doc in both tables shown once

    def test_variants_then_nested_part_groups(self):
        m = self.root.find(f'{COLL}/Elites/Sub/Acanthrites')
        self.assertEqual([c.name for c in m.children], ['Default', 'Supported', 'Supported · Lychee'])
        heads = self.root.find(f'{COLL}/Elites/Sub/Acanthrites/supported/kit/heads')
        self.assertEqual([(c.name, c.ref['entry_id']) for c in heads.children], [('head.stl', 4)])

    def test_files_false_stops_at_models(self):
        conn = make_db()
        self.addCleanup(conn.close)
        root = V.build(conn, files=False)
        self.assertEqual(root.find(f'{COLL}/Elites/Sub/Acanthrites').children, [])


if __name__ == '__main__':
    unittest.main()
