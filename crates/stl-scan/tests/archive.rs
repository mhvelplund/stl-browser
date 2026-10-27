mod common;

use std::collections::HashSet;
use std::sync::Arc;

use stl_scan::archive::{self, ArchiveInput, ScanError};

fn crc_hex(b: &[u8]) -> String {
    format!("{:08X}", crc32fast::hash(b))
}

fn extract(a: &mut dyn archive::Archive, raw: &str) -> Vec<u8> {
    let mut out = Vec::new();
    a.extract(raw, &mut out).unwrap();
    out
}

#[test]
fn zip_lists_files_not_dirs() {
    let dir = tempfile::tempdir().unwrap();
    let p = common::write(dir.path(), "k.zip", &common::zip(&[("Kit/", b""), ("Kit/a.stl", b"12345"), ("Kit/b.jpg", b"jp")]));
    let mut a = archive::open(ArchiveInput::Path(p)).unwrap();
    let m = a.list().unwrap();
    assert_eq!(m.len(), 2, "{m:?}");
    let stl = m.iter().find(|x| x.path == "Kit/a.stl").unwrap();
    assert_eq!(stl.size, 5);
    assert_eq!(stl.crc, crc_hex(b"12345"));
    assert_eq!(stl.crc.len(), 8);
}

#[test]
fn zip_backslash_names() {
    let dir = tempfile::tempdir().unwrap();
    let p = common::write(dir.path(), "k.zip", &common::zip(&[("Kit\\a.stl", b"abc")]));
    let mut a = archive::open(ArchiveInput::Path(p)).unwrap();
    let m = a.list().unwrap();
    assert_eq!(m[0].path, "Kit/a.stl");
    assert_eq!(m[0].raw_name, "Kit\\a.stl");
    assert_eq!(extract(a.as_mut(), &m[0].raw_name), b"abc");
}

#[test]
fn zip_unicode_names() {
    let dir = tempfile::tempdir().unwrap();
    let p = common::write(dir.path(), "k.zip", &common::zip(&[("ГОБЛИНЫ/воин.stl", b"orc")]));
    let mut a = archive::open(ArchiveInput::Path(p)).unwrap();
    let m = a.list().unwrap();
    assert_eq!(m[0].path, "ГОБЛИНЫ/воин.stl");
    assert_eq!(extract(a.as_mut(), &m[0].raw_name), b"orc");
}

#[test]
fn sevenz_list_and_extract() {
    let dir = tempfile::tempdir().unwrap();
    let p = common::write(dir.path(), "k.7z", &common::sevenz(&[("Kit/a.stl", b"12345"), ("Kit/b.jpg", b"jp")]));
    let mut a = archive::open(ArchiveInput::Path(p)).unwrap();
    let m = a.list().unwrap();
    assert_eq!(m.len(), 2, "{m:?}");
    let stl = m.iter().find(|x| x.path == "Kit/a.stl").unwrap();
    assert_eq!(stl.size, 5);
    assert_eq!(stl.crc, crc_hex(b"12345"));
    assert_eq!(extract(a.as_mut(), &stl.raw_name), b"12345");
}

#[test]
fn bytes_input_equals_path_input() {
    let dir = tempfile::tempdir().unwrap();
    let bytes = common::zip(&[("x/a.stl", b"1"), ("x/b.stl", b"22")]);
    let p = common::write(dir.path(), "k.zip", &bytes);
    let from_path = archive::open(ArchiveInput::Path(p)).unwrap().list().unwrap();
    let from_bytes = archive::open(ArchiveInput::Bytes(Arc::new(bytes), "k.zip".into())).unwrap().list().unwrap();
    assert_eq!(from_path, from_bytes);
}

#[test]
fn extract_many_single_pass() {
    let bytes = common::sevenz(&[("a.stl", b"A"), ("b.stl", b"BB"), ("c.stl", b"CCC")]);
    let mut a = archive::open(ArchiveInput::Bytes(Arc::new(bytes), "k.7z".into())).unwrap();
    let want: HashSet<String> = ["a.stl".to_string(), "c.stl".to_string()].into();
    let mut got = Vec::new();
    a.extract_many(&want, &mut |name, data| {
        got.push((name.to_string(), data.to_vec()));
        Ok(())
    })
    .unwrap();
    got.sort();
    assert_eq!(got, vec![("a.stl".to_string(), b"A".to_vec()), ("c.stl".to_string(), b"CCC".to_vec())]);
}

/// Records the largest single write, to show extraction streams instead of buffering a member.
struct MaxChunk {
    total: usize,
    max: usize,
}

impl std::io::Write for MaxChunk {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.total += buf.len();
        self.max = self.max.max(buf.len());
        Ok(buf.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

#[test]
fn sevenz_extract_streams() {
    let big: Vec<u8> = (0..4 * 1024 * 1024u32).map(|i| (i % 251) as u8).collect();
    let bytes = common::sevenz(&[("first.stl", &big), ("second.stl", &big)]);
    let mut a = archive::open(ArchiveInput::Bytes(Arc::new(bytes), "k.7z".into())).unwrap();
    let mut w = MaxChunk { total: 0, max: 0 };
    a.extract("second.stl", &mut w).unwrap();
    assert_eq!(w.total, big.len());
    assert!(w.max <= 1024 * 1024, "member written in one {} byte chunk", w.max);
}

#[test]
fn password_protected_archives_are_encrypted() {
    let cases = [
        ("zip member", common::zip_encrypted(), "k.zip"),
        ("7z content", common::sevenz_encrypted(false), "k.7z"),
        ("7z header", common::sevenz_encrypted(true), "k.7z"),
    ];
    for (what, bytes, name) in cases {
        let r = archive::open(ArchiveInput::Bytes(Arc::new(bytes), name.into())).and_then(|mut a| a.list());
        assert!(matches!(r, Err(ScanError::Encrypted)), "{what}: {r:?}");
    }
}

#[test]
fn corrupt_archive_is_format_error() {
    let junk: Vec<u8> = (0..4096u32).map(|i| (i * 7919 % 251) as u8).collect();
    for name in ["x.zip", "x.7z"] {
        let r = archive::open(ArchiveInput::Bytes(Arc::new(junk.clone()), name.into())).and_then(|mut a| a.list());
        assert!(matches!(r, Err(ScanError::Format(_))), "{name}: {r:?}");
    }
}
