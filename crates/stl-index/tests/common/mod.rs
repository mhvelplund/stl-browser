//! Fixture helpers shared by the index integration tests.
#![allow(dead_code)]
use std::io::{Cursor, Write};
use std::path::Path;

use rusqlite::Connection;

pub fn sevenz(files: &[(&str, &[u8])]) -> Vec<u8> {
    let mut w = sevenz_rust2::ArchiveWriter::new(Cursor::new(Vec::new())).unwrap();
    for (name, data) in files {
        w.push_archive_entry(sevenz_rust2::ArchiveEntry::new_file(name), Some(*data)).unwrap();
    }
    w.finish().unwrap().into_inner()
}

pub fn zip(files: &[(&str, &[u8])]) -> Vec<u8> {
    let mut w = zip::ZipWriter::new(Cursor::new(Vec::new()));
    for (name, data) in files {
        w.start_file(*name, zip::write::SimpleFileOptions::default()).unwrap();
        w.write_all(data).unwrap();
    }
    w.finish().unwrap().into_inner()
}

pub fn zip_encrypted() -> Vec<u8> {
    let mut w = zip::ZipWriter::new(Cursor::new(Vec::new()));
    let opts = zip::write::SimpleFileOptions::default().with_aes_encryption(zip::AesMode::Aes256, "pw");
    w.start_file("secret.stl", opts).unwrap();
    w.write_all(b"s").unwrap();
    w.finish().unwrap().into_inner()
}

pub fn put(root: &Path, rel: &str, bytes: &[u8]) {
    let p = root.join(rel);
    std::fs::create_dir_all(p.parent().unwrap()).unwrap();
    std::fs::write(p, bytes).unwrap();
}

/// Rewrite a file and move its mtime forward, so a rescan sees a change.
pub fn rewrite(root: &Path, rel: &str, bytes: &[u8]) {
    let p = root.join(rel);
    std::fs::write(&p, bytes).unwrap();
    let now = filetime::FileTime::from_last_modification_time(&std::fs::metadata(&p).unwrap());
    filetime::set_file_mtime(&p, filetime::FileTime::from_unix_time(now.unix_seconds() + 10, 0)).unwrap();
}

pub fn count(conn: &Connection, sql: &str) -> i64 {
    conn.query_row(sql, [], |r| r.get(0)).unwrap()
}
