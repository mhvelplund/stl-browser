//! Location of the app data dir (`~/.stl-browser`, or `$STL_BROWSER_HOME` for tests).
use std::path::PathBuf;

pub const HOME_ENV: &str = "STL_BROWSER_HOME";

/// The app data dir. Created if missing.
pub fn app_dir() -> PathBuf {
    let dir = match std::env::var_os(HOME_ENV) {
        Some(p) => PathBuf::from(p),
        None => dirs::home_dir().expect("no home directory").join(".stl-browser"),
    };
    std::fs::create_dir_all(&dir).expect("cannot create app dir");
    dir
}

pub fn db_path() -> PathBuf {
    app_dir().join("index.db")
}
