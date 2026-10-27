//! Fixture archives built at test time.
#![allow(dead_code)]
use std::io::{Cursor, Write};
use std::path::{Path, PathBuf};

/// A zip with the given members. Names ending in `/` become directory entries.
pub fn zip(files: &[(&str, &[u8])]) -> Vec<u8> {
    let mut w = zip::ZipWriter::new(Cursor::new(Vec::new()));
    let opts = zip::write::SimpleFileOptions::default();
    for (name, data) in files {
        if name.ends_with('/') {
            w.add_directory(*name, opts).unwrap();
        } else {
            w.start_file(*name, opts).unwrap();
            w.write_all(data).unwrap();
        }
    }
    w.finish().unwrap().into_inner()
}

/// A 7z with the given files.
pub fn sevenz(files: &[(&str, &[u8])]) -> Vec<u8> {
    let mut w = sevenz_rust2::ArchiveWriter::new(Cursor::new(Vec::new())).unwrap();
    for (name, data) in files {
        w.push_archive_entry(sevenz_rust2::ArchiveEntry::new_file(name), Some(*data)).unwrap();
    }
    w.finish().unwrap().into_inner()
}

/// A zip whose second member is AES-encrypted.
pub fn zip_encrypted() -> Vec<u8> {
    let mut w = zip::ZipWriter::new(Cursor::new(Vec::new()));
    w.start_file("open.stl", zip::write::SimpleFileOptions::default()).unwrap();
    w.write_all(b"open").unwrap();
    let locked = zip::write::SimpleFileOptions::default().with_aes_encryption(zip::AesMode::Aes256, "pw");
    w.start_file("secret.stl", locked).unwrap();
    w.write_all(b"secret").unwrap();
    w.finish().unwrap().into_inner()
}

/// A 7z with AES-encrypted content; `encrypt_header` also hides the file list.
pub fn sevenz_encrypted(encrypt_header: bool) -> Vec<u8> {
    use sevenz_rust2::encoder_options::{AesEncoderOptions, Lzma2Options};
    let mut w = sevenz_rust2::ArchiveWriter::new(Cursor::new(Vec::new())).unwrap();
    w.set_encrypt_header(encrypt_header);
    w.set_content_methods(vec![AesEncoderOptions::new(sevenz_rust2::Password::new("pw")).into(), Lzma2Options::default().into()]);
    w.push_archive_entry(sevenz_rust2::ArchiveEntry::new_file("secret.stl"), Some(&b"secret"[..])).unwrap();
    w.finish().unwrap().into_inner()
}

pub fn write(dir: &Path, name: &str, bytes: &[u8]) -> PathBuf {
    let p = dir.join(name);
    if let Some(parent) = p.parent() {
        std::fs::create_dir_all(parent).unwrap();
    }
    std::fs::write(&p, bytes).unwrap();
    p
}

/// `o.zip` > `Pack/inner.7z` > `Model/deep.zip` > `part.stl`.
pub const PART: &[u8] = b"solid part\n";
pub fn nested_fixture() -> Vec<u8> {
    let deep = zip(&[("part.stl", PART)]);
    let inner = sevenz(&[("Model/deep.zip", &deep), ("Model/preview.jpg", b"jpg")]);
    zip(&[("Pack/", b""), ("Pack/inner.7z", &inner), ("Pack/readme.txt", b"hi")])
}
