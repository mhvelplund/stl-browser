//! Pictures that scene files carry themselves, so they need no rendering:
//!
//! - **LYS** (Lychee scenes) are Electron *asar* containers: `u32 4`, `u32 header pickle size`,
//!   `u32`, `u32 JSON length`, the JSON file table at offset 16, and the file data from
//!   `8 + header pickle size`. The table (`mangoFiles`, or `files` in plain asar) lists a
//!   `preview.png` with its `offset` (a decimal string) and `size`.
//! - **3MF** files are ZIPs. `_rels/.rels` names the package thumbnail with the OPC
//!   relationship type `…/metadata/thumbnail`; slicers point it at a PNG such as
//!   `Metadata/plate_1.png`.
use std::io::{Cursor, Read};

/// Largest embedded picture read from a 3MF.
const MAX_PICTURE: u64 = 64 * 1024 * 1024;

/// The embedded picture's encoded bytes of a `.3mf` or `.lys` file (by the extension of `name`).
pub fn preview_bytes(name: &str, bytes: &[u8]) -> Result<Vec<u8>, String> {
    let ext = name.rsplit_once('.').map(|(_, e)| e.to_ascii_lowercase()).unwrap_or_default();
    match ext.as_str() {
        "lys" => lys_preview(bytes).map(<[u8]>::to_vec),
        "3mf" => threemf_preview(bytes),
        _ => Err(format!("no embedded preview: {name}")),
    }
}

fn lys_preview(b: &[u8]) -> Result<&[u8], String> {
    let bad = || "not a Lychee scene".to_string();
    let word = |i: usize| b.get(i * 4..i * 4 + 4).map(|s| u32::from_le_bytes([s[0], s[1], s[2], s[3]]) as usize);
    let (pickle, json_len) = (word(1).ok_or_else(bad)?, word(3).ok_or_else(bad)?);
    let json = b.get(16..16usize.checked_add(json_len).ok_or_else(bad)?).ok_or_else(bad)?;
    let table: serde_json::Value = serde_json::from_slice(json).map_err(|_| bad())?;
    let files = table.get("mangoFiles").or_else(|| table.get("files")).ok_or_else(bad)?;
    let entry = files.get("preview.png").ok_or("no preview in the scene")?;
    let num = |v: &serde_json::Value| v.as_u64().or_else(|| v.as_str()?.parse().ok());
    let offset = entry.get("offset").and_then(num).ok_or_else(bad)?;
    let size = entry.get("size").and_then(num).ok_or_else(bad)?;
    let start = 8u64.checked_add(pickle as u64).and_then(|s| s.checked_add(offset)).ok_or_else(bad)?;
    let end = start.checked_add(size).ok_or_else(bad)?;
    b.get(usize::try_from(start).map_err(|_| bad())?..usize::try_from(end).map_err(|_| bad())?).ok_or_else(|| "scene preview is cut off".into())
}

fn threemf_preview(b: &[u8]) -> Result<Vec<u8>, String> {
    let mut zip = zip::ZipArchive::new(Cursor::new(b)).map_err(|e| format!("not a 3MF: {e}"))?;
    let mut wanted = Vec::new();
    if let Ok(rels) = member(&mut zip, "_rels/.rels") {
        wanted.extend(thumbnail_target(&String::from_utf8_lossy(&rels)));
    }
    wanted.push("Metadata/thumbnail.png".into());
    wanted.push("Metadata/plate_1.png".into());
    for name in wanted {
        if let Ok(pic) = member(&mut zip, &name) {
            return Ok(pic);
        }
    }
    Err("no preview in the 3MF".into())
}

fn member(zip: &mut zip::ZipArchive<Cursor<&[u8]>>, name: &str) -> Result<Vec<u8>, String> {
    let f = zip.by_name(name).map_err(|e| e.to_string())?;
    if f.size() > MAX_PICTURE {
        return Err("too large".into());
    }
    let mut out = Vec::new();
    f.take(MAX_PICTURE).read_to_end(&mut out).map_err(|e| e.to_string())?;
    Ok(out)
}

/// The member a `.rels` file names as the package thumbnail, without its leading `/`.
fn thumbnail_target(rels: &str) -> Option<String> {
    rels.split("<Relationship").skip(1).find_map(|tag| {
        let tag = &tag[..tag.find('>').unwrap_or(tag.len())];
        let ty = attr(tag, "Type")?;
        ty.ends_with("/metadata/thumbnail").then(|| attr(tag, "Target"))?.map(|t| t.trim_start_matches('/').to_string())
    })
}

/// The value of attribute `name` in the inside of an XML tag (whole-word match, either quote).
fn attr<'a>(tag: &'a str, name: &str) -> Option<&'a str> {
    let mut from = 0;
    while let Some(i) = tag[from..].find(name).map(|i| i + from) {
        from = i + name.len();
        let before_ok = tag[..i].chars().next_back().is_some_and(char::is_whitespace);
        let rest = tag[from..].trim_start();
        if let (true, Some(rest)) = (before_ok, rest.strip_prefix('=')) {
            let rest = rest.trim_start();
            let q = rest.chars().next().filter(|c| *c == '"' || *c == '\'')?;
            let rest = &rest[1..];
            return rest.find(q).map(|end| &rest[..end]);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fixtures::{lys, png, zip};

    #[test]
    fn lys_preview_is_found() {
        let pic = png(5, 7);
        let scene = lys(&[("abc.bin", b"mesh data here"), ("o1_islands.bin", b"xx"), ("preview.png", &pic), ("scene.bin", b"s")]);
        assert_eq!(preview_bytes("a/b.LYS", &scene).unwrap(), pic);
    }

    #[test]
    fn broken_lys_is_an_error_not_a_panic() {
        let pic = png(5, 7);
        let scene = lys(&[("a.bin", b"m"), ("preview.png", &pic)]);
        for cut in [0, 3, 15, 20, 40, scene.len() - 1] {
            assert!(preview_bytes("x.lys", &scene[..cut]).is_err(), "cut at {cut}");
        }
        assert!(preview_bytes("x.lys", &lys(&[("a.bin", b"m")])).is_err());
        let mut huge = scene.clone();
        huge[12..16].copy_from_slice(&u32::MAX.to_le_bytes());
        assert!(preview_bytes("x.lys", &huge).is_err());
        assert!(preview_bytes("x.lys", b"garbage garbage garbage").is_err());
    }

    #[test]
    fn threemf_uses_the_thumbnail_relationship() {
        let (pic, other) = (png(3, 3), png(9, 9));
        let rels = br#"<?xml version="1.0"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">
 <Relationship Target="/3D/3dmodel.model" Id="rel-1" Type="http://schemas.microsoft.com/3dmanufacturing/2013/01/3dmodel"/>
 <Relationship TargetMode="Internal" Target='/Auxiliaries/.thumbnails/thumbnail_3mf.png' Id="rel-2" Type="http://schemas.openxmlformats.org/package/2006/relationships/metadata/thumbnail" />
 <Relationship Target="/Metadata/plate_1.png" Id="rel-4" Type="http://schemas.example.com/package/2021/cover-thumbnail-middle"/>
</Relationships>"#;
        let f = zip(&[("_rels/.rels", rels), ("Metadata/plate_1.png", &other), ("Auxiliaries/.thumbnails/thumbnail_3mf.png", &pic)]);
        assert_eq!(preview_bytes("m.3MF", &f).unwrap(), pic);
    }

    #[test]
    fn threemf_falls_back_to_known_names() {
        let pic = png(3, 3);
        assert_eq!(preview_bytes("m.3mf", &zip(&[("Metadata/thumbnail.png", &pic)])).unwrap(), pic);
        assert_eq!(preview_bytes("m.3mf", &zip(&[("_rels/.rels", b"<junk"), ("Metadata/plate_1.png", &pic)])).unwrap(), pic);
        assert!(preview_bytes("m.3mf", &zip(&[("3D/3dmodel.model", b"<model/>")])).is_err());
        assert!(preview_bytes("m.3mf", b"not a zip").is_err());
    }

    #[test]
    fn attributes_match_whole_names() {
        assert_eq!(attr(r#" TargetMode="x" Target="/a.png""#, "Target"), Some("/a.png"));
        assert_eq!(attr(r#" xTarget="/no" Target = '/b.png'"#, "Target"), Some("/b.png"));
        assert_eq!(attr(r#" Target=/bare"#, "Target"), None);
        assert_eq!(attr(r#" Id="1""#, "Target"), None);
    }
}
