//! Thumbnails: cache naming, image scaling, STL/OBJ rendering, batch generation and the job queue.
pub mod cache;
pub mod generate;
pub mod image;
pub mod mesh;
pub mod queue;
pub mod raster;

#[cfg(test)]
mod fixtures;

/// Thumbnails are square, this many pixels on a side.
pub const THUMB_SIZE: u32 = 256;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ThumbKind {
    Image,
    Mesh,
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
        assert_eq!(thumb_kind("x.lys"), None);
        assert_eq!(thumb_kind("x.3mf"), None);
        assert_eq!(thumb_kind("noext"), None);
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
