import unittest

from stlindex import names as N


class VariantTests(unittest.TestCase):
    def check(self, name, expected):
        self.assertEqual(N.variant_of(name), expected, name)

    def test_plain_variant_folders(self):
        self.check('Supported', ('supported', ''))
        self.check('Unsupported', ('unsupported', ''))
        self.check('Lychee scenes', ('lychee', ''))
        self.check('LYCHEE SCENES', ('lychee', ''))
        self.check('Original Files', ('original', ''))
        self.check('RAW STLS', ('raw', ''))
        self.check('PRE-SUPPORTED STLS', ('supported', ''))
        self.check('STLs', ('stl', ''))
        self.check('OBJ', ('obj', ''))
        self.check('Finished', ('finished', ''))

    def test_prefixed_variants(self):
        self.check('Asgardian Trikers - Supported', ('supported', 'asgardian trikers'))
        self.check('GordonSupported', ('supported', 'gordon'))
        self.check('GordonUnsuported', ('unsupported', 'gordon'))
        self.check('842_Tarantula_Unsupported', ('unsupported', '842 tarantula'))
        self.check('Colonel STL', ('stl', 'colonel'))
        self.check('Colonel Supported STL', ('supported', 'colonel'))
        self.check('Corrupted Guard Walker Pressuported', ('supported', 'corrupted guard walker'))
        self.check('PunchBot_RawOBJs', ('raw', 'punchbot'))
        self.check('PunchBot_SupportedOBJs', ('supported', 'punchbot'))
        self.check('Pressupported - By AiR', ('supported', ''))

    def test_format_and_scale(self):
        self.check('01. FORMAT STL', ('stl', ''))
        self.check('32mm Supported STL', ('supported 32mm', ''))
        self.check('25mm STL', ('stl 25mm', ''))

    def test_leading_variant(self):
        self.check('Presupported Flamers', ('supported', 'flamers'))
        self.check('support_m4_free_brood', ('supported', 'm4 free brood'))
        self.check('Supported Backpacks STL', ('stl supported', 'backpacks'))

    def test_not_variants(self):
        for name in ('Sentinel - By AiR', 'Asgardian Trikers', 'Imperial_Fists_Builder', 'Renders',
                     'Starlight Arcus', 'hands and flamers', 'Wild West Orkz Warbozz 1'):
            self.assertIsNone(N.variant_of(name), name)


class PartGroupTests(unittest.TestCase):
    def test_parts(self):
        for name in ('Arms', 'L Arm', 'Power Fist', 'Empty Helmets', 'TEOTH_Drones', 'asas', 'bracos',
                     'cabeca', 'corpo', 'solder+base', 'LEFT_HAND', 'arms-Empty', 'Backpack'):
            self.assertTrue(N.is_part_group(name), name)

    def test_not_parts(self):
        for name in ('Gloomy Captains', '859_ogre_with_shield_1', 'Character', 'angel',
                     'Apothecary Biologis', 'Immortals', 'hands and flamers'):
            self.assertFalse(N.is_part_group(name), name)


class ImageTests(unittest.TestCase):
    def test_roles(self):
        self.assertEqual(N.image_role('Pose - Front C1.stl.png'), 'thumb')
        self.assertEqual(N.image_role('Assembly-Instructions.jpg'), 'instructions')
        self.assertEqual(N.image_role('WARFORGED_INST_01.png'), 'instructions')
        self.assertEqual(N.image_role('Part_list.jpg'), 'instructions')
        self.assertEqual(N.image_role('Build Guide.png'), 'instructions')
        self.assertEqual(N.image_role('842 Complete Tarantula.jpg'), 'preview')

    def test_generic(self):
        for name in ('1.jpg', 'photo.jpg', 'untitled.822.jpg', 'LsB0hMMwR5E.jpg', '-qAyp6h5X38.jpg',
                     '043ab0ec0976b0963523d742dc22973c_preview_featured.jpg', 'photo_2.jpg'):
            self.assertTrue(N.is_generic_image(name), name)
        for name in ('Destiny Bane (Death Jester)-01.jpg', 'Heldrake.png', 'HL165.jpg'):
            self.assertFalse(N.is_generic_image(name), name)


class ParseNameTests(unittest.TestCase):
    def test_catalog_and_tags(self):
        self.assertEqual(N.parse_name('565_Sorcerer Lintiel and Sphinx'),
                         ('Sorcerer Lintiel and Sphinx', {'catalog_id': '565'}))
        self.assertEqual(N.parse_name('HL165'), ('HL165', {'catalog_id': 'HL165'}))
        self.assertEqual(N.parse_name('Canoptek Spyder (VirusesofDeath) (5E)', designer_tags=['VirusesofDeath']),
                         ('Canoptek Spyder', {'edition': '5E', 'designer': 'VirusesofDeath'}))
        self.assertEqual(N.parse_name('Horus Lupercal By Alexei Konev'),
                         ('Horus Lupercal', {'designer': 'Alexei Konev'}))
        self.assertEqual(N.parse_name('PiperMakes (Format OBJ)')[0], 'PiperMakes')
        self.assertEqual(N.parse_name('El Rhyman, Surrogate Miniatures April Hero Release')[0], 'El Rhyman')


if __name__ == '__main__':
    unittest.main()
