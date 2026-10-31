//! Which files the views list: those whose ending is in the shown set. Folders, archives and
//! locked items are always shown.
use std::collections::HashSet;

use super::{Node, NodeKind};
use crate::names::{Kind, kind_of};

/// The endings shown when the config doesn't set any.
pub const DEFAULT_SHOWN: &[&str] = &[
    // meshes
    "stl", "obj", "3mf", "fbx", "blend", "dae",
    // slicer and scene files
    "lys", "lyt", "chitubox", "ctb", "photon", "sl1",
    // images
    "jpg", "jpeg", "png", "gif", "webp", "bmp",
    // documents
    "pdf", "txt", "md", "html", "htm",
];

/// Lowercase endings without the dot.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Shown(HashSet<String>);

impl Default for Shown {
    fn default() -> Self {
        Self::defaults()
    }
}

impl Shown {
    /// Endings are trimmed, lowercased and lose a leading dot (`.STL` is `stl`).
    pub fn new<S: AsRef<str>>(endings: impl IntoIterator<Item = S>) -> Self {
        Shown(
            endings
                .into_iter()
                .map(|e| e.as_ref().trim().trim_start_matches('.').to_lowercase())
                .filter(|e| !e.is_empty())
                .collect(),
        )
    }

    pub fn defaults() -> Self {
        Self::new(DEFAULT_SHOWN)
    }

    /// A file or image is shown when its ending is in the set; anything else always is.
    pub fn shows(&self, n: &Node) -> bool {
        // an inner archive that wasn't expanded (too deep, unreadable) stays a file node
        if !matches!(n.kind, NodeKind::File | NodeKind::Image) || kind_of(&n.name) == Kind::Archive {
            return true;
        }
        match n.name.rsplit_once('.') {
            Some((_, ext)) => self.0.contains(&ext.to_lowercase()),
            None => false,
        }
    }
}
