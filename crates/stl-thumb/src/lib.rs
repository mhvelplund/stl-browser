//! Thumbnails: cache naming, image scaling, STL/OBJ rendering (a still and a turntable strip),
//! embedded 3MF/LYS previews, batch generation and the job queue.
pub mod cache;
pub mod generate;
pub mod image;
pub mod mesh;
pub mod preview;
pub mod queue;
pub mod raster;

#[cfg(test)]
mod fixtures;

/// Thumbnails are square, this many pixels on a side.
pub const THUMB_SIZE: u32 = 256;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ThumbKind {
    Image,
    /// Rendered from triangles, with a turntable strip for hovering.
    Mesh,
    /// A picture the file carries itself (3MF and LYS scenes).
    Preview,
}

/// Frames in a mesh's turntable strip (`cache::spin_rel`), evenly spaced around a full turn.
pub const SPIN_FRAMES: u32 = 12;

/// True when a file of this name gets a turntable strip next to its still thumbnail.
pub fn has_spin(name: &str) -> bool {
    thumb_kind(name) == Some(ThumbKind::Mesh)
}

/// True when a file of this name gets a carousel of its own pictures after its still (a 3MF's
/// plate renders and photos, `cache::extra_rels`).
pub fn has_extras(name: &str) -> bool {
    thumb_kind(name) == Some(ThumbKind::Preview) && name.to_ascii_lowercase().ends_with(".3mf")
}

/// Which files get a thumbnail, by extension.
pub fn thumb_kind(name: &str) -> Option<ThumbKind> {
    let mut parts = name.split(['/', '\\']);
    // macOS AppleDouble metadata (`__MACOSX/`, `._x.stl`) borrows the real file's name
    if parts.clone().any(|p| p.eq_ignore_ascii_case("__macosx")) {
        return None;
    }
    let base = parts.next_back().unwrap_or(name);
    if base.starts_with("._") {
        return None;
    }
    let (_, ext) = base.rsplit_once('.')?;
    match ext.to_ascii_lowercase().as_str() {
        "jpg" | "jpeg" | "png" | "webp" | "gif" | "bmp" => Some(ThumbKind::Image),
        "stl" | "obj" => Some(ThumbKind::Mesh),
        "3mf" | "lys" => Some(ThumbKind::Preview),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kinds_by_extension() {
        assert_eq!(thumb_kind("A/B.JPG"), Some(ThumbKind::Image));
        assert_eq!(thumb_kind("x.stl"), Some(ThumbKind::Mesh));
        assert_eq!(thumb_kind("x.obj"), Some(ThumbKind::Mesh));
        assert_eq!(thumb_kind("x.LYS"), Some(ThumbKind::Preview));
        assert_eq!(thumb_kind("a/x.3mf"), Some(ThumbKind::Preview));
        assert_eq!(thumb_kind("x.lyt"), None);
        assert_eq!(thumb_kind("noext"), None);
    }

    #[test]
    fn only_meshes_spin() {
        assert!(has_spin("a/b.STL"));
        assert!(has_spin("b.obj"));
        assert!(!has_spin("b.3mf"));
        assert!(!has_spin("b.lys"));
        assert!(!has_spin("b.png"));
        assert!(!has_spin("__MACOSX/._b.stl"));
    }

    #[test]
    fn only_3mf_has_extras() {
        assert!(has_extras("a/B.3MF"));
        assert!(!has_extras("b.lys"));
        assert!(!has_extras("b.stl"));
        assert!(!has_extras("__MACOSX/._b.3mf"));
    }

    #[test]
    fn macos_metadata_files_have_no_thumbnail() {
        assert_eq!(thumb_kind("Overlord/__MACOSX/._head.stl"), None);
        assert_eq!(thumb_kind("._head.stl"), None);
        assert_eq!(thumb_kind("a\\__MACOSX\\b\\x.png"), None);
        assert_eq!(thumb_kind("__macosx/x.png"), None);
        assert_eq!(thumb_kind("._"), None);
        assert_eq!(thumb_kind("a._b.stl"), Some(ThumbKind::Mesh));
    }
}
