//! Port of `prototype/tests/test_resolve.py`: resolver tests built from real archive layouts.
use std::collections::{BTreeSet, HashMap};

use stl_core::names::{Attrs, Kind};
use stl_core::resolve::{Entry, Resolution, resolve};

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
