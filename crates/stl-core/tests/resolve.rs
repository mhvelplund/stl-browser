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
    // Outpost Forge Delta Squad
    let res = run(
        "Outpost Forge",
        &[
            "Outpost Forge/01. FORMAT STL/Delta Squad/1.jpg",
            "Outpost Forge/01. FORMAT STL/Delta Squad/25mm STL/Figure 1/a.stl",
            "Outpost Forge/01. FORMAT STL/Delta Squad/25mm Supported STL/Figure 1/a.stl",
            "Outpost Forge/01. FORMAT STL/Delta Squad/32mm STL/Figure 1/a.stl",
        ],
    );
    assert_eq!(res.models[0].variants(), strs(&["25mm", "32mm", "supported+25mm"]));
}

#[test]
fn leading_variant_merges_with_sibling() {
    // Outpost Forge Miscellaneous
    let res = run(
        "Outpost Forge",
        &[
            "Outpost Forge/Misc/1.jpg",
            "Outpost Forge/Misc/Miscellaneous/Backpacks/a.stl",
            "Outpost Forge/Misc/Miscellaneous Supported/Supported Backpacks STL/a.stl",
            "Outpost Forge/Misc/Miscellaneous/Skull/s.stl",
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
    // P6 + P14 (Grimlax Celestial)
    let base = "Grimlax/565-592_Fantasy January 2023/568-577_Celestial Archers";
    let paths = [
        format!("{base}/Celestial Archers/569 Celestial Archers 2 (male)/569 Celestial Archers 2 (male).zip"),
        format!("{base}/Celestial Archers/569 Celestial Archers 2 (male)/569 Celestial Archers 2 (male).zip/body.stl"),
        format!("{base}/Celestial Archers presupported/569 Celestial Archers 2 (male) presupported/body.stl"),
        format!("{base}/Celestial Archers/570 Celestial Archers 3 (female)/body.stl"),
    ];
    let res = run("Grimlax", &paths.iter().map(String::as_str).collect::<Vec<_>>());
    assert_eq!(names(&res), set(&["Celestial Archers 2 (male)", "Celestial Archers 3 (female)"]));
    let m = &res.models[by_name(&res)["Celestial Archers 2 (male)"]];
    assert_eq!(m.variants(), strs(&["supported", "unsupported"]));
    assert_eq!(m.files.len(), 2); // .zip itself dropped
}

#[test]
fn designer_tags_and_edition() {
    // P15 (Robots)
    let res = run(
        "Robot Units - Vexa",
        &[
            "Robot Units - Vexa/Gearwork Spider (PixelsofDoom) (5E)/a.stl",
            "Robot Units - Vexa/Cryptoguards (PixelsofDoom)/a.stl",
            "Robot Units - Vexa/Gearwork Reviver (unidentified)/a.stl",
        ],
    );
    let m = &res.models[by_name(&res)["Gearwork Spider"]];
    assert_eq!(
        m.attrs,
        Attrs { edition: Some("5E".into()), designer: Some("PixelsofDoom".into()), ..Default::default() }
    );
}

#[test]
fn unexpanded_nested_archive_counts_as_model() {
    // P14 (QKR without --nested)
    let res = run(
        "Quick Rules",
        &[
            "Quick Rules/QKR_War Sisters/Quick Rules - War Sisters - APC/WS - APC - US.7z",
            "Quick Rules/QKR_War Sisters/Quick Rules - War Sisters - Tank/WS - Tank - US.7z",
        ],
    );
    assert_eq!(res.models.len(), 2);
    assert_eq!(res.models[0].files[0].kind, Kind::Archive);
}

#[test]
fn common_word_tag_is_not_a_designer() {
    // FluteMakes drones
    let res = run("FluteMakes", &["P/K/Gun Drone 00 (Drone)/a.stl", "P/K/Recon Drone (Drone)/a.stl", "P/K/Medic/a.stl"]);
    assert!(res.models.iter().all(|m| m.attrs.designer.is_none()));
}

#[test]
fn thingiverse_exports_and_shared_prefix() {
    // D5 + D6 (07. Terrain)
    let mut paths = Vec::new();
    for n in ["Ancient+Alien+Pylon", "Modular_Trench", "Festungsmauer", "Eagle_Shrine", "Barricades"] {
        let z = format!("Tabletop - Wargames Terrain - {n}.zip");
        for f in ["LICENSE.txt", "README.txt", "attribution_card.html", "files/part_a.stl", "files/part_b.stl"] {
            paths.push(format!("{z}/{n}/{f}"));
        }
    }
    let res = run("07. Tabletop Terrain", &paths.iter().map(String::as_str).collect::<Vec<_>>());
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
    let res = run("Brutus_Varo", &["Brutus_Varo/Brutus_Varo.jpg", "Brutus_Varo/Head.stl", "Brutus_Varo/backpack.stl"]);
    assert_eq!(res.models.iter().map(|m| m.name.as_str()).collect::<Vec<_>>(), ["Brutus Varo"]);
    assert_eq!(res.models[0].files.len(), 2);
    assert_eq!(res.hero(0).unwrap().name, "Brutus_Varo.jpg");
}

#[test]
fn image_beside_single_folder() {
    // P2 + wrapper
    let res = run(
        "The Fallen Warlord Brakk",
        &[
            "The Fallen Warlord Brakk/The_Fallen_Warlord_Brakk.jpg",
            "The Fallen Warlord Brakk/The Fallen Warlord Brakk_8/body.stl",
            "The Fallen Warlord Brakk/The Fallen Warlord Brakk_8/wings.stl",
        ],
    );
    assert_eq!(res.models.len(), 1);
    assert_eq!(res.models[0].name, "The Fallen Warlord Brakk");
    assert_eq!(res.hero(0).unwrap().name, "The_Fallen_Warlord_Brakk.jpg");
}

#[test]
fn model_with_prefixed_variants() {
    // P3/P4 (Tidal Forge)
    let res = run(
        "Tidal Forge",
        &[
            "Tidal Forge/Northern Trikers/photo_2.jpg",
            "Tidal Forge/Northern Trikers/resize-tidal2-251.jpg",
            "Tidal Forge/Northern Trikers/Northern Trikers - Supported/bike.stl",
            "Tidal Forge/Northern Trikers/Northern Trikers - Unsupported/bike.stl",
            "Tidal Forge/Winged Seraphs/Winged Seraphs - Supported/a.stl",
            "Tidal Forge/Winged Seraphs/Winged Seraphs - Unsupported/a.stl",
        ],
    );
    assert_eq!(names(&res), set(&["Northern Trikers", "Winged Seraphs"]));
    let t = by_name(&res)["Northern Trikers"];
    assert_eq!(res.models[t].variants(), strs(&["supported", "unsupported"]));
    assert_eq!(part_groups(&res, t), set(&[""]));
    assert_eq!(res.models[t].assets.len(), 2);
    assert_eq!(res.groups[0].name, "Tidal Forge");
}

#[test]
fn duplicate_nesting_collapses() {
    // P5 (Basalt)
    let res = run(
        "Basalt Miniatures",
        &[
            "Basalt Miniatures/Grim Company Walker Thicc Boi/Basalt_Miniatures1.jpg",
            "Basalt Miniatures/Grim Company Walker Thicc Boi/Basalt Miniatures/Supported/a.stl",
            "Basalt Miniatures/Grim Company Walker Thicc Boi/Basalt Miniatures/Unsupported/a.stl",
            "Basalt Miniatures/Tomb Angel/Basalt Miniatures - Tomb Angel/Supported/b.stl",
            "Basalt Miniatures/Tomb Angel/Basalt Miniatures - Tomb Angel/Supported/desktop.ini",
        ],
    );
    assert_eq!(names(&res), set(&["Grim Company Walker Thicc Boi", "Tomb Angel"]));
    let m = by_name(&res);
    assert_eq!(res.models[m["Grim Company Walker Thicc Boi"]].assets.len(), 1);
    assert_eq!(res.models[m["Tomb Angel"]].files.len(), 1); // desktop.ini dropped
}

#[test]
fn variant_split_above_parts() {
    // P6 + P7 (STARWALKERS)
    let mut paths = Vec::new();
    for v in ["RAW STLS", "PRE-SUPPORTED STLS"] {
        for part in ["ARMS/ARM_01_L.stl", "HEADS/HEAD_1.stl", "LEGS/LEGS_07.stl", "BASE/BASE_40mm.stl"] {
            paths.push(format!("STARWALKERS/{v}/{part}"));
        }
    }
    paths.extend(["STARWALKERS/LYCHEE SCENES/scene1.lys", "STARWALKERS/CONTENTS.png", "STARWALKERS/INST_01.png"].map(String::from));
    let res = run("STARWALKERS", &owned(&paths));
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
        "Outpost Forge",
        &[
            "Outpost Forge/01. FORMAT STL/IronWard - Command Force/1.jpg",
            "Outpost Forge/01. FORMAT STL/IronWard - Command Force/Command Force/a.stl",
            "Outpost Forge/01. FORMAT STL/IronWard - Command Force/Command Force Supported/a.stl",
            "Outpost Forge/02. FORMAT OBJ/IronWard - Command Force/Command Force/a.obj",
            "Outpost Forge/01. FORMAT STL/Corrupted Guard Strike Team/1.jpg",
            "Outpost Forge/01. FORMAT STL/Corrupted Guard Strike Team/Corrupted/Gunner/a.stl",
            "Outpost Forge/01. FORMAT STL/Corrupted Guard Strike Team/Corrupted/Leader/a.stl",
            "Outpost Forge/01. FORMAT STL/Corrupted Guard Strike Team/Corrupted Presupported/Gunner/a.stl",
            "Outpost Forge/01. FORMAT STL/Corrupted Guard Strike Team/Corrupted Presupported/Leader/a.stl",
        ],
    );
    // the strike team's figures have no pictures of their own -> the unit is one model
    assert_eq!(names(&res), set(&["IronWard - Command Force", "Corrupted Guard Strike Team"]));
    let m = by_name(&res);
    assert_eq!(res.models[m["IronWard - Command Force"]].variants(), strs(&["", "supported", "unsupported"]));
    let kt = m["Corrupted Guard Strike Team"];
    assert_eq!(res.models[kt].variants(), strs(&["supported", "unsupported"]));
    assert_eq!(part_groups(&res, kt), set(&["Gunner", "Leader"]));
    assert_eq!(res.hero(kt).unwrap().name, "1.jpg");
}

#[test]
fn gdrive_export_folders_and_parent_name_repeat() {
    // Grimlax Phalanx
    let base = "Grimlax/565-592_Fantasy January 2023/578-590_Phalanx of Dawn";
    let paths = under(
        base,
        &[
            "Phalanx of Dawn/578 Phalanx of Dawn A/578 Complete Phalanx of Dawn A.jpg",
            "Phalanx of Dawn/578 Phalanx of Dawn A/578 Phalanx of Dawn A.zip/a.stl",
            "presupported/578 Phalanx of Dawn A.zip/578 Phalanx of Dawn A-20221219T082318Z-001/578 Phalanx of Dawn A/a Lychee.lys",
            "Phalanx of Dawn/579 Phalanx of Dawn B/579 Phalanx of Dawn B.zip/b.stl",
            "presupported/579 Phalanx of Dawn B.zip/579 Phalanx of Dawn B/b.stl",
        ],
    );
    let res = run("Grimlax", &owned(&paths));
    assert_eq!(names(&res), set(&["Phalanx of Dawn A", "Phalanx of Dawn B"]));
    assert_eq!(res.models[by_name(&res)["Phalanx of Dawn A"]].variants(), strs(&["", "supported"]));
    assert_eq!(hero(&res, "Phalanx of Dawn A").unwrap(), "578 Complete Phalanx of Dawn A.jpg");
}

#[test]
fn group_images_matched_by_distinctive_tokens() {
    // QKR _Images folder
    let paths = under(
        "Quick Rules/QKR_War Sisters",
        &[
            "_Images/APC 1.png",
            "_Images/Biker Warrior 1.png",
            "_Images/Fanatic Superiors 2.png",
            "_Images/Fanatics 1.png",
            "Quick Rules - War Sisters - APC/a.stl",
            "Quick Rules - War Sisters - Biker Warriors/a.stl",
            "Quick Rules - War Sisters - Fanatics/a.stl",
            "Quick Rules - War Sisters - Fanatics Superior/a.stl",
        ],
    );
    let res = run("Quick Rules", &owned(&paths));
    assert_eq!(
        heroes(&res),
        heroes_of(&[
            ("War Sisters - APC", "APC 1.png"),
            ("War Sisters - Biker Warriors", "Biker Warrior 1.png"),
            ("War Sisters - Fanatics", "Fanatics 1.png"),
            ("War Sisters - Fanatics Superior", "Fanatic Superiors 2.png"),
        ])
    );
}

#[test]
fn format_tag_in_archive_name_is_not_a_variant() {
    // RENEGADE TROOPER BUILDER
    let s = "RENEGADE TROOPER BUILDER (Format OBJ)";
    let paths = under(
        s,
        &[
            "RENEGADE_TROOPER_BUILDER.jpg",
            "BUILDER 1/Arms/a.obj",
            "BUILDER 1/Heads/h.obj",
            "BUILDER 2/Bodies/b.stl",
            "BUILDER 2/Chainaxes/c.stl",
            "BUILDER 2/Pistols/p.stl",
        ],
    );
    let res = run(s, &owned(&paths));
    assert_eq!(res.models.iter().map(|m| m.name.as_str()).collect::<Vec<_>>(), ["RENEGADE TROOPER BUILDER"]);
    assert_eq!(res.models[0].variants(), strs(&[""]));
    assert_eq!(res.hero(0).unwrap().name, "RENEGADE_TROOPER_BUILDER.jpg");
}

#[test]
fn parts_kit_is_one_model() {
    // P7 (Skydrake, Portuguese)
    let res = run(
        "Skydrake",
        &["Skydrake/image.png", "Skydrake/asas/a.stl", "Skydrake/bracos/b.stl", "Skydrake/cabeca/c.stl", "Skydrake/corpo/d.stl"],
    );
    assert_eq!(res.models.iter().map(|m| m.name.as_str()).collect::<Vec<_>>(), ["Skydrake"]);
    assert_eq!(res.hero(0).unwrap().name, "image.png");
}

#[test]
fn numbered_catalog_image_in_variant() {
    // P9 (Grimlax 842)
    let paths = under(
        "Grimlax/842-877_Sci-Fi - 2023.06",
        &[
            "842 Complete Spiderwalker/842_Spiderwalker_Supported/a.stl",
            "842 Complete Spiderwalker/842_Spiderwalker_Unsupported/a.stl",
            "842 Complete Spiderwalker/842_Spiderwalker_Unsupported/842 Complete Spiderwalker.jpg",
            "843 Complete Void Bringer/843_Void_bringer_Supported/a.stl",
        ],
    );
    let res = run("Grimlax", &owned(&paths));
    let t = by_name(&res)["Spiderwalker"];
    assert_eq!(res.models[t].attrs.catalog_id.as_deref(), Some("842"));
    assert_eq!(res.hero(t).unwrap().name, "842 Complete Spiderwalker.jpg");
}

#[test]
fn group_images_matched_by_name() {
    // P12 (Ember)
    let paths = under(
        "Ember Arts/Star Dancer Troupe Strike Team",
        &[
            "Star Dancer Troupe Strike Team (Ember Arts) Fatal Whim (Grim Jester).jpg",
            "Star Dancer Troupe Strike Team (Ember Arts) Masked Glory (Loner).jpg",
            "Fatal Whim (Grim Jester)/Fatal Whim (Grim Jester)/a.stl",
            "Masked Glory (Loner)/Lychee Files/a.lys",
            "Masked Glory (Loner)/Supported Files/a.stl",
            "Masked Glory (Loner)/Summit 3D Info Sheet.pdf",
        ],
    );
    let res = run("Ember Arts", &owned(&paths));
    let m = by_name(&res);
    assert_eq!(res.hero(m["Fatal Whim (Grim Jester)"]).unwrap().match_, Match::Name);
    assert_eq!(res.hero(m["Masked Glory (Loner)"]).unwrap().match_, Match::Name);
    assert_eq!(res.models[m["Masked Glory (Loner)"]].variants(), strs(&["lychee", "supported"]));
}

#[test]
fn render_folders_and_per_file_thumbs() {
    // P10 + P11
    let res = run(
        "MarionetteWar Miniatures",
        &[
            "MarionetteWar Miniatures/Marionettewar_Feb23_/Marionettewar_Feb23.jpg",
            "MarionetteWar Miniatures/Marionettewar_Feb23_/Ronin/Renders/ronin (1).jpg",
            "MarionetteWar Miniatures/Marionettewar_Feb23_/Ronin/Supported/a.stl",
            "MarionetteWar Miniatures/Marionettewar_Feb23_/Ronin/Base/base.stl",
            "MarionetteWar Miniatures/Marionettewar_Feb23_/Knight/Supported/Pose A.stl",
            "MarionetteWar Miniatures/Marionettewar_Feb23_/Knight/Supported/Pose A.stl.png",
            "MarionetteWar Miniatures/New_Enforcer/supported/a.stl",
        ],
    );
    let m = by_name(&res);
    assert_eq!(res.hero(m["Ronin"]).unwrap().name, "ronin (1).jpg");
    assert_eq!(part_groups(&res, m["Ronin"]), set(&["", "Base"]));
    assert_eq!(res.model_assets(m["Knight"])[0].role, AssetRole::Thumb);
    let poster = res.assets.iter().find(|a| a.role == AssetRole::Poster).unwrap();
    assert_eq!(res.groups[poster.group.unwrap()].name, "Marionettewar Feb23");
}

#[test]
fn catalog_id_adopted_from_own_pictures() {
    // Grimlax Wild West Gobz
    let paths = under(
        "Grimlax/Wild West Gobz/442-447_Wild West Gobz Hogriderz Squad 1",
        &[
            "resize-442-complete-hogriderz-1-1.jpg",
            "resize-443-complete-hogriderz-2-1.jpg",
            "Hogriderz 1/442 Complete hogriderz 1.png",
            "Hogriderz 1/a.stl",
            "Hogriderz 2/443 complete hogriderz 2.png",
            "Hogriderz 2/a.stl",
        ],
    );
    let res = run("Grimlax", &owned(&paths));
    let b1 = by_name(&res)["Hogriderz 1"];
    assert_eq!(res.models[b1].attrs.catalog_id.as_deref(), Some("442"));
    assert_eq!(
        res.model_assets(b1).iter().map(|a| a.name.clone()).collect::<BTreeSet<_>>(),
        set(&["442 Complete hogriderz 1.png", "resize-442-complete-hogriderz-1-1.jpg"])
    );
}

#[test]
fn kit_prefixed_part_folders() {
    // FluteMakes Solar Frame
    let base = "FluteMakes/Solar Frame - TRIAD Version (Format OBJ)";
    let mut paths = vec![format!("{base}/Render1.png")];
    for p in ["Drones", "Heads", "Hands", "Legs_DiscKnees", "Main", "Weapons"] {
        paths.push(format!("{base}/TRIAD_{p}/x.obj"));
    }
    paths.push("FluteMakes/Boxbot/a.stl".into());
    let res = run("FluteMakes", &owned(&paths));
    assert_eq!(hero(&res, "Solar Frame - TRIAD Version").unwrap(), "Render1.png");
    assert_eq!(res.models.len(), 2);
}

#[test]
fn release_poster_goes_to_matching_subgroup() {
    // Grimlax Fantasy January
    let paths = under(
        "Grimlax/565-592_Fantasy January 2023",
        &[
            "Celestial Archers presupported.jpg",
            "568-577_Celestial Archers/569 Celestial Archers 2 (male)/a.stl",
            "568-577_Celestial Archers/569 Celestial Archers 2 (male)/569 Complete.jpg",
            "568-577_Celestial Archers/570 Celestial Archers 3 (female)/a.stl",
            "566_Starlight Bowman/a.stl",
        ],
    );
    let res = run("Grimlax", &owned(&paths));
    let poster = res.assets.iter().find(|a| a.role == AssetRole::Poster).unwrap();
    assert_eq!(res.groups[poster.group.unwrap()].name, "Celestial Archers");
}

// ---------------------------------------------------------------- collection level (Vol.2)

#[test]
fn vol1_archive_inside_collection_resolves_as_before() {
    let res = run(
        "01. Studio Collection",
        &[
            "Tidal Forge.rar/Tidal Forge/Northern Trikers/photo_2.jpg",
            "Tidal Forge.rar/Tidal Forge/Northern Trikers/Northern Trikers - Supported/bike.stl",
            "Tidal Forge.rar/Tidal Forge/Northern Trikers/Northern Trikers - Unsupported/bike.stl",
            "Tidal Forge.rar/Tidal Forge/Winged Seraphs/Winged Seraphs - Supported/a.stl",
            "Skydrake.rar/Skydrake/image.png",
            "Skydrake.rar/Skydrake/asas/a.stl",
            "Skydrake.rar/Skydrake/corpo/d.stl",
            "Skydrake.rar/Skydrake/cabeca/c.stl",
        ],
    );
    assert_eq!(names(&res), set(&["Northern Trikers", "Winged Seraphs", "Skydrake"]));
    assert_eq!(res.models[by_name(&res)["Northern Trikers"]].variants(), strs(&["supported", "unsupported"]));
    assert_eq!(hero(&res, "Skydrake").unwrap(), "image.png");
}

#[test]
fn sidecar_images_on_disk() {
    // D1 (02. Model Collection)
    let res = run(
        "02. Model Collection",
        &[
            "SF Dwarf STL.jpg",
            "SF Dwarf STL.rar/dwarf.stl",
            "SF_Empress_fina.jpg",
            "SF_Empress_final.rar/empress/body.stl",
            "Eternal Falcon (1).jpg",
            "Eternal Falcon (2).jpg",
            "Eternal Falcon.rar/Eternal Falcon/a.stl",
            "Cyber_Raider.jpg",
            "Cyber_Raider1.jpg",
            "Cyber_Raider.rar/a.stl",
            "RC GC Heads and Weapons1.jpg",
            "RC GC Heads and Weapons.rar/RC/heads/h.stl",
            "Tech_Priests.zip/Tech/a.stl",
        ],
    );
    let i = imgs(&res);
    assert_eq!(i["SF Dwarf"], strs(&["SF Dwarf STL.jpg"]));
    assert_eq!(i["SF Empress final"], strs(&["SF_Empress_fina.jpg"]));
    assert_eq!(i["Eternal Falcon"], strs(&["Eternal Falcon (1).jpg", "Eternal Falcon (2).jpg"]));
    assert_eq!(i["Cyber Raider"], strs(&["Cyber_Raider.jpg", "Cyber_Raider1.jpg"]));
    assert_eq!(i["RC GC Heads and Weapons"], strs(&["RC GC Heads and Weapons1.jpg"]));
    assert!((0..res.models.len()).all(|m| res.model_assets(m).iter().all(|a| a.match_ == Match::Sidecar)));
}

#[test]
fn sidecars_inside_studio_archive() {
    // D1 (01. Studio Collection)
    let mut paths = under(
        "MarionetteWar Miniatures.rar/MarionetteWar Miniatures",
        &[
            "Bug_Warrior.jpg",
            "Bug_Warriors.rar/bug/a.stl",
            "Ronin_Heavy_Gunners_Set_B.jpg",
            "Ronin_Heavy_Gunners_Set_B.rar/a.stl",
            "Ronin_Heavy_Gunners_Set_C.jpg",
            "Ronin_Heavy_Gunners_Set_C.rar/a.stl",
        ],
    );
    paths.extend(under(
        "Resin Works.rar/Resin Works",
        &[
            "Arcanus_Knight_Paladin-1_1.webp",
            "Arcanus_Knight_Paladin-1_3.jpg",
            "Arcanus_Knight_Paladin-1.zip/k.stl",
            "Overlord.zip/i.stl",
        ],
    ));
    let res = run("01. Studio Collection", &owned(&paths));
    let i = imgs(&res);
    assert_eq!(i["Bug Warriors"], strs(&["Bug_Warrior.jpg"]));
    assert_eq!(i["Ronin Heavy Gunners Set C"], strs(&["Ronin_Heavy_Gunners_Set_C.jpg"]));
    assert_eq!(
        i["Arcanus Knight Paladin-1"],
        strs(&["Arcanus_Knight_Paladin-1_1.webp", "Arcanus_Knight_Paladin-1_3.jpg"])
    );
}

#[test]
fn disk_folder_of_variant_archives_is_one_model() {
    // D2
    let res = run(
        "03. ГОБЛИНЫ",
        &[
            "Jetpack_lads_pk1/FMHM7pZW0VU.jpg",
            "Jetpack_lads_pk1/Jetpack_lads_pk1 2.jpg",
            "Jetpack_lads_pk1/Jetpack_lads_pk1_unsupported.zip/Jetpack_lads/a.stl",
            "Jetpack_lads_pk1/Unsupported.rar/b.stl",
            "MrModulgob's Fire Gangz/kVkW-BVEjM0.jpg",
            "MrModulgob's Fire Gangz/Unsupported.zip/Unsupported/a.stl",
            "MrModulgob's Fire Gangz/Unsupported_1.zip/x/b.stl",
            "Primal_Boss.jpg",
            "Primal_Boss.rar/Primal_Boss/p.stl",
        ],
    );
    assert_eq!(names(&res), set(&["Jetpack lads pk1", "MrModulgob's Fire Gangz", "Primal Boss"]));
    let m = by_name(&res);
    assert_eq!(res.models[m["Jetpack lads pk1"]].variants(), strs(&["unsupported"]));
    assert_eq!(res.models[m["Jetpack lads pk1"]].assets.len(), 2);
    assert_eq!(res.models[m["MrModulgob's Fire Gangz"]].variants(), strs(&["unsupported"]));
}

#[test]
fn variant_archives_with_abbreviations() {
    // D2 (Tabletop Exclusive)
    let paths = under(
        "Tabletop Exclusive.rar/Tabletop Exclusive",
        &[
            "RoK_AGG/RoK_AGG.jpg",
            "RoK_AGG/RoK_AGG_STL.zip/agg.stl",
            "RoK_AGG/RoK_AGG_STL_SUP.zip/agg_sup.stl",
            "RoK_CCW/RoK_CCW.jpg",
            "RoK_CCW/RoK_CCW_STL.zip/ccw.stl",
        ],
    );
    let res = run("01. Studio Collection", &owned(&paths));
    assert_eq!(names(&res), set(&["RoK AGG", "RoK CCW"]));
    assert_eq!(res.models[by_name(&res)["RoK AGG"]].variants(), strs(&["", "supported"]));
    assert_eq!(hero(&res, "RoK AGG").unwrap(), "RoK_AGG.jpg");
}

#[test]
fn archive_with_pictures_named_after_inner_models() {
    // D3 (04. Warlords)
    let res = run(
        "04. Warlords",
        &[
            "Brakk.jpg",
            "Vorn Hald.jpg",
            "Morvane.jpg",
            "Warlords.zip/Warlords/Brakk/a.stl",
            "Warlords.zip/Warlords/Vorn Halda/l.stl",
            "Warlords.zip/Warlords/Morvane/m.stl",
        ],
    );
    assert_eq!(
        heroes(&res),
        heroes_of(&[("Brakk", "Brakk.jpg"), ("Vorn Halda", "Vorn Hald.jpg"), ("Morvane", "Morvane.jpg")])
    );
}

#[test]
fn hollow_and_solid_casts_are_variants() {
    // D8 (Warlords/Aurelion)
    let res = run(
        "04. Warlords",
        &[
            "Aurelion.jpg",
            "Warlords.zip/Warlords/Aurelion/Hueco/body.stl",
            "Warlords.zip/Warlords/Aurelion/Normal/body.stl",
            "Warlords.zip/Warlords/Brakk/a.stl",
        ],
    );
    let s = by_name(&res)["Aurelion"];
    assert_eq!(res.models[s].variants(), strs(&["hollow", "solid"]));
    assert_eq!(res.hero(s).unwrap().name, "Aurelion.jpg");
}

#[test]
fn folder_poster_one_level_up() {
    // D4 (XENOBUGS)
    let res = run(
        "02. Model Collection",
        &[
            "XENOBUGS/SQD_-_XENOBUGS_Complete.jpg",
            "XENOBUGS/SQD_-_XENOBUGS_Complete/Sporeback.rar/Sporeback/a.stl",
            "XENOBUGS/SQD_-_XENOBUGS_Complete/Sporeback.rar/Sporeback/sporeback.jpg",
            "XENOBUGS/SQD_-_XENOBUGS_Complete/Hivequeen.rar/Hivequeen/b.stl",
            "XENOBUGS/SQD_-_XENOBUGS_Complete/Hivequeen.rar/Hivequeen/hivequeen.png",
            "Trench Horse.jpg",
            "Trench Horse.rar/h.stl",
        ],
    );
    let poster = res.assets.iter().find(|a| a.name == "SQD_-_XENOBUGS_Complete.jpg").unwrap();
    assert_eq!(poster.role, AssetRole::Poster);
    assert_eq!(poster.group, res.models[by_name(&res)["Hivequeen"]].group); // the units' group
    assert_eq!(hero(&res, "Sporeback").unwrap(), "sporeback.jpg");
}

#[test]
fn cyrillic_collection_resolves() {
    let res = run(
        "03. ГОБЛИНЫ",
        &["Воины/Воины.jpg", "Воины/Воины Supported/a.stl", "Воины/Воины Unsupported/a.stl", "Вожди/Вожди.jpg", "Вожди/n.stl"],
    );
    assert_eq!(names(&res), set(&["Воины", "Вожди"]));
    let b = by_name(&res)["Воины"];
    assert_eq!(res.models[b].variants(), strs(&["supported", "unsupported"]));
    assert_eq!(res.hero(b).unwrap().name, "Воины.jpg");
}
