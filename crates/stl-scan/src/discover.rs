//! Find the sources under a root: every archive, plus one folder source per top-level directory
//! that holds loose (non-archive, non-junk) files. Port of `discover` in `prototype/stlindex/scan.py`.
use std::collections::BTreeMap;
use std::io;
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

use stl_core::names::{self, Kind};
use walkdir::WalkDir;

use crate::extract::SourceKind;

#[derive(Debug, Clone, PartialEq)]
pub struct SourceInfo {
    pub kind: SourceKind,
    /// Absolute path of the archive file / folder.
    pub path: PathBuf,
    /// `"<volume>/<path relative to the root>"`.
    pub rel_path: String,
    /// Display name: archive stem, or folder name.
    pub name: String,
    /// `"<volume>/<top-level folder>"`, or just the volume for files directly in the root.
    pub collection: String,
    /// Archive path inside its collection folder; `""` for folder sources.
    pub coll_path: String,
    pub size: u64,
    /// Seconds since the Unix epoch.
    pub mtime: f64,
    /// Folder sources: file paths relative to `path`, `/`-separated.
    pub loose_files: Vec<String>,
}

fn mtime_of(meta: &std::fs::Metadata) -> f64 {
    meta.modified().ok().and_then(|t| t.duration_since(UNIX_EPOCH).ok()).map_or(0.0, |d| d.as_secs_f64())
}

fn collection(vol: &str, top: &str) -> String {
    if top.is_empty() { vol.to_string() } else { format!("{vol}/{top}") }
}

/// Sources under `root`: archives sorted by relative path, then folder sources sorted by name.
pub fn discover(root: &Path) -> io::Result<Vec<SourceInfo>> {
    let vol = root.file_name().map_or_else(|| root.to_string_lossy().into_owned(), |n| n.to_string_lossy().into_owned());
    let mut archives = Vec::new();
    let mut loose: BTreeMap<String, Vec<(String, std::fs::Metadata)>> = BTreeMap::new();
    let walker = WalkDir::new(root)
        .sort_by_file_name()
        .into_iter()
        .filter_entry(|e| e.depth() == 0 || !(e.file_type().is_dir() && e.file_name().to_string_lossy().starts_with('.')));
    for entry in walker {
        let entry = entry.map_err(io::Error::other)?;
        if !entry.file_type().is_file() {
            continue;
        }
        let rel = entry.path().strip_prefix(root).map_err(io::Error::other)?.to_string_lossy().replace('\\', "/");
        let file_name = entry.file_name().to_string_lossy().into_owned();
        let top = match rel.split_once('/') {
            Some((t, _)) => t.to_string(),
            None => String::new(),
        };
        let meta = entry.metadata().map_err(io::Error::other)?;
        match names::kind_of(&file_name) {
            Kind::Archive => {
                if names::ext(&file_name) == ".rar" && names::is_secondary_rar_volume(&file_name) {
                    continue;
                }
                let coll_path = if top.is_empty() { rel.clone() } else { rel[top.len() + 1..].to_string() };
                archives.push(SourceInfo {
                    kind: SourceKind::Archive,
                    path: entry.path().to_path_buf(),
                    rel_path: format!("{vol}/{rel}"),
                    name: names::archive_stem(&file_name),
                    collection: collection(&vol, &top),
                    coll_path,
                    size: meta.len(),
                    mtime: mtime_of(&meta),
                    loose_files: Vec::new(),
                });
            }
            Kind::Junk => {}
            _ => loose.entry(top).or_default().push((rel, meta)),
        }
    }
    archives.sort_by(|a, b| a.rel_path.cmp(&b.rel_path));
    let mut out = archives;
    for (top, files) in loose {
        let path = if top.is_empty() { root.to_path_buf() } else { root.join(&top) };
        out.push(SourceInfo {
            kind: SourceKind::Folder,
            path,
            rel_path: collection(&vol, &top),
            name: if top.is_empty() { vol.clone() } else { top.clone() },
            collection: collection(&vol, &top),
            coll_path: String::new(),
            size: files.iter().map(|(_, m)| m.len()).sum(),
            mtime: files.iter().map(|(_, m)| mtime_of(m)).fold(0.0, f64::max),
            loose_files: files.into_iter().map(|(rel, _)| if top.is_empty() { rel } else { rel[top.len() + 1..].to_string() }).collect(),
        });
    }
    Ok(out)
}
