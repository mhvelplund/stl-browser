//! Thumbnails: cache naming, image scaling, STL/OBJ rendering, batch generation and the job queue.
pub mod cache;
pub mod image;
pub mod mesh;
pub mod raster;

#[cfg(test)]
mod fixtures;

/// Thumbnails are square, this many pixels on a side.
pub const THUMB_SIZE: u32 = 128;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ThumbKind {
    Image,
    Mesh,
}

/// Which files get a thumbnail, by extension.
pub fn thumb_kind(name: &str) -> Option<ThumbKind> {
    let base = name.rsplit(['/', '\\']).next().unwrap_or(name);
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
}
