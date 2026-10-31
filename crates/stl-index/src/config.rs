//! The user's settings file, `config.yaml` in the app data dir. Read once at startup; a key it
//! leaves out takes its default.
use std::path::Path;

use serde::Deserialize;
use stl_core::vtree::filter::{DEFAULT_SHOWN, Shown};

use crate::paths::app_dir;

pub const FILE_NAME: &str = "config.yaml";

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct Raw {
    shown_extensions: Option<Vec<String>>,
}

#[derive(Debug, Clone)]
pub struct Config {
    /// The file endings the views list (folders and archives always show).
    pub shown: Shown,
    /// Why the file was ignored, when it couldn't be read.
    pub warning: Option<String>,
}

/// Read `config.yaml` in `dir`, writing a commented template there when it is missing.
pub fn load_from(dir: &Path) -> Config {
    let path = dir.join(FILE_NAME);
    let defaults = |warning| Config { shown: Shown::defaults(), warning };
    let text = match std::fs::read_to_string(&path) {
        Ok(t) => t,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            // best effort: the defaults apply either way
            let _ = std::fs::write(&path, template());
            return defaults(None);
        }
        Err(e) => return defaults(Some(format!("Could not read {}: {e}", path.display()))),
    };
    if text.trim().is_empty() {
        return defaults(None);
    }
    match serde_norway::from_str::<Raw>(&text) {
        Ok(raw) => Config { shown: raw.shown_extensions.map_or_else(Shown::defaults, Shown::new), warning: None },
        Err(e) => defaults(Some(format!("Ignoring {}: {e}", path.display()))),
    }
}

/// The config in the app data dir.
pub fn load() -> Config {
    load_from(&app_dir())
}

fn template() -> String {
    let mut s = String::from(
        "# STL Browser settings. Restart the app after editing.\n\
         \n\
         # File endings to show (without the dot). Folders and archives always show; any other\n\
         # file is hidden from browsing, search, drags and Extract to.\n\
         # Delete this key to go back to the defaults.\n\
         shown_extensions:\n",
    );
    for e in DEFAULT_SHOWN {
        s.push_str(&format!("  - {e}\n"));
    }
    s
}
