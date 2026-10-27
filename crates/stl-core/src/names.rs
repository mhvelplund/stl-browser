//! Name heuristics: file kinds and name normalisation. Port of `prototype/stlindex/names.py`.

use std::sync::LazyLock;

use regex::Regex;

pub const MODEL_EXTS: &[&str] = &[".stl", ".obj", ".3mf", ".ply", ".fbx", ".blend", ".ztl", ".zpr", ".step", ".stp"];
pub const SCENE_EXTS: &[&str] = &[".lys", ".lyt", ".chitubox", ".ctb", ".cbddlp", ".goo", ".photon", ".pwmo", ".pwms", ".sl1"];
pub const IMAGE_EXTS: &[&str] = &[".jpg", ".jpeg", ".png", ".webp", ".gif", ".bmp"];
pub const COMPANION_EXTS: &[&str] = &[".mtl"];
pub const DOC_EXTS: &[&str] = &[".pdf", ".txt", ".md", ".html", ".htm", ".rtf"];
pub const ARCHIVE_EXTS: &[&str] = &[".zip", ".rar", ".7z"];
pub const JUNK_NAMES: &[&str] = &["desktop.ini", "thumbs.db", ".ds_store"];
/// Thingiverse/MMF export boilerplate: kept, but not shown as documents.
pub const META_STEMS: &[&str] = &["readme", "read me", "license", "licence", "sources", "attribution card", "link"];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Kind {
    Model,
    Scene,
    Image,
    Companion,
    Doc,
    Meta,
    Archive,
    Junk,
    Other,
}

/// The last path segment (`/` or `\` separated).
pub fn basename(name: &str) -> &str {
    name.rsplit(['/', '\\']).next().unwrap_or(name)
}

/// Split like Python's `os.path.splitext` on the basename: leading dots never start an extension.
fn split_ext(name: &str) -> (&str, &str) {
    let base = basename(name);
    let prefix = name.len() - base.len();
    match base.rfind('.') {
        Some(i) if base[..i].chars().any(|c| c != '.') => (&name[..prefix + i], &name[prefix + i..]),
        _ => (name, ""),
    }
}

/// Lowercase extension including the dot, or `""`.
pub fn ext(name: &str) -> String {
    split_ext(name).1.to_lowercase()
}

pub fn stem(name: &str) -> &str {
    split_ext(name).0
}

/// Classify a file name (or path; only the basename counts).
pub fn kind_of(name: &str) -> Kind {
    let base = basename(name);
    if JUNK_NAMES.contains(&base.to_lowercase().as_str()) {
        return Kind::Junk;
    }
    let e = ext(base);
    let e = e.as_str();
    if matches!(e, ".txt" | ".html" | ".htm" | ".md") && META_STEMS.contains(&normalize(stem(base)).as_str()) {
        return Kind::Meta;
    }
    for (exts, kind) in [
        (MODEL_EXTS, Kind::Model),
        (SCENE_EXTS, Kind::Scene),
        (IMAGE_EXTS, Kind::Image),
        (COMPANION_EXTS, Kind::Companion),
        (DOC_EXTS, Kind::Doc),
        (ARCHIVE_EXTS, Kind::Archive),
    ] {
        if exts.contains(&e) {
            return kind;
        }
    }
    Kind::Other
}

static NON_WORD: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"[\W_]+").unwrap());
static PART_VOLUME: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?i)\.part0*(\d+)\.rar$").unwrap());

/// Lowercase, runs of punctuation/underscores become single spaces.
pub fn normalize(name: &str) -> String {
    NON_WORD.replace_all(&name.to_lowercase(), " ").trim().to_string()
}

/// Archive display name: the stem without a `.part1`/`.part01`/`.part001` suffix.
pub fn archive_stem(file_name: &str) -> String {
    let s = stem(file_name);
    for suffix in [".part1", ".part01", ".part001"] {
        if s.to_lowercase().ends_with(suffix) {
            return s[..s.len() - suffix.len()].to_string();
        }
    }
    s.to_string()
}

/// `x.part2.rar`, `x.part03.rar`…: continuation volumes of a multi-part RAR, not sources of their own.
pub fn is_secondary_rar_volume(file_name: &str) -> bool {
    PART_VOLUME.captures(file_name).is_some_and(|c| c[1].parse::<u32>().is_ok_and(|n| n != 1))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kinds() {
        let cases = [
            ("a.STL", Kind::Model),
            ("x.lys", Kind::Scene),
            ("p.JPEG", Kind::Image),
            ("m.mtl", Kind::Companion),
            ("guide.pdf", Kind::Doc),
            ("README.txt", Kind::Meta),
            ("attribution_card.html", Kind::Meta),
            ("notes.txt", Kind::Doc),
            ("k.7z", Kind::Archive),
            ("Desktop.ini", Kind::Junk),
            ("f.obv", Kind::Other),
            ("Pack/inner.7z", Kind::Archive),
        ];
        for (name, kind) in cases {
            assert_eq!(kind_of(name), kind, "{name}");
        }
    }

    #[test]
    fn ext_and_stem() {
        assert_eq!(ext("A.B.STL"), ".stl");
        assert_eq!(stem("A.B.STL"), "A.B");
        assert_eq!(ext(".hidden"), "");
        assert_eq!(stem("noext"), "noext");
    }

    #[test]
    fn normalize_punctuation() {
        assert_eq!(normalize("Foo_Bar-Baz  (1)"), "foo bar baz 1");
        assert_eq!(normalize("ОРКИ_бой"), "орки бой");
    }

    #[test]
    fn rar_volumes() {
        assert_eq!(archive_stem("Kit.part01.rar"), "Kit");
        assert_eq!(archive_stem("Kit.zip"), "Kit");
        assert!(is_secondary_rar_volume("Kit.part2.rar"));
        assert!(!is_secondary_rar_volume("Kit.part1.rar"));
        assert!(!is_secondary_rar_volume("Kit.part001.rar"));
        assert!(!is_secondary_rar_volume("Kit.rar"));
    }
}
