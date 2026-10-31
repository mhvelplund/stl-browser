//! Index storage: the app data dir, the SQLite schema, settings and incremental scans.
pub mod config;
pub mod db;
pub mod export;
pub mod parity;
pub mod paths;
pub mod resolvestore;
pub mod scanstore;
pub mod tagstore;
pub mod thumbstore;
pub mod views;
