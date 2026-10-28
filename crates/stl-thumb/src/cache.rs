//! Cache layout: `<thumbs dir>/<location>/<content>.webp`. The location says where a file is
//! (source path + chain), the content says what it holds, so a moved or changed file never
//! reuses a stale thumbnail.

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
    format!("{location}/{content}.webp")
}

/// Only `<16 lowercase hex>/<lowercase hex and dashes>.webp` names a cache file.
pub fn is_valid_rel(rel: &str) -> bool {
    let lower_hex = |c: char| c.is_ascii_digit() || ('a'..='f').contains(&c);
    let Some((dir, file)) = rel.split_once('/') else { return false };
    let Some(stem) = file.strip_suffix(".webp") else { return false };
    dir.len() == 16 && dir.chars().all(lower_hex) && !stem.is_empty() && stem.chars().all(|c| lower_hex(c) || c == '-')
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_are_stable_and_shaped() {
        let a = location_dir("D:\\V\\G.rar", &["Ghamak/842.zip".into(), "S/body.stl".into()]);
        assert_eq!(a.len(), 16);
        assert!(a.chars().all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase()));
        assert_eq!(a, location_dir("D:\\V\\G.rar", &["Ghamak/842.zip".into(), "S/body.stl".into()]));
        assert_ne!(a, location_dir("D:\\V\\G.rar", &["Ghamak/842.zip/S/body.stl".into()]));
        assert_eq!(content_name("1A2B3C4D", 1234).as_deref(), Some("1a2b3c4d-1234"));
        assert_eq!(content_name("", 1234), None);
        assert_eq!(content_hash(b"x").len(), 32);
        assert_eq!(cache_rel(&a, "1a2b3c4d-1234"), format!("{a}/1a2b3c4d-1234.webp"));
    }

    #[test]
    fn cache_rel_is_validated() {
        assert!(is_valid_rel("0123456789abcdef/1a2b3c4d-1234.webp"));
        assert!(is_valid_rel("0123456789abcdef/0123456789abcdef0123456789abcdef.webp"));
        for bad in [
            "../index.db",
            "0123456789abcdef/../../x.webp",
            "C:\\x.webp",
            "/etc/x.webp",
            "0123456789abcdef\\a.webp",
            "0123456789abcdef/a.png",
            "0123456789ABCDEF/a.webp",
            "0123456789abcdef/.webp",
            "0123456789abcdef/a.webp/b.webp",
            "",
        ] {
            assert!(!is_valid_rel(bad), "{bad}");
        }
    }
}
