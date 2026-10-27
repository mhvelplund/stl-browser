//! Port of `prototype/tests/test_names.py`: same inputs, same expected outputs.
use stl_core::names::*;

fn v(label: &str, prefix: &str) -> Option<(String, String)> {
    Some((label.into(), prefix.into()))
}

fn check(name: &str, expected: Option<(String, String)>) {
    assert_eq!(variant_of(name), expected, "{name}");
}

#[test]
fn plain_variant_folders() {
    check("Supported", v("supported", ""));
    check("Unsupported", v("unsupported", ""));
    check("Lychee scenes", v("lychee", ""));
    check("LYCHEE SCENES", v("lychee", ""));
    check("Original Files", v("original", ""));
    check("RAW STLS", v("raw", ""));
    check("PRE-SUPPORTED STLS", v("supported", ""));
    check("STLs", v("stl", ""));
    check("OBJ", v("obj", ""));
    check("Finished", v("finished", ""));
}

#[test]
fn prefixed_variants() {
    check("Asgardian Trikers - Supported", v("supported", "asgardian trikers"));
    check("GordonSupported", v("supported", "gordon"));
    check("GordonUnsuported", v("unsupported", "gordon"));
    check("842_Tarantula_Unsupported", v("unsupported", "842 tarantula"));
    check("Colonel STL", v("stl", "colonel"));
    check("Colonel Supported STL", v("supported", "colonel"));
    check("Corrupted Guard Walker Pressuported", v("supported", "corrupted guard walker"));
    check("PunchBot_RawOBJs", v("raw", "punchbot"));
    check("PunchBot_SupportedOBJs", v("supported", "punchbot"));
    check("Pressupported - By AiR", v("supported", ""));
}

#[test]
fn format_and_scale() {
    check("01. FORMAT STL", v("stl", ""));
    check("32mm Supported STL", v("supported 32mm", ""));
    check("25mm STL", v("stl 25mm", ""));
}

#[test]
fn leading_variant() {
    check("Presupported Flamers", v("supported", "flamers"));
    check("support_m4_free_brood", v("supported", "m4 free brood"));
    check("Supported Backpacks STL", v("stl supported", "backpacks"));
}

#[test]
fn not_variants() {
    for name in [
        "Sentinel - By AiR",
        "Asgardian Trikers",
        "Imperial_Fists_Builder",
        "Renders",
        "Starlight Arcus",
        "hands and flamers",
        "Wild West Orkz Warbozz 1",
    ] {
        assert_eq!(variant_of(name), None, "{name}");
    }
}

#[test]
fn parts() {
    for name in [
        "Arms", "L Arm", "Power Fist", "Empty Helmets", "TEOTH_Drones", "asas", "bracos", "cabeca", "corpo",
        "solder+base", "LEFT_HAND", "arms-Empty", "Backpack",
    ] {
        assert!(is_part_group(name), "{name}");
    }
}

#[test]
fn not_parts() {
    for name in [
        "Gloomy Captains", "859_ogre_with_shield_1", "Character", "angel", "Apothecary Biologis", "Immortals",
        "hands and flamers",
    ] {
        assert!(!is_part_group(name), "{name}");
    }
}

#[test]
fn roles() {
    assert_eq!(image_role("Pose - Front C1.stl.png"), ImageRole::Thumb);
    assert_eq!(image_role("Assembly-Instructions.jpg"), ImageRole::Instructions);
    assert_eq!(image_role("WARFORGED_INST_01.png"), ImageRole::Instructions);
    assert_eq!(image_role("Part_list.jpg"), ImageRole::Instructions);
    assert_eq!(image_role("Build Guide.png"), ImageRole::Instructions);
    assert_eq!(image_role("842 Complete Tarantula.jpg"), ImageRole::Preview);
}

#[test]
fn generic() {
    for name in [
        "1.jpg",
        "photo.jpg",
        "untitled.822.jpg",
        "LsB0hMMwR5E.jpg",
        "-qAyp6h5X38.jpg",
        "043ab0ec0976b0963523d742dc22973c_preview_featured.jpg",
        "photo_2.jpg",
    ] {
        assert!(is_generic_image(name), "{name}");
    }
    for name in ["Destiny Bane (Death Jester)-01.jpg", "Heldrake.png", "HL165.jpg"] {
        assert!(!is_generic_image(name), "{name}");
    }
}

#[test]
fn catalog_and_tags() {
    assert_eq!(
        parse_name("565_Sorcerer Lintiel and Sphinx", None, &[]),
        ("Sorcerer Lintiel and Sphinx".into(), Attrs { catalog_id: Some("565".into()), ..Default::default() })
    );
    assert_eq!(parse_name("HL165", None, &[]), ("HL165".into(), Attrs { catalog_id: Some("HL165".into()), ..Default::default() }));
    assert_eq!(
        parse_name("Canoptek Spyder (VirusesofDeath) (5E)", None, &["VirusesofDeath".into()]),
        (
            "Canoptek Spyder".into(),
            Attrs { edition: Some("5E".into()), designer: Some("VirusesofDeath".into()), ..Default::default() }
        )
    );
    assert_eq!(
        parse_name("Horus Lupercal By Alexei Konev", None, &[]),
        ("Horus Lupercal".into(), Attrs { designer: Some("Alexei Konev".into()), ..Default::default() })
    );
    assert_eq!(parse_name("PiperMakes (Format OBJ)", None, &[]).0, "PiperMakes");
    assert_eq!(parse_name("El Rhyman, Surrogate Miniatures April Hero Release", None, &[]).0, "El Rhyman");
}

#[test]
fn multibyte_names() {
    assert_eq!(variant_of("Орки Supported"), v("supported", "орки"));
    // the trailing-digit rule only looks at ASCII letters (as in the prototype), but must not panic
    assert_eq!(sidecar_key("Бой1.jpg"), "бой1");
    assert_eq!(sidecar_key("Café Viking1.jpg"), "café viking");
    assert_eq!(catalog_id("842 Орк"), Some("842".into()));
    assert_eq!(parse_name("ОРКИ_бой", None, &[]).0, "ОРКИ бой");
}
