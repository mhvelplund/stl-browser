"""Resolver tests built from real archive layouts in 'Warhammer 40K Collection Vol.1'."""
import unittest

from stlindex.resolve import Entry, resolve


def run(source, paths):
    return resolve(source, [Entry(i + 1, p, 1000 + i) for i, p in enumerate(paths)])


def by_name(res):
    return {m.name: m for m in res.models}


class ResolveTests(unittest.TestCase):
    def test_flat_kit(self):                                         # P1
        res = run('Socratus_Typho', ['Socratus_Typho/Socratus_Typho.jpg', 'Socratus_Typho/Head.stl',
                                     'Socratus_Typho/backpack.stl'])
        self.assertEqual([m.name for m in res.models], ['Socratus Typho'])
        m = res.models[0]
        self.assertEqual(len(m.files), 2)
        self.assertEqual(m.hero.name, 'Socratus_Typho.jpg')

    def test_image_beside_single_folder(self):                       # P2 + wrapper
        res = run('The Daemon Primarch Angron', [
            'The Daemon Primarch Angron/The_Daemon_Primarch_Angron.jpg',
            'The Daemon Primarch Angron/The Daemon Primarch Angron_8/body.stl',
            'The Daemon Primarch Angron/The Daemon Primarch Angron_8/wings.stl'])
        self.assertEqual(len(res.models), 1)
        self.assertEqual(res.models[0].name, 'The Daemon Primarch Angron')
        self.assertEqual(res.models[0].hero.name, 'The_Daemon_Primarch_Angron.jpg')

    def test_model_with_prefixed_variants(self):                     # P3/P4 (Atlan Forge)
        res = run('Atlan Forge', [
            'Atlan Forge/Asgardian Trikers/photo_2.jpg',
            'Atlan Forge/Asgardian Trikers/resize-atlan2-251.jpg',
            'Atlan Forge/Asgardian Trikers/Asgardian Trikers - Supported/bike.stl',
            'Atlan Forge/Asgardian Trikers/Asgardian Trikers - Unsupported/bike.stl',
            'Atlan Forge/Angelic Seraphim/Angelic Seraphim - Supported/a.stl',
            'Atlan Forge/Angelic Seraphim/Angelic Seraphim - Unsupported/a.stl'])
        models = by_name(res)
        self.assertEqual(set(models), {'Asgardian Trikers', 'Angelic Seraphim'})
        trikers = models['Asgardian Trikers']
        self.assertEqual(trikers.variants, ['supported', 'unsupported'])
        self.assertEqual({f.part_group for f in trikers.files}, {''})
        self.assertEqual(len(trikers.assets), 2)
        self.assertEqual(res.groups[0].name, 'Atlan Forge')

    def test_duplicate_nesting_collapses(self):                      # P5 (Blackrock)
        res = run('Blackrock Miniatures', [
            'Blackrock Miniatures/Death Company Redemptor Thicc Boi/Blackrock_Miniatures1.jpg',
            'Blackrock Miniatures/Death Company Redemptor Thicc Boi/Blackrock Miniatures/Supported/a.stl',
            'Blackrock Miniatures/Death Company Redemptor Thicc Boi/Blackrock Miniatures/Unsupported/a.stl',
            'Blackrock Miniatures/Sepulchral Angel/Blackrock Miniatures - Sepulchral Angel/Supported/b.stl',
            'Blackrock Miniatures/Sepulchral Angel/Blackrock Miniatures - Sepulchral Angel/Supported/desktop.ini'])
        models = by_name(res)
        self.assertEqual(set(models), {'Death Company Redemptor Thicc Boi', 'Sepulchral Angel'})
        self.assertEqual(len(models['Death Company Redemptor Thicc Boi'].assets), 1)
        self.assertEqual(len(models['Sepulchral Angel'].files), 1)          # desktop.ini dropped

    def test_variant_split_above_parts(self):                        # P6 + P7 (VOIDWALKERS)
        paths = []
        for v in ('RAW STLS', 'PRE-SUPPORTED STLS'):
            for part in ('ARMS/ARM_01_L.stl', 'HEADS/HEAD_1.stl', 'LEGS/LEGS_07.stl', 'BASE/BASE_40mm.stl'):
                paths.append(f'VOIDWALKERS/{v}/{part}')
        paths += ['VOIDWALKERS/LYCHEE SCENES/scene1.lys', 'VOIDWALKERS/CONTENTS.png', 'VOIDWALKERS/INST_01.png']
        res = run('VOIDWALKERS', paths)
        self.assertEqual(len(res.models), 1)
        m = res.models[0]
        self.assertEqual(m.variants, ['lychee', 'supported', 'unsupported'])
        self.assertEqual({f.part_group for f in m.files}, {'', 'ARMS', 'HEADS', 'LEGS', 'BASE'})
        self.assertEqual(m.hero.name, 'CONTENTS.png')
        self.assertEqual({a.role for a in m.assets}, {'instructions'})

    def test_format_split_station_forge_with_sibling_rule(self):     # P6 + sibling rule
        res = run('Station Forge', [
            'Station Forge/01. FORMAT STL/GrimGuard - Command Force/1.jpg',
            'Station Forge/01. FORMAT STL/GrimGuard - Command Force/Command Force/a.stl',
            'Station Forge/01. FORMAT STL/GrimGuard - Command Force/Command Force Supported/a.stl',
            'Station Forge/02. FORMAT OBJ/GrimGuard - Command Force/Command Force/a.obj',
            'Station Forge/01. FORMAT STL/Corrupted Guard Kill Team/1.jpg',
            'Station Forge/01. FORMAT STL/Corrupted Guard Kill Team/Corrupted/Gunner/a.stl',
            'Station Forge/01. FORMAT STL/Corrupted Guard Kill Team/Corrupted/Leader/a.stl',
            'Station Forge/01. FORMAT STL/Corrupted Guard Kill Team/Corrupted Presupported/Gunner/a.stl',
            'Station Forge/01. FORMAT STL/Corrupted Guard Kill Team/Corrupted Presupported/Leader/a.stl'])
        models = by_name(res)
        # the kill team's figures have no pictures of their own -> the unit is one model
        self.assertEqual(set(models), {'GrimGuard - Command Force', 'Corrupted Guard Kill Team'})
        cf = models['GrimGuard - Command Force']
        self.assertEqual(cf.variants, ['', 'supported', 'unsupported'])     # '' = the OBJ copy
        kt = models['Corrupted Guard Kill Team']
        self.assertEqual(kt.variants, ['supported', 'unsupported'])
        self.assertEqual({f.part_group for f in kt.files}, {'Gunner', 'Leader'})
        self.assertEqual(kt.hero.name, '1.jpg')

    def test_scale_and_format_variants_collapse_to_atoms(self):     # Station Forge Delta Squad
        res = run('Station Forge', [
            'Station Forge/01. FORMAT STL/Delta Squad/1.jpg',
            'Station Forge/01. FORMAT STL/Delta Squad/25mm STL/Figure 1/a.stl',
            'Station Forge/01. FORMAT STL/Delta Squad/25mm Supported STL/Figure 1/a.stl',
            'Station Forge/01. FORMAT STL/Delta Squad/32mm STL/Figure 1/a.stl'])
        self.assertEqual(res.models[0].variants, ['25mm', '32mm', 'supported+25mm'])

    def test_leading_variant_merges_with_sibling(self):              # Station Forge Miscellaneous
        res = run('Station Forge', [
            'Station Forge/Misc/1.jpg',
            'Station Forge/Misc/Miscellaneous/Backpacks/a.stl',
            'Station Forge/Misc/Miscellaneous Supported/Supported Backpacks STL/a.stl',
            'Station Forge/Misc/Miscellaneous/Skull/s.stl'])
        m = res.models[0]
        self.assertEqual({(f.part_group, f.variant) for f in m.files},
                         {('Backpacks', 'unsupported'), ('Backpacks', 'supported'), ('Skull', 'unsupported')})

    def test_gdrive_export_folders_and_parent_name_repeat(self):     # Ghamak Hoplites
        base = 'Ghamak/565-592_Fantasy January 2023/578-590_Hoplites of Light'
        res = run('Ghamak', [
            f'{base}/Hoplites of Light/578 Hoplites of Light A/578 Complete Hoplites of Light A.jpg',
            f'{base}/Hoplites of Light/578 Hoplites of Light A/578 Hoplites of Light A.zip/a.stl',
            f'{base}/presupported/578 Hoplites of Light A.zip/578 Hoplites of Light A-20221219T082318Z-001/'
            '578 Hoplites of Light A/a Lychee.lys',
            f'{base}/Hoplites of Light/579 Hoplites of Light B/579 Hoplites of Light B.zip/b.stl',
            f'{base}/presupported/579 Hoplites of Light B.zip/579 Hoplites of Light B/b.stl'])
        models = by_name(res)
        self.assertEqual(set(models), {'Hoplites of Light A', 'Hoplites of Light B'})
        self.assertEqual(models['Hoplites of Light A'].variants, ['', 'supported'])
        self.assertEqual(models['Hoplites of Light A'].hero.name, '578 Complete Hoplites of Light A.jpg')

    def test_group_images_matched_by_distinctive_tokens(self):       # OPR _Images folder
        base = 'One Page Rules/OPR_Battle Sisters'
        res = run('One Page Rules', [
            f'{base}/_Images/APC 1.png', f'{base}/_Images/Biker Warrior 1.png',
            f'{base}/_Images/Fanatic Superiors 2.png', f'{base}/_Images/Fanatics 1.png',
            f'{base}/One Page Rules - Battle Sisters - APC/a.stl',
            f'{base}/One Page Rules - Battle Sisters - Biker Warriors/a.stl',
            f'{base}/One Page Rules - Battle Sisters - Fanatics/a.stl',
            f'{base}/One Page Rules - Battle Sisters - Fanatics Superior/a.stl'])
        heroes = {m.name: m.hero and m.hero.name for m in res.models}
        self.assertEqual(heroes, {'Battle Sisters - APC': 'APC 1.png',
                                  'Battle Sisters - Biker Warriors': 'Biker Warrior 1.png',
                                  'Battle Sisters - Fanatics': 'Fanatics 1.png',
                                  'Battle Sisters - Fanatics Superior': 'Fanatic Superiors 2.png'})

    def test_format_tag_in_archive_name_is_not_a_variant(self):     # CHAOS MARINE BUILDER
        res = run('CHAOS MARINE BUILDER (Format OBJ)', [
            'CHAOS MARINE BUILDER (Format OBJ)/CHAOS_MARINE_BUILDER.jpg',
            'CHAOS MARINE BUILDER (Format OBJ)/BUILDER 1/Arms/a.obj',
            'CHAOS MARINE BUILDER (Format OBJ)/BUILDER 1/Heads/h.obj',
            'CHAOS MARINE BUILDER (Format OBJ)/BUILDER 2/Bodies/b.stl',
            'CHAOS MARINE BUILDER (Format OBJ)/BUILDER 2/Chainswords/c.stl',
            'CHAOS MARINE BUILDER (Format OBJ)/BUILDER 2/Pistols/p.stl'])
        self.assertEqual([m.name for m in res.models], ['CHAOS MARINE BUILDER'])
        self.assertEqual(res.models[0].variants, [''])
        self.assertEqual(res.models[0].hero.name, 'CHAOS_MARINE_BUILDER.jpg')

    def test_parts_kit_is_one_model(self):                           # P7 (Heldrake, Portuguese)
        res = run('Heldrake', ['Heldrake/image.png', 'Heldrake/asas/a.stl', 'Heldrake/bracos/b.stl',
                               'Heldrake/cabeca/c.stl', 'Heldrake/corpo/d.stl'])
        self.assertEqual([m.name for m in res.models], ['Heldrake'])
        self.assertEqual(res.models[0].hero.name, 'image.png')

    def test_numbered_catalog_image_in_variant(self):                # P9 (Ghamak 842)
        res = run('Ghamak', [
            'Ghamak/842-877_Sci-Fi - 2023.06/842 Complete Tarantula/842_Tarantula_Supported/a.stl',
            'Ghamak/842-877_Sci-Fi - 2023.06/842 Complete Tarantula/842_Tarantula_Unsupported/a.stl',
            'Ghamak/842-877_Sci-Fi - 2023.06/842 Complete Tarantula/842_Tarantula_Unsupported/842 Complete Tarantula.jpg',
            'Ghamak/842-877_Sci-Fi - 2023.06/843 Complete Warp Bringer/843_Warp_bringer_Supported/a.stl'])
        models = by_name(res)
        t = models['Tarantula']
        self.assertEqual(t.attrs['catalog_id'], '842')
        self.assertEqual(t.hero.name, '842 Complete Tarantula.jpg')

    def test_cross_variant_model_merge_with_nested_zip(self):       # P6 + P14 (Ghamak Celestial)
        base = 'Ghamak/565-592_Fantasy January 2023/568-577_Celestial Sagittarii'
        res = run('Ghamak', [
            f'{base}/Celestial Sagittarii/569 Celestial Sagittarii 2 (male)/569 Celestial Sagittarii 2 (male).zip',
            f'{base}/Celestial Sagittarii/569 Celestial Sagittarii 2 (male)/569 Celestial Sagittarii 2 (male).zip/body.stl',
            f'{base}/Celestial Sagittarii presupported/569 Celestial Sagittarii 2 (male) presupported/body.stl',
            f'{base}/Celestial Sagittarii/570 Celestial Sagittarii 3 (female)/body.stl'])
        models = by_name(res)
        self.assertEqual(set(models), {'Celestial Sagittarii 2 (male)', 'Celestial Sagittarii 3 (female)'})
        m = models['Celestial Sagittarii 2 (male)']
        self.assertEqual(m.variants, ['supported', 'unsupported'])
        self.assertEqual(len(m.files), 2)                                    # .zip itself dropped

    def test_group_images_matched_by_name(self):                     # P12 (Imperitum)
        base = 'Imperitum Arts/Void Dancer Troupe Kill Team'
        res = run('Imperitum Arts', [
            f'{base}/Void Dancer Troupe Kill Team (Imperitum Arts) Destiny Bane (Death Jester).jpg',
            f'{base}/Void Dancer Troupe Kill Team (Imperitum Arts) Faceless Triumph (Solitaire).jpg',
            f'{base}/Destiny Bane (Death Jester)/Destiny Bane (Death Jester)/a.stl',
            f'{base}/Faceless Triumph (Solitaire)/Lychee Files/a.lys',
            f'{base}/Faceless Triumph (Solitaire)/Supported Files/a.stl',
            f'{base}/Faceless Triumph (Solitaire)/Mountain 3D Info Sheet.pdf'])
        models = by_name(res)
        self.assertEqual(models['Destiny Bane (Death Jester)'].hero.match, 'name')
        self.assertEqual(models['Faceless Triumph (Solitaire)'].hero.match, 'name')
        self.assertEqual(models['Faceless Triumph (Solitaire)'].variants, ['lychee', 'supported'])

    def test_render_folders_and_per_file_thumbs(self):               # P10 + P11
        res = run('PuppetsWar Miniatures', [
            'PuppetsWar Miniatures/Puppetswar_Feb23_/Puppetswar_Feb23.jpg',
            'PuppetsWar Miniatures/Puppetswar_Feb23_/Bushi/Renders/bushi (1).jpg',
            'PuppetsWar Miniatures/Puppetswar_Feb23_/Bushi/Supported/a.stl',
            'PuppetsWar Miniatures/Puppetswar_Feb23_/Bushi/Base/base.stl',
            'PuppetsWar Miniatures/Puppetswar_Feb23_/Knight/Supported/Pose A.stl',
            'PuppetsWar Miniatures/Puppetswar_Feb23_/Knight/Supported/Pose A.stl.png',
            'PuppetsWar Miniatures/New_Enforcer/supported/a.stl'])
        models = by_name(res)
        self.assertEqual(models['Bushi'].hero.name, 'bushi (1).jpg')
        self.assertEqual({f.part_group for f in models['Bushi'].files}, {'', 'Base'})
        self.assertEqual(models['Knight'].assets[0].role, 'thumb')
        posters = [a for a in res.assets if a.role == 'poster']
        self.assertEqual(posters[0].group.name, 'Puppetswar Feb23')

    def test_designer_tags_and_edition(self):                        # P15 (Necrons)
        res = run('Necron Units - Ellit', [
            "Necron Units - Ellit/Canoptek Spyder (VirusesofDeath) (5E)/a.stl",
            "Necron Units - Ellit/Cryptothralls (VirusesofDeath)/a.stl",
            "Necron Units - Ellit/Canoptek Reanimator (unidentified)/a.stl"])
        m = by_name(res)['Canoptek Spyder']
        self.assertEqual(m.attrs, {'edition': '5E', 'designer': 'VirusesofDeath'})

    def test_unexpanded_nested_archive_counts_as_model(self):        # P14 (OPR without --nested)
        res = run('One Page Rules', [
            'One Page Rules/OPR_Battle Sisters/One Page Rules - Battle Sisters - APC/BS - APC - US.7z',
            'One Page Rules/OPR_Battle Sisters/One Page Rules - Battle Sisters - Tank/BS - Tank - US.7z'])
        self.assertEqual(len(res.models), 2)
        self.assertEqual(res.models[0].files[0].kind, 'archive')


    def test_catalog_id_adopted_from_own_pictures(self):            # Ghamak Wild West Orkz
        base = 'Ghamak/Wild West Orkz/442-447_Wild West Orkz Bullboyz Squad 1'
        res = run('Ghamak', [
            f'{base}/resize-442-complete-bullboyz-1-1.jpg',
            f'{base}/resize-443-complete-bullboyz-2-1.jpg',
            f'{base}/Bullboyz 1/442 Complete bullboyz 1.png', f'{base}/Bullboyz 1/a.stl',
            f'{base}/Bullboyz 2/443 complete bullboyz 2.png', f'{base}/Bullboyz 2/a.stl'])
        m = by_name(res)
        self.assertEqual(m['Bullboyz 1'].attrs['catalog_id'], '442')
        self.assertEqual({a.name for a in m['Bullboyz 1'].assets},
                         {'442 Complete bullboyz 1.png', 'resize-442-complete-bullboyz-1-1.jpg'})

    def test_kit_prefixed_part_folders(self):                        # PiperMakes Eclipse Frame
        base = 'PiperMakes/Eclipse Frame - TEOTH Version (Format OBJ)'
        res = run('PiperMakes', [f'{base}/Render1.png'] + [
            f'{base}/TEOTH_{p}/x.obj' for p in ('Drones', 'Heads', 'Hands', 'Legs_DiscKnees', 'Main', 'Weapons')
        ] + ['PiperMakes/Punchbot/a.stl'])
        self.assertEqual(by_name(res)['Eclipse Frame - TEOTH Version'].hero.name, 'Render1.png')
        self.assertEqual(len(res.models), 2)

    def test_release_poster_goes_to_matching_subgroup(self):         # Ghamak Fantasy January
        base = 'Ghamak/565-592_Fantasy January 2023'
        res = run('Ghamak', [
            f'{base}/Celestial Sagittarii presupported.jpg',
            f'{base}/568-577_Celestial Sagittarii/569 Celestial Sagittarii 2 (male)/a.stl',
            f'{base}/568-577_Celestial Sagittarii/569 Celestial Sagittarii 2 (male)/569 Complete.jpg',
            f'{base}/568-577_Celestial Sagittarii/570 Celestial Sagittarii 3 (female)/a.stl',
            f'{base}/566_Starlight Arcus/a.stl'])
        poster = [a for a in res.assets if a.role == 'poster'][0]
        self.assertEqual(poster.group.name, 'Celestial Sagittarii')

    def test_common_word_tag_is_not_a_designer(self):                # PiperMakes drones
        res = run('PiperMakes', ['P/K/Gun Drone 00 (Drone)/a.stl', 'P/K/Recon Drone (Drone)/a.stl',
                                 'P/K/Medic/a.stl'])
        self.assertTrue(all('designer' not in m.attrs for m in res.models))


class CollectionLevelTests(unittest.TestCase):
    """Vol.2: archives resolved as folders inside their collection folder (disk level)."""

    def test_vol1_archive_inside_collection_resolves_as_before(self):
        res = run('01. Studio Collection', [
            'Atlan Forge.rar/Atlan Forge/Asgardian Trikers/photo_2.jpg',
            'Atlan Forge.rar/Atlan Forge/Asgardian Trikers/Asgardian Trikers - Supported/bike.stl',
            'Atlan Forge.rar/Atlan Forge/Asgardian Trikers/Asgardian Trikers - Unsupported/bike.stl',
            'Atlan Forge.rar/Atlan Forge/Angelic Seraphim/Angelic Seraphim - Supported/a.stl',
            'Heldrake.rar/Heldrake/image.png', 'Heldrake.rar/Heldrake/asas/a.stl',
            'Heldrake.rar/Heldrake/corpo/d.stl', 'Heldrake.rar/Heldrake/cabeca/c.stl'])
        models = by_name(res)
        self.assertEqual(set(models), {'Asgardian Trikers', 'Angelic Seraphim', 'Heldrake'})
        self.assertEqual(models['Asgardian Trikers'].variants, ['supported', 'unsupported'])
        self.assertEqual(models['Heldrake'].hero.name, 'image.png')

    def test_sidecar_images_on_disk(self):                           # D1 (02. Model Collection)
        res = run('02. Model Collection', [
            '40k Squat STL.jpg', '40k Squat STL.rar/squat.stl',
            '40K_Empress_fina.jpg', '40K_Empress_final.rar/empress/body.stl',
            'Eternal Eagle (1).jpg', 'Eternal Eagle (2).jpg', 'Eternal Eagle.rar/Eternal Eagle/a.stl',
            'Cyber_Viking.jpg', 'Cyber_Viking1.jpg', 'Cyber_Viking.rar/a.stl',
            'BA DC Heads and Weapons1.jpg', 'BA DC Heads and Weapons.rar/BA/heads/h.stl',
            'Adeptus_Mechanicus.zip/Adeptus/a.stl'])
        imgs = {m.name: sorted(a.name for a in m.assets) for m in res.models}
        self.assertEqual(imgs['40k Squat'], ['40k Squat STL.jpg'])
        self.assertEqual(imgs['40K Empress final'], ['40K_Empress_fina.jpg'])
        self.assertEqual(imgs['Eternal Eagle'], ['Eternal Eagle (1).jpg', 'Eternal Eagle (2).jpg'])
        self.assertEqual(imgs['Cyber Viking'], ['Cyber_Viking.jpg', 'Cyber_Viking1.jpg'])
        self.assertEqual(imgs['BA DC Heads and Weapons'], ['BA DC Heads and Weapons1.jpg'])
        self.assertTrue(all(a.match == 'sidecar' for m in res.models for a in m.assets))

    def test_sidecars_inside_studio_archive(self):                   # D1 (01. Studio Collection)
        base = 'PuppetsWar Miniatures.rar/PuppetsWar Miniatures'
        res = run('01. Studio Collection', [
            f'{base}/Bug_Warrior.jpg', f'{base}/Bug_Warriors.rar/bug/a.stl',
            f'{base}/Bushi_Heavy_Gunners_Set_B.jpg', f'{base}/Bushi_Heavy_Gunners_Set_B.rar/a.stl',
            f'{base}/Bushi_Heavy_Gunners_Set_C.jpg', f'{base}/Bushi_Heavy_Gunners_Set_C.rar/a.stl',
            'Forge World.rar/Forge World/Acasatus_Knight_Porphyrion-1_1.webp',
            'Forge World.rar/Forge World/Acasatus_Knight_Porphyrion-1_3.jpg',
            'Forge World.rar/Forge World/Acasatus_Knight_Porphyrion-1.zip/k.stl',
            'Forge World.rar/Forge World/Imotekh.zip/i.stl'])
        imgs = {m.name: sorted(a.name for a in m.assets) for m in res.models}
        self.assertEqual(imgs['Bug Warriors'], ['Bug_Warrior.jpg'])
        self.assertEqual(imgs['Bushi Heavy Gunners Set C'], ['Bushi_Heavy_Gunners_Set_C.jpg'])
        self.assertEqual(imgs['Acasatus Knight Porphyrion-1'],
                         ['Acasatus_Knight_Porphyrion-1_1.webp', 'Acasatus_Knight_Porphyrion-1_3.jpg'])

    def test_disk_folder_of_variant_archives_is_one_model(self):     # D2
        res = run('03. ОРКИ', [
            'Jetpack_boys_pk1/FMHM7pZW0VU.jpg', 'Jetpack_boys_pk1/Jetpack_boys_pk1 2.jpg',
            'Jetpack_boys_pk1/Jetpack_boys_pk1_unsupported.zip/Jetpack_boys/a.stl',
            'Jetpack_boys_pk1/Unsupported.rar/b.stl',
            "MrModulork's Fire Ladz/kVkW-BVEjM0.jpg",
            "MrModulork's Fire Ladz/Unsupported.zip/Unsupported/a.stl",
            "MrModulork's Fire Ladz/Unsupported_1.zip/x/b.stl",
            'Primal_Boss.jpg', 'Primal_Boss.rar/Primal_Boss/p.stl'])
        models = by_name(res)
        self.assertEqual(set(models), {'Jetpack boys pk1', "MrModulork's Fire Ladz", 'Primal Boss'})
        self.assertEqual(models['Jetpack boys pk1'].variants, ['unsupported'])
        self.assertEqual(len(models['Jetpack boys pk1'].assets), 2)
        self.assertEqual(models["MrModulork's Fire Ladz"].variants, ['unsupported'])

    def test_variant_archives_with_abbreviations(self):              # D2 (Wargame Exclusive)
        base = 'Wargame Exclusive.rar/Wargame Exclusive'
        res = run('01. Studio Collection', [
            f'{base}/RoP_AGG/RoP_AGG.jpg', f'{base}/RoP_AGG/RoP_AGG_STL.zip/agg.stl',
            f'{base}/RoP_AGG/RoP_AGG_STL_SUP.zip/agg_sup.stl',
            f'{base}/RoP_CCW/RoP_CCW.jpg', f'{base}/RoP_CCW/RoP_CCW_STL.zip/ccw.stl'])
        models = by_name(res)
        self.assertEqual(set(models), {'RoP AGG', 'RoP CCW'})
        self.assertEqual(models['RoP AGG'].variants, ['', 'supported'])
        self.assertEqual(models['RoP AGG'].hero.name, 'RoP_AGG.jpg')

    def test_archive_with_pictures_named_after_inner_models(self):   # D3 (04. Primarchs)
        res = run('04. Primarchs', [
            'Angron.jpg', 'Leman Rus.jpg', 'Mortarion.jpg',
            'Primarchs.zip/Primarchs/Angron/a.stl', 'Primarchs.zip/Primarchs/Leman Russ/l.stl',
            'Primarchs.zip/Primarchs/Mortarion/m.stl'])
        heroes = {m.name: m.hero and m.hero.name for m in res.models}
        self.assertEqual(heroes, {'Angron': 'Angron.jpg', 'Leman Russ': 'Leman Rus.jpg',
                                  'Mortarion': 'Mortarion.jpg'})

    def test_hollow_and_solid_casts_are_variants(self):              # D8 (Primarchs/Sanguinus)
        res = run('04. Primarchs', [
            'Sanguinus.jpg', 'Primarchs.zip/Primarchs/Sanguinus/Hueco/body.stl',
            'Primarchs.zip/Primarchs/Sanguinus/Normal/body.stl', 'Primarchs.zip/Primarchs/Angron/a.stl'])
        m = by_name(res)['Sanguinus']
        self.assertEqual(m.variants, ['hollow', 'solid'])
        self.assertEqual(m.hero.name, 'Sanguinus.jpg')

    def test_folder_poster_one_level_up(self):                       # D4 (TYRANIDS)
        res = run('02. Model Collection', [
            'TYRANIDS/VOD_-_TYRANIDS_Complete.jpg',
            'TYRANIDS/VOD_-_TYRANIDS_Complete/Biovore.rar/Biovore/a.stl',
            'TYRANIDS/VOD_-_TYRANIDS_Complete/Biovore.rar/Biovore/biovore.jpg',
            'TYRANIDS/VOD_-_TYRANIDS_Complete/Broodlord.rar/Broodlord/b.stl',
            'TYRANIDS/VOD_-_TYRANIDS_Complete/Broodlord.rar/Broodlord/broodlord.png',
            'Krieg Horse.jpg', 'Krieg Horse.rar/h.stl'])
        poster = [a for a in res.assets if a.name == 'VOD_-_TYRANIDS_Complete.jpg'][0]
        self.assertEqual(poster.role, 'poster')
        self.assertIs(poster.group, by_name(res)['Broodlord'].group)       # the units' group
        self.assertEqual(by_name(res)['Biovore'].hero.name, 'biovore.jpg')

    def test_thingiverse_exports_and_shared_prefix(self):            # D5 + D6 (07. Terrain)
        names = ['Ancient+Alien+Pylon', 'Modular_Trench', 'Festungsmauer', 'Eagle_Shrine', 'Barricades']
        paths = []
        for n in names:
            z = f'Warhammer 40k - Wargames Terrain - {n}.zip'
            paths += [f'{z}/{n}/LICENSE.txt', f'{z}/{n}/README.txt', f'{z}/{n}/attribution_card.html',
                      f'{z}/{n}/files/part_a.stl', f'{z}/{n}/files/part_b.stl']
        res = run('07. Warhammer 40k Terrain', paths)
        self.assertEqual(sorted(m.name for m in res.models),
                         ['Ancient Alien Pylon', 'Barricades', 'Eagle Shrine', 'Festungsmauer', 'Modular Trench'])
        self.assertTrue(all(f.kind == 'model' for m in res.models for f in m.files))
        self.assertEqual(res.assets, [])                                  # meta files aren't docs


if __name__ == '__main__':
    unittest.main()
