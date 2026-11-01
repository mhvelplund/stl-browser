//! Cache layout: `<thumbs dir>/<location>/<content>.jpg`. The location says where a file is
//! (source path + chain), the content says what it holds, so a moved or changed file never
//! reuses a stale thumbnail.
use std::path::Path;

use crate::{has_extras, has_spin};

/// First 16 hex chars of blake3(`source path` + `\0` + chain joined with `\0`).
pub fn location_dir(source_path: &str, chain: &[String]) -> String {
    let mut h = blake3::Hasher::new();
    h.update(source_path.as_bytes());
    for member in chain {
        h.update(b"\0");
        h.update(member.as_bytes());
    }
    h.finalize().to_hex()[..16].to_string()
}

/// `<crc>-<size>` from the archive's stored CRC; `None` when the archive stores none.
pub fn content_name(crc: &str, size: u64) -> Option<String> {
    (!crc.is_empty()).then(|| format!("{}-{size}", crc.to_lowercase()))
}

/// First 32 hex chars of blake3 of the bytes (loose files, entries without a CRC).
pub fn content_hash(bytes: &[u8]) -> String {
    blake3::hash(bytes).to_hex()[..32].to_string()
}

pub fn cache_rel(location: &str, content: &str) -> String {
    format!("{location}/{content}.jpg")
}

/// The turntable strip next to a mesh's still: `<location>/<content>.spin.jpg`.
pub fn spin_rel(rel: &str) -> String {
    format!("{}.spin.jpg", rel.strip_suffix(".jpg").unwrap_or(rel))
}

/// Picture `i` (from 1) of a 3MF's carousel after its still: `<location>/<content>.p<i>.jpg`.
pub fn extra_rel(rel: &str, i: usize) -> String {
    format!("{}.p{i}.jpg", rel.strip_suffix(".jpg").unwrap_or(rel))
}

/// How many carousel pictures a 3MF has, written after them: `<location>/<content>.pics`.
pub fn pics_rel(rel: &str) -> String {
    format!("{}.pics", rel.strip_suffix(".jpg").unwrap_or(rel))
}

/// The carousel pictures after the still at `rel` that are on disk, in order (none until the
/// count file is written).
pub fn extra_rels(thumbs_dir: &Path, rel: &str) -> Vec<String> {
    let n: usize = std::fs::read_to_string(thumbs_dir.join(pics_rel(rel))).ok().and_then(|s| s.trim().parse().ok()).unwrap_or(0);
    (1..=n.min(64)).map(|i| extra_rel(rel, i)).filter(|r| thumbs_dir.join(r).is_file()).collect()
}

/// True when the cache holds everything the file `name` gets: the still, a mesh's turntable
/// strip, a 3MF's carousel count.
pub fn is_complete(thumbs_dir: &Path, rel: &str, name: &str) -> bool {
    thumbs_dir.join(rel).is_file()
        && (!has_spin(name) || thumbs_dir.join(spin_rel(rel)).is_file())
        && (!has_extras(name) || thumbs_dir.join(pics_rel(rel)).is_file())
}

/// Only `<16 lowercase hex>/<lowercase hex and dashes>[.spin|.p<n>].jpg` names a cache file
/// (so rows from the earlier WebP cache count as stale and are regenerated).
pub fn is_valid_rel(rel: &str) -> bool {
    let lower_hex = |c: char| c.is_ascii_digit() || ('a'..='f').contains(&c);
    let Some((dir, file)) = rel.split_once('/') else { return false };
    let Some(stem) = file.strip_suffix(".jpg") else { return false };
    let stem = match stem.rsplit_once('.') {
        Some((s, "spin")) => s,
        Some((s, p)) if p.len() > 1 && p.starts_with('p') && p[1..].chars().all(|c| c.is_ascii_digit()) => s,
        Some(_) => return false,
        None => stem,
    };
    dir.len() == 16 && dir.chars().all(lower_hex) && !stem.is_empty() && stem.chars().all(|c| lower_hex(c) || c == '-')
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_are_stable_and_shaped() {
        let a = location_dir("D:\\V\\G.rar", &["Grimlax/842.zip".into(), "S/body.stl".into()]);
        assert_eq!(a.len(), 16);
        assert!(a.chars().all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase()));
        assert_eq!(a, location_dir("D:\\V\\G.rar", &["Grimlax/842.zip".into(), "S/body.stl".into()]));
        assert_ne!(a, location_dir("D:\\V\\G.rar", &["Grimlax/842.zip/S/body.stl".into()]));
        assert_eq!(content_name("1A2B3C4D", 1234).as_deref(), Some("1a2b3c4d-1234"));
        assert_eq!(content_name("", 1234), None);
        assert_eq!(content_hash(b"x").len(), 32);
        assert_eq!(cache_rel(&a, "1a2b3c4d-1234"), format!("{a}/1a2b3c4d-1234.jpg"));
    }

    #[test]
    fn extras_need_their_count_and_files() {
        let tmp = tempfile::tempdir().unwrap();
        let rel = "0123456789abcdef/1a-4.jpg";
        std::fs::create_dir_all(tmp.path().join("0123456789abcdef")).unwrap();
        std::fs::write(tmp.path().join(rel), b"j").unwrap();
        for i in [1, 2, 4] {
            std::fs::write(tmp.path().join(extra_rel(rel, i)), b"j").unwrap();
        }
        assert!(extra_rels(tmp.path(), rel).is_empty()); // no count yet
        assert!(!is_complete(tmp.path(), rel, "a.3mf"));
        assert!(is_complete(tmp.path(), rel, "a.lys"));
        std::fs::write(tmp.path().join(pics_rel(rel)), "4").unwrap();
        assert_eq!(extra_rels(tmp.path(), rel), [extra_rel(rel, 1), extra_rel(rel, 2), extra_rel(rel, 4)]);
        assert!(is_complete(tmp.path(), rel, "a.3mf"));
        assert!(!is_complete(tmp.path(), rel, "a.stl")); // no strip
    }

    #[test]
    fn cache_rel_is_validated() {
        assert!(is_valid_rel("0123456789abcdef/1a2b3c4d-1234.jpg"));
        assert!(is_valid_rel("0123456789abcdef/0123456789abcdef0123456789abcdef.jpg"));
        assert!(is_valid_rel("0123456789abcdef/1a2b3c4d-1234.spin.jpg"));
        assert_eq!(spin_rel("0123456789abcdef/1a2b3c4d-1234.jpg"), "0123456789abcdef/1a2b3c4d-1234.spin.jpg");
        assert!(is_valid_rel("0123456789abcdef/1a2b3c4d-1234.p12.jpg"));
        assert_eq!(extra_rel("0123456789abcdef/1a-4.jpg", 3), "0123456789abcdef/1a-4.p3.jpg");
        assert_eq!(pics_rel("0123456789abcdef/1a-4.jpg"), "0123456789abcdef/1a-4.pics");
        for bad in [
            "0123456789abcdef/1a-4.pics",
            "0123456789abcdef/1a-4.p.jpg",
            "0123456789abcdef/1a-4.px.jpg",
            "0123456789abcdef/1a-4.p1.spin.jpg",
            "0123456789abcdef/a.spin.spin.jpg",
            "0123456789abcdef/.spin.jpg",
            "0123456789abcdef/a.other.jpg",
            "../index.db",
            "0123456789abcdef/../../x.jpg",
            "C:\\x.jpg",
            "/etc/x.jpg",
            "0123456789abcdef\\a.jpg",
            "0123456789abcdef/a.png",
            "0123456789abcdef/1a2b3c4d-1234.webp",
            "0123456789ABCDEF/a.jpg",
            "0123456789abcdef/.jpg",
            "0123456789abcdef/a.jpg/b.jpg",
            "",
        ] {
            assert!(!is_valid_rel(bad), "{bad}");
        }
    }
}
