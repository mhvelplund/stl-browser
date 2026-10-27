//! Port of `prototype/tests/test_resolve.py`: resolver tests built from real archive layouts.
use std::collections::{BTreeSet, HashMap};

use stl_core::names::{Attrs, Kind};
use stl_core::resolve::{AssetRole, Entry, Match, Resolution, resolve};

fn run(source: &str, paths: &[&str]) -> Resolution {
    let entries: Vec<Entry> = paths
        .iter()
        .enumerate()
        .map(|(i, p)| Entry { id: i as i64 + 1, path: p.to_string(), size: 1000 + i as u64 })
        .collect();
    resolve(source, &entries)
}

fn by_name(res: &Resolution) -> HashMap<String, usize> {
    res.models.iter().enumerate().map(|(i, m)| (m.name.clone(), i)).collect()
}

fn names(res: &Resolution) -> BTreeSet<String> {
    res.models.iter().map(|m| m.name.clone()).collect()
}

fn set(items: &[&str]) -> BTreeSet<String> {
    items.iter().map(|s| s.to_string()).collect()
}

fn strs(items: &[&str]) -> Vec<String> {
    items.iter().map(|s| s.to_string()).collect()
}

// ---------------------------------------------------------------- structure (Task 2)

#[test]
fn scale_and_format_variants_collapse_to_atoms() {
    // Station Forge Delta Squad
    let res = run(
        "Station Forge",
        &[
            "Station Forge/01. FORMAT STL/Delta Squad/1.jpg",
            "Station Forge/01. FORMAT STL/Delta Squad/25mm STL/Figure 1/a.stl",
            "Station Forge/01. FORMAT STL/Delta Squad/25mm Supported STL/Figure 1/a.stl",
            "Station Forge/01. FORMAT STL/Delta Squad/32mm STL/Figure 1/a.stl",
        ],
    );
    assert_eq!(res.models[0].variants(), strs(&["25mm", "32mm", "supported+25mm"]));
}

#[test]
fn leading_variant_merges_with_sibling() {
    // Station Forge Miscellaneous
    let res = run(
        "Station Forge",
        &[
            "Station Forge/Misc/1.jpg",
            "Station Forge/Misc/Miscellaneous/Backpacks/a.stl",
            "Station Forge/Misc/Miscellaneous Supported/Supported Backpacks STL/a.stl",
            "Station Forge/Misc/Miscellaneous/Skull/s.stl",
        ],
    );
    let got: BTreeSet<(String, String)> =
        res.models[0].files.iter().map(|f| (f.part_group.clone(), f.variant.clone())).collect();
    let want: BTreeSet<(String, String)> = [("Backpacks", "unsupported"), ("Backpacks", "supported"), ("Skull", "unsupported")]
        .iter()
        .map(|(a, b)| (a.to_string(), b.to_string()))
        .collect();
    assert_eq!(got, want);
}

#[test]
fn cross_variant_model_merge_with_nested_zip() {
    // P6 + P14 (Ghamak Celestial)
    let base = "Ghamak/565-592_Fantasy January 2023/568-577_Celestial Sagittarii";
    let paths = [
        format!("{base}/Celestial Sagittarii/569 Celestial Sagittarii 2 (male)/569 Celestial Sagittarii 2 (male).zip"),
        format!("{base}/Celestial Sagittarii/569 Celestial Sagittarii 2 (male)/569 Celestial Sagittarii 2 (male).zip/body.stl"),
        format!("{base}/Celestial Sagittarii presupported/569 Celestial Sagittarii 2 (male) presupported/body.stl"),
        format!("{base}/Celestial Sagittarii/570 Celestial Sagittarii 3 (female)/body.stl"),
    ];
    let res = run("Ghamak", &paths.iter().map(String::as_str).collect::<Vec<_>>());
    assert_eq!(names(&res), set(&["Celestial Sagittarii 2 (male)", "Celestial Sagittarii 3 (female)"]));
    let m = &res.models[by_name(&res)["Celestial Sagittarii 2 (male)"]];
    assert_eq!(m.variants(), strs(&["supported", "unsupported"]));
    assert_eq!(m.files.len(), 2); // .zip itself dropped
}

#[test]
fn designer_tags_and_edition() {
    // P15 (Necrons)
    let res = run(
        "Necron Units - Ellit",
        &[
            "Necron Units - Ellit/Canoptek Spyder (VirusesofDeath) (5E)/a.stl",
            "Necron Units - Ellit/Cryptothralls (VirusesofDeath)/a.stl",
            "Necron Units - Ellit/Canoptek Reanimator (unidentified)/a.stl",
        ],
    );
    let m = &res.models[by_name(&res)["Canoptek Spyder"]];
    assert_eq!(
        m.attrs,
        Attrs { edition: Some("5E".into()), designer: Some("VirusesofDeath".into()), ..Default::default() }
    );
}

#[test]
fn unexpanded_nested_archive_counts_as_model() {
    // P14 (OPR without --nested)
    let res = run(
        "One Page Rules",
        &[
            "One Page Rules/OPR_Battle Sisters/One Page Rules - Battle Sisters - APC/BS - APC - US.7z",
            "One Page Rules/OPR_Battle Sisters/One Page Rules - Battle Sisters - Tank/BS - Tank - US.7z",
        ],
    );
    assert_eq!(res.models.len(), 2);
    assert_eq!(res.models[0].files[0].kind, Kind::Archive);
}

#[test]
fn common_word_tag_is_not_a_designer() {
    // PiperMakes drones
    let res = run("PiperMakes", &["P/K/Gun Drone 00 (Drone)/a.stl", "P/K/Recon Drone (Drone)/a.stl", "P/K/Medic/a.stl"]);
    assert!(res.models.iter().all(|m| m.attrs.designer.is_none()));
}

#[test]
fn thingiverse_exports_and_shared_prefix() {
    // D5 + D6 (07. Terrain)
    let mut paths = Vec::new();
    for n in ["Ancient+Alien+Pylon", "Modular_Trench", "Festungsmauer", "Eagle_Shrine", "Barricades"] {
        let z = format!("Warhammer 40k - Wargames Terrain - {n}.zip");
        for f in ["LICENSE.txt", "README.txt", "attribution_card.html", "files/part_a.stl", "files/part_b.stl"] {
            paths.push(format!("{z}/{n}/{f}"));
        }
    }
    let res = run("07. Warhammer 40k Terrain", &paths.iter().map(String::as_str).collect::<Vec<_>>());
    let mut got: Vec<String> = res.models.iter().map(|m| m.name.clone()).collect();
    got.sort();
    assert_eq!(got, strs(&["Ancient Alien Pylon", "Barricades", "Eagle Shrine", "Festungsmauer", "Modular Trench"]));
    assert!(res.models.iter().all(|m| m.files.iter().all(|f| f.kind == Kind::Model)));
    assert!(res.assets.is_empty()); // meta files aren't docs
}

// ---------------------------------------------------------------- assets and units (Task 3)

fn hero(res: &Resolution, name: &str) -> Option<String> {
    res.hero(by_name(res)[name]).map(|a| a.name.clone())
}

fn heroes(res: &Resolution) -> HashMap<String, Option<String>> {
    (0..res.models.len()).map(|i| (res.models[i].name.clone(), res.hero(i).map(|a| a.name.clone()))).collect()
}

fn imgs(res: &Resolution) -> HashMap<String, Vec<String>> {
    (0..res.models.len())
        .map(|i| {
            let mut v: Vec<String> = res.model_assets(i).iter().map(|a| a.name.clone()).collect();
            v.sort();
            (res.models[i].name.clone(), v)
        })
        .collect()
}

fn owned(paths: &[String]) -> Vec<&str> {
    paths.iter().map(String::as_str).collect()
}

fn under(base: &str, rel: &[&str]) -> Vec<String> {
    rel.iter().map(|p| format!("{base}/{p}")).collect()
}

fn part_groups(res: &Resolution, m: usize) -> BTreeSet<String> {
    res.models[m].files.iter().map(|f| f.part_group.clone()).collect()
}

fn heroes_of(pairs: &[(&str, &str)]) -> HashMap<String, Option<String>> {
    pairs.iter().map(|(m, h)| (m.to_string(), Some(h.to_string()))).collect()
}

#[test]
fn flat_kit() {
    // P1
    let res = run("Socratus_Typho", &["Socratus_Typho/Socratus_Typho.jpg", "Socratus_Typho/Head.stl", "Socratus_Typho/backpack.stl"]);
    assert_eq!(res.models.iter().map(|m| m.name.as_str()).collect::<Vec<_>>(), ["Socratus Typho"]);
    assert_eq!(res.models[0].files.len(), 2);
    assert_eq!(res.hero(0).unwrap().name, "Socratus_Typho.jpg");
}

#[test]
fn image_beside_single_folder() {
    // P2 + wrapper
    let res = run(
        "The Daemon Primarch Angron",
        &[
            "The Daemon Primarch Angron/The_Daemon_Primarch_Angron.jpg",
            "The Daemon Primarch Angron/The Daemon Primarch Angron_8/body.stl",
            "The Daemon Primarch Angron/The Daemon Primarch Angron_8/wings.stl",
        ],
    );
    assert_eq!(res.models.len(), 1);
    assert_eq!(res.models[0].name, "The Daemon Primarch Angron");
    assert_eq!(res.hero(0).unwrap().name, "The_Daemon_Primarch_Angron.jpg");
}

#[test]
fn model_with_prefixed_variants() {
    // P3/P4 (Atlan Forge)
    let res = run(
        "Atlan Forge",
        &[
            "Atlan Forge/Asgardian Trikers/photo_2.jpg",
            "Atlan Forge/Asgardian Trikers/resize-atlan2-251.jpg",
            "Atlan Forge/Asgardian Trikers/Asgardian Trikers - Supported/bike.stl",
            "Atlan Forge/Asgardian Trikers/Asgardian Trikers - Unsupported/bike.stl",
            "Atlan Forge/Angelic Seraphim/Angelic Seraphim - Supported/a.stl",
            "Atlan Forge/Angelic Seraphim/Angelic Seraphim - Unsupported/a.stl",
        ],
    );
    assert_eq!(names(&res), set(&["Asgardian Trikers", "Angelic Seraphim"]));
    let t = by_name(&res)["Asgardian Trikers"];
    assert_eq!(res.models[t].variants(), strs(&["supported", "unsupported"]));
    assert_eq!(part_groups(&res, t), set(&[""]));
    assert_eq!(res.models[t].assets.len(), 2);
    assert_eq!(res.groups[0].name, "Atlan Forge");
}

#[test]
fn duplicate_nesting_collapses() {
    // P5 (Blackrock)
    let res = run(
        "Blackrock Miniatures",
        &[
            "Blackrock Miniatures/Death Company Redemptor Thicc Boi/Blackrock_Miniatures1.jpg",
            "Blackrock Miniatures/Death Company Redemptor Thicc Boi/Blackrock Miniatures/Supported/a.stl",
            "Blackrock Miniatures/Death Company Redemptor Thicc Boi/Blackrock Miniatures/Unsupported/a.stl",
            "Blackrock Miniatures/Sepulchral Angel/Blackrock Miniatures - Sepulchral Angel/Supported/b.stl",
            "Blackrock Miniatures/Sepulchral Angel/Blackrock Miniatures - Sepulchral Angel/Supported/desktop.ini",
        ],
    );
    assert_eq!(names(&res), set(&["Death Company Redemptor Thicc Boi", "Sepulchral Angel"]));
    let m = by_name(&res);
    assert_eq!(res.models[m["Death Company Redemptor Thicc Boi"]].assets.len(), 1);
    assert_eq!(res.models[m["Sepulchral Angel"]].files.len(), 1); // desktop.ini dropped
}

#[test]
fn variant_split_above_parts() {
    // P6 + P7 (VOIDWALKERS)
    let mut paths = Vec::new();
    for v in ["RAW STLS", "PRE-SUPPORTED STLS"] {
        for part in ["ARMS/ARM_01_L.stl", "HEADS/HEAD_1.stl", "LEGS/LEGS_07.stl", "BASE/BASE_40mm.stl"] {
            paths.push(format!("VOIDWALKERS/{v}/{part}"));
        }
    }
    paths.extend(["VOIDWALKERS/LYCHEE SCENES/scene1.lys", "VOIDWALKERS/CONTENTS.png", "VOIDWALKERS/INST_01.png"].map(String::from));
    let res = run("VOIDWALKERS", &owned(&paths));
    assert_eq!(res.models.len(), 1);
    assert_eq!(res.models[0].variants(), strs(&["lychee", "supported", "unsupported"]));
    assert_eq!(part_groups(&res, 0), set(&["", "ARMS", "HEADS", "LEGS", "BASE"]));
    assert_eq!(res.hero(0).unwrap().name, "CONTENTS.png");
    assert!(res.model_assets(0).iter().all(|a| a.role == AssetRole::Instructions));
}

#[test]
fn format_split_station_forge_with_sibling_rule() {
    // P6 + sibling rule
    let res = run(
        "Station Forge",
        &[
            "Station Forge/01. FORMAT STL/GrimGuard - Command Force/1.jpg",
            "Station Forge/01. FORMAT STL/GrimGuard - Command Force/Command Force/a.stl",
            "Station Forge/01. FORMAT STL/GrimGuard - Command Force/Command Force Supported/a.stl",
            "Station Forge/02. FORMAT OBJ/GrimGuard - Command Force/Command Force/a.obj",
            "Station Forge/01. FORMAT STL/Corrupted Guard Kill Team/1.jpg",
            "Station Forge/01. FORMAT STL/Corrupted Guard Kill Team/Corrupted/Gunner/a.stl",
            "Station Forge/01. FORMAT STL/Corrupted Guard Kill Team/Corrupted/Leader/a.stl",
            "Station Forge/01. FORMAT STL/Corrupted Guard Kill Team/Corrupted Presupported/Gunner/a.stl",
            "Station Forge/01. FORMAT STL/Corrupted Guard Kill Team/Corrupted Presupported/Leader/a.stl",
        ],
    );
    // the kill team's figures have no pictures of their own -> the unit is one model
    assert_eq!(names(&res), set(&["GrimGuard - Command Force", "Corrupted Guard Kill Team"]));
    let m = by_name(&res);
    assert_eq!(res.models[m["GrimGuard - Command Force"]].variants(), strs(&["", "supported", "unsupported"]));
    let kt = m["Corrupted Guard Kill Team"];
    assert_eq!(res.models[kt].variants(), strs(&["supported", "unsupported"]));
    assert_eq!(part_groups(&res, kt), set(&["Gunner", "Leader"]));
    assert_eq!(res.hero(kt).unwrap().name, "1.jpg");
}

#[test]
fn gdrive_export_folders_and_parent_name_repeat() {
    // Ghamak Hoplites
    let base = "Ghamak/565-592_Fantasy January 2023/578-590_Hoplites of Light";
    let paths = under(
        base,
        &[
            "Hoplites of Light/578 Hoplites of Light A/578 Complete Hoplites of Light A.jpg",
            "Hoplites of Light/578 Hoplites of Light A/578 Hoplites of Light A.zip/a.stl",
            "presupported/578 Hoplites of Light A.zip/578 Hoplites of Light A-20221219T082318Z-001/578 Hoplites of Light A/a Lychee.lys",
            "Hoplites of Light/579 Hoplites of Light B/579 Hoplites of Light B.zip/b.stl",
            "presupported/579 Hoplites of Light B.zip/579 Hoplites of Light B/b.stl",
        ],
    );
    let res = run("Ghamak", &owned(&paths));
    assert_eq!(names(&res), set(&["Hoplites of Light A", "Hoplites of Light B"]));
    assert_eq!(res.models[by_name(&res)["Hoplites of Light A"]].variants(), strs(&["", "supported"]));
    assert_eq!(hero(&res, "Hoplites of Light A").unwrap(), "578 Complete Hoplites of Light A.jpg");
}

#[test]
fn group_images_matched_by_distinctive_tokens() {
    // OPR _Images folder
    let paths = under(
        "One Page Rules/OPR_Battle Sisters",
        &[
            "_Images/APC 1.png",
            "_Images/Biker Warrior 1.png",
            "_Images/Fanatic Superiors 2.png",
            "_Images/Fanatics 1.png",
            "One Page Rules - Battle Sisters - APC/a.stl",
            "One Page Rules - Battle Sisters - Biker Warriors/a.stl",
            "One Page Rules - Battle Sisters - Fanatics/a.stl",
            "One Page Rules - Battle Sisters - Fanatics Superior/a.stl",
        ],
    );
    let res = run("One Page Rules", &owned(&paths));
    assert_eq!(
        heroes(&res),
        heroes_of(&[
            ("Battle Sisters - APC", "APC 1.png"),
            ("Battle Sisters - Biker Warriors", "Biker Warrior 1.png"),
            ("Battle Sisters - Fanatics", "Fanatics 1.png"),
            ("Battle Sisters - Fanatics Superior", "Fanatic Superiors 2.png"),
        ])
    );
}

#[test]
fn format_tag_in_archive_name_is_not_a_variant() {
    // CHAOS MARINE BUILDER
    let s = "CHAOS MARINE BUILDER (Format OBJ)";
    let paths = under(
        s,
        &[
            "CHAOS_MARINE_BUILDER.jpg",
            "BUILDER 1/Arms/a.obj",
            "BUILDER 1/Heads/h.obj",
            "BUILDER 2/Bodies/b.stl",
            "BUILDER 2/Chainswords/c.stl",
            "BUILDER 2/Pistols/p.stl",
        ],
    );
    let res = run(s, &owned(&paths));
    assert_eq!(res.models.iter().map(|m| m.name.as_str()).collect::<Vec<_>>(), ["CHAOS MARINE BUILDER"]);
    assert_eq!(res.models[0].variants(), strs(&[""]));
    assert_eq!(res.hero(0).unwrap().name, "CHAOS_MARINE_BUILDER.jpg");
}

#[test]
fn parts_kit_is_one_model() {
    // P7 (Heldrake, Portuguese)
    let res = run(
        "Heldrake",
        &["Heldrake/image.png", "Heldrake/asas/a.stl", "Heldrake/bracos/b.stl", "Heldrake/cabeca/c.stl", "Heldrake/corpo/d.stl"],
    );
    assert_eq!(res.models.iter().map(|m| m.name.as_str()).collect::<Vec<_>>(), ["Heldrake"]);
    assert_eq!(res.hero(0).unwrap().name, "image.png");
}

#[test]
fn numbered_catalog_image_in_variant() {
    // P9 (Ghamak 842)
    let paths = under(
        "Ghamak/842-877_Sci-Fi - 2023.06",
        &[
            "842 Complete Tarantula/842_Tarantula_Supported/a.stl",
            "842 Complete Tarantula/842_Tarantula_Unsupported/a.stl",
            "842 Complete Tarantula/842_Tarantula_Unsupported/842 Complete Tarantula.jpg",
            "843 Complete Warp Bringer/843_Warp_bringer_Supported/a.stl",
        ],
    );
    let res = run("Ghamak", &owned(&paths));
    let t = by_name(&res)["Tarantula"];
    assert_eq!(res.models[t].attrs.catalog_id.as_deref(), Some("842"));
    assert_eq!(res.hero(t).unwrap().name, "842 Complete Tarantula.jpg");
}

#[test]
fn group_images_matched_by_name() {
    // P12 (Imperitum)
    let paths = under(
        "Imperitum Arts/Void Dancer Troupe Kill Team",
        &[
            "Void Dancer Troupe Kill Team (Imperitum Arts) Destiny Bane (Death Jester).jpg",
            "Void Dancer Troupe Kill Team (Imperitum Arts) Faceless Triumph (Solitaire).jpg",
            "Destiny Bane (Death Jester)/Destiny Bane (Death Jester)/a.stl",
            "Faceless Triumph (Solitaire)/Lychee Files/a.lys",
            "Faceless Triumph (Solitaire)/Supported Files/a.stl",
            "Faceless Triumph (Solitaire)/Mountain 3D Info Sheet.pdf",
        ],
    );
    let res = run("Imperitum Arts", &owned(&paths));
    let m = by_name(&res);
    assert_eq!(res.hero(m["Destiny Bane (Death Jester)"]).unwrap().match_, Match::Name);
    assert_eq!(res.hero(m["Faceless Triumph (Solitaire)"]).unwrap().match_, Match::Name);
    assert_eq!(res.models[m["Faceless Triumph (Solitaire)"]].variants(), strs(&["lychee", "supported"]));
}

#[test]
fn render_folders_and_per_file_thumbs() {
    // P10 + P11
    let res = run(
        "PuppetsWar Miniatures",
        &[
            "PuppetsWar Miniatures/Puppetswar_Feb23_/Puppetswar_Feb23.jpg",
            "PuppetsWar Miniatures/Puppetswar_Feb23_/Bushi/Renders/bushi (1).jpg",
            "PuppetsWar Miniatures/Puppetswar_Feb23_/Bushi/Supported/a.stl",
            "PuppetsWar Miniatures/Puppetswar_Feb23_/Bushi/Base/base.stl",
            "PuppetsWar Miniatures/Puppetswar_Feb23_/Knight/Supported/Pose A.stl",
            "PuppetsWar Miniatures/Puppetswar_Feb23_/Knight/Supported/Pose A.stl.png",
            "PuppetsWar Miniatures/New_Enforcer/supported/a.stl",
        ],
    );
    let m = by_name(&res);
    assert_eq!(res.hero(m["Bushi"]).unwrap().name, "bushi (1).jpg");
    assert_eq!(part_groups(&res, m["Bushi"]), set(&["", "Base"]));
    assert_eq!(res.model_assets(m["Knight"])[0].role, AssetRole::Thumb);
    let poster = res.assets.iter().find(|a| a.role == AssetRole::Poster).unwrap();
    assert_eq!(res.groups[poster.group.unwrap()].name, "Puppetswar Feb23");
}

#[test]
fn catalog_id_adopted_from_own_pictures() {
    // Ghamak Wild West Orkz
    let paths = under(
        "Ghamak/Wild West Orkz/442-447_Wild West Orkz Bullboyz Squad 1",
        &[
            "resize-442-complete-bullboyz-1-1.jpg",
            "resize-443-complete-bullboyz-2-1.jpg",
            "Bullboyz 1/442 Complete bullboyz 1.png",
            "Bullboyz 1/a.stl",
            "Bullboyz 2/443 complete bullboyz 2.png",
            "Bullboyz 2/a.stl",
        ],
    );
    let res = run("Ghamak", &owned(&paths));
    let b1 = by_name(&res)["Bullboyz 1"];
    assert_eq!(res.models[b1].attrs.catalog_id.as_deref(), Some("442"));
    assert_eq!(
        res.model_assets(b1).iter().map(|a| a.name.clone()).collect::<BTreeSet<_>>(),
        set(&["442 Complete bullboyz 1.png", "resize-442-complete-bullboyz-1-1.jpg"])
    );
}

#[test]
fn kit_prefixed_part_folders() {
    // PiperMakes Eclipse Frame
    let base = "PiperMakes/Eclipse Frame - TEOTH Version (Format OBJ)";
    let mut paths = vec![format!("{base}/Render1.png")];
    for p in ["Drones", "Heads", "Hands", "Legs_DiscKnees", "Main", "Weapons"] {
        paths.push(format!("{base}/TEOTH_{p}/x.obj"));
    }
    paths.push("PiperMakes/Punchbot/a.stl".into());
    let res = run("PiperMakes", &owned(&paths));
    assert_eq!(hero(&res, "Eclipse Frame - TEOTH Version").unwrap(), "Render1.png");
    assert_eq!(res.models.len(), 2);
}

#[test]
fn release_poster_goes_to_matching_subgroup() {
    // Ghamak Fantasy January
    let paths = under(
        "Ghamak/565-592_Fantasy January 2023",
        &[
            "Celestial Sagittarii presupported.jpg",
            "568-577_Celestial Sagittarii/569 Celestial Sagittarii 2 (male)/a.stl",
            "568-577_Celestial Sagittarii/569 Celestial Sagittarii 2 (male)/569 Complete.jpg",
            "568-577_Celestial Sagittarii/570 Celestial Sagittarii 3 (female)/a.stl",
            "566_Starlight Arcus/a.stl",
        ],
    );
    let res = run("Ghamak", &owned(&paths));
    let poster = res.assets.iter().find(|a| a.role == AssetRole::Poster).unwrap();
    assert_eq!(res.groups[poster.group.unwrap()].name, "Celestial Sagittarii");
}

// ---------------------------------------------------------------- collection level (Vol.2)

#[test]
fn vol1_archive_inside_collection_resolves_as_before() {
    let res = run(
        "01. Studio Collection",
        &[
            "Atlan Forge.rar/Atlan Forge/Asgardian Trikers/photo_2.jpg",
            "Atlan Forge.rar/Atlan Forge/Asgardian Trikers/Asgardian Trikers - Supported/bike.stl",
            "Atlan Forge.rar/Atlan Forge/Asgardian Trikers/Asgardian Trikers - Unsupported/bike.stl",
            "Atlan Forge.rar/Atlan Forge/Angelic Seraphim/Angelic Seraphim - Supported/a.stl",
            "Heldrake.rar/Heldrake/image.png",
            "Heldrake.rar/Heldrake/asas/a.stl",
            "Heldrake.rar/Heldrake/corpo/d.stl",
            "Heldrake.rar/Heldrake/cabeca/c.stl",
        ],
    );
    assert_eq!(names(&res), set(&["Asgardian Trikers", "Angelic Seraphim", "Heldrake"]));
    assert_eq!(res.models[by_name(&res)["Asgardian Trikers"]].variants(), strs(&["supported", "unsupported"]));
    assert_eq!(hero(&res, "Heldrake").unwrap(), "image.png");
}

#[test]
fn sidecar_images_on_disk() {
    // D1 (02. Model Collection)
    let res = run(
        "02. Model Collection",
        &[
            "40k Squat STL.jpg",
            "40k Squat STL.rar/squat.stl",
            "40K_Empress_fina.jpg",
            "40K_Empress_final.rar/empress/body.stl",
            "Eternal Eagle (1).jpg",
            "Eternal Eagle (2).jpg",
            "Eternal Eagle.rar/Eternal Eagle/a.stl",
            "Cyber_Viking.jpg",
            "Cyber_Viking1.jpg",
            "Cyber_Viking.rar/a.stl",
            "BA DC Heads and Weapons1.jpg",
            "BA DC Heads and Weapons.rar/BA/heads/h.stl",
            "Adeptus_Mechanicus.zip/Adeptus/a.stl",
        ],
    );
    let i = imgs(&res);
    assert_eq!(i["40k Squat"], strs(&["40k Squat STL.jpg"]));
    assert_eq!(i["40K Empress final"], strs(&["40K_Empress_fina.jpg"]));
    assert_eq!(i["Eternal Eagle"], strs(&["Eternal Eagle (1).jpg", "Eternal Eagle (2).jpg"]));
    assert_eq!(i["Cyber Viking"], strs(&["Cyber_Viking.jpg", "Cyber_Viking1.jpg"]));
    assert_eq!(i["BA DC Heads and Weapons"], strs(&["BA DC Heads and Weapons1.jpg"]));
    assert!((0..res.models.len()).all(|m| res.model_assets(m).iter().all(|a| a.match_ == Match::Sidecar)));
}

#[test]
fn sidecars_inside_studio_archive() {
    // D1 (01. Studio Collection)
    let mut paths = under(
        "PuppetsWar Miniatures.rar/PuppetsWar Miniatures",
        &[
            "Bug_Warrior.jpg",
            "Bug_Warriors.rar/bug/a.stl",
            "Bushi_Heavy_Gunners_Set_B.jpg",
            "Bushi_Heavy_Gunners_Set_B.rar/a.stl",
            "Bushi_Heavy_Gunners_Set_C.jpg",
            "Bushi_Heavy_Gunners_Set_C.rar/a.stl",
        ],
    );
    paths.extend(under(
        "Forge World.rar/Forge World",
        &[
            "Acasatus_Knight_Porphyrion-1_1.webp",
            "Acasatus_Knight_Porphyrion-1_3.jpg",
            "Acasatus_Knight_Porphyrion-1.zip/k.stl",
            "Imotekh.zip/i.stl",
        ],
    ));
    let res = run("01. Studio Collection", &owned(&paths));
    let i = imgs(&res);
    assert_eq!(i["Bug Warriors"], strs(&["Bug_Warrior.jpg"]));
    assert_eq!(i["Bushi Heavy Gunners Set C"], strs(&["Bushi_Heavy_Gunners_Set_C.jpg"]));
    assert_eq!(
        i["Acasatus Knight Porphyrion-1"],
        strs(&["Acasatus_Knight_Porphyrion-1_1.webp", "Acasatus_Knight_Porphyrion-1_3.jpg"])
    );
}

#[test]
fn disk_folder_of_variant_archives_is_one_model() {
    // D2
    let res = run(
        "03. ОРКИ",
        &[
            "Jetpack_boys_pk1/FMHM7pZW0VU.jpg",
            "Jetpack_boys_pk1/Jetpack_boys_pk1 2.jpg",
            "Jetpack_boys_pk1/Jetpack_boys_pk1_unsupported.zip/Jetpack_boys/a.stl",
            "Jetpack_boys_pk1/Unsupported.rar/b.stl",
            "MrModulork's Fire Ladz/kVkW-BVEjM0.jpg",
            "MrModulork's Fire Ladz/Unsupported.zip/Unsupported/a.stl",
            "MrModulork's Fire Ladz/Unsupported_1.zip/x/b.stl",
            "Primal_Boss.jpg",
            "Primal_Boss.rar/Primal_Boss/p.stl",
        ],
    );
    assert_eq!(names(&res), set(&["Jetpack boys pk1", "MrModulork's Fire Ladz", "Primal Boss"]));
    let m = by_name(&res);
    assert_eq!(res.models[m["Jetpack boys pk1"]].variants(), strs(&["unsupported"]));
    assert_eq!(res.models[m["Jetpack boys pk1"]].assets.len(), 2);
    assert_eq!(res.models[m["MrModulork's Fire Ladz"]].variants(), strs(&["unsupported"]));
}

#[test]
fn variant_archives_with_abbreviations() {
    // D2 (Wargame Exclusive)
    let paths = under(
        "Wargame Exclusive.rar/Wargame Exclusive",
        &[
            "RoP_AGG/RoP_AGG.jpg",
            "RoP_AGG/RoP_AGG_STL.zip/agg.stl",
            "RoP_AGG/RoP_AGG_STL_SUP.zip/agg_sup.stl",
            "RoP_CCW/RoP_CCW.jpg",
            "RoP_CCW/RoP_CCW_STL.zip/ccw.stl",
        ],
    );
    let res = run("01. Studio Collection", &owned(&paths));
    assert_eq!(names(&res), set(&["RoP AGG", "RoP CCW"]));
    assert_eq!(res.models[by_name(&res)["RoP AGG"]].variants(), strs(&["", "supported"]));
    assert_eq!(hero(&res, "RoP AGG").unwrap(), "RoP_AGG.jpg");
}

#[test]
fn archive_with_pictures_named_after_inner_models() {
    // D3 (04. Primarchs)
    let res = run(
        "04. Primarchs",
        &[
            "Angron.jpg",
            "Leman Rus.jpg",
            "Mortarion.jpg",
            "Primarchs.zip/Primarchs/Angron/a.stl",
            "Primarchs.zip/Primarchs/Leman Russ/l.stl",
            "Primarchs.zip/Primarchs/Mortarion/m.stl",
        ],
    );
    assert_eq!(
        heroes(&res),
        heroes_of(&[("Angron", "Angron.jpg"), ("Leman Russ", "Leman Rus.jpg"), ("Mortarion", "Mortarion.jpg")])
    );
}

#[test]
fn hollow_and_solid_casts_are_variants() {
    // D8 (Primarchs/Sanguinus)
    let res = run(
        "04. Primarchs",
        &[
            "Sanguinus.jpg",
            "Primarchs.zip/Primarchs/Sanguinus/Hueco/body.stl",
            "Primarchs.zip/Primarchs/Sanguinus/Normal/body.stl",
            "Primarchs.zip/Primarchs/Angron/a.stl",
        ],
    );
    let s = by_name(&res)["Sanguinus"];
    assert_eq!(res.models[s].variants(), strs(&["hollow", "solid"]));
    assert_eq!(res.hero(s).unwrap().name, "Sanguinus.jpg");
}

#[test]
fn folder_poster_one_level_up() {
    // D4 (TYRANIDS)
    let res = run(
        "02. Model Collection",
        &[
            "TYRANIDS/VOD_-_TYRANIDS_Complete.jpg",
            "TYRANIDS/VOD_-_TYRANIDS_Complete/Biovore.rar/Biovore/a.stl",
            "TYRANIDS/VOD_-_TYRANIDS_Complete/Biovore.rar/Biovore/biovore.jpg",
            "TYRANIDS/VOD_-_TYRANIDS_Complete/Broodlord.rar/Broodlord/b.stl",
            "TYRANIDS/VOD_-_TYRANIDS_Complete/Broodlord.rar/Broodlord/broodlord.png",
            "Krieg Horse.jpg",
            "Krieg Horse.rar/h.stl",
        ],
    );
    let poster = res.assets.iter().find(|a| a.name == "VOD_-_TYRANIDS_Complete.jpg").unwrap();
    assert_eq!(poster.role, AssetRole::Poster);
    assert_eq!(poster.group, res.models[by_name(&res)["Broodlord"]].group); // the units' group
    assert_eq!(hero(&res, "Biovore").unwrap(), "biovore.jpg");
}

#[test]
fn cyrillic_collection_resolves() {
    let res = run(
        "03. ОРКИ",
        &["Бойз/Бойз.jpg", "Бойз/Бойз Supported/a.stl", "Бойз/Бойз Unsupported/a.stl", "Нобы/Нобы.jpg", "Нобы/n.stl"],
    );
    assert_eq!(names(&res), set(&["Бойз", "Нобы"]));
    let b = by_name(&res)["Бойз"];
    assert_eq!(res.models[b].variants(), strs(&["supported", "unsupported"]));
    assert_eq!(res.hero(b).unwrap().name, "Бойз.jpg");
}
