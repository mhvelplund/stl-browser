mod common;

use std::collections::{BTreeMap, HashSet};

use stl_scan::extract::{SourceKind, extract_entry, extract_many_entries};

fn chain(names: &[&str]) -> Vec<String> {
    names.iter().map(|s| s.to_string()).collect()
}

#[test]
fn extract_deep_chain_byte_identical() {
    let dir = tempfile::tempdir().unwrap();
    let p = common::write(dir.path(), "o.zip", &common::nested_fixture());
    let mut out = Vec::new();
    let n = extract_entry(SourceKind::Archive, &p, &chain(&["Pack/inner.7z", "Model/deep.zip", "part.stl"]), &mut out).unwrap();
    assert_eq!(out, common::PART);
    assert_eq!(n, common::PART.len() as u64);
}

#[test]
fn extract_from_folder() {
    let dir = tempfile::tempdir().unwrap();
    common::write(dir.path(), "sub/x.stl", b"loose");
    let mut out = Vec::new();
    extract_entry(SourceKind::Folder, dir.path(), &chain(&["sub/x.stl"]), &mut out).unwrap();
    assert_eq!(out, b"loose");
}

#[test]
fn missing_member_errors() {
    let dir = tempfile::tempdir().unwrap();
    let p = common::write(dir.path(), "o.zip", &common::nested_fixture());
    let mut out = Vec::new();
    assert!(extract_entry(SourceKind::Archive, &p, &chain(&["Pack/inner.7z", "nope.stl"]), &mut out).is_err());
    assert!(extract_entry(SourceKind::Archive, &p, &chain(&[]), &mut out).is_err());
}

fn many(kind: SourceKind, p: &std::path::Path, outer: &[&str], names: &[&str]) -> BTreeMap<String, Vec<u8>> {
    let want: HashSet<String> = names.iter().map(|s| s.to_string()).collect();
    let mut got = BTreeMap::new();
    extract_many_entries(kind, p, &chain(outer), &want, &mut |n, b| {
        got.insert(n.to_string(), b.to_vec());
        Ok(())
    })
    .unwrap();
    got
}

#[test]
fn extract_many_entries_follows_nested_chain() {
    let dir = tempfile::tempdir().unwrap();
    let p = common::write(dir.path(), "o.zip", &common::nested_fixture());
    let got = many(SourceKind::Archive, &p, &["Pack/inner.7z", "Model/deep.zip"], &["part.stl"]);
    assert_eq!(got.len(), 1);
    assert_eq!(got["part.stl"], common::PART);
}

#[test]
fn extract_many_entries_from_outer_archive() {
    let dir = tempfile::tempdir().unwrap();
    let p = common::write(dir.path(), "a.zip", &common::zip(&[("a.txt", b"A"), ("d/b.txt", b"B"), ("c.txt", b"C")]));
    let got = many(SourceKind::Archive, &p, &[], &["a.txt", "d/b.txt"]);
    assert_eq!(got.keys().collect::<Vec<_>>(), ["a.txt", "d/b.txt"]);
    assert_eq!(got["d/b.txt"], b"B");
}

#[test]
fn extract_many_entries_from_folder_skips_missing() {
    let dir = tempfile::tempdir().unwrap();
    common::write(dir.path(), "sub/x.stl", b"loose");
    let got = many(SourceKind::Folder, dir.path(), &[], &["sub/x.stl", "gone.stl"]);
    assert_eq!(got.len(), 1);
    assert_eq!(got["sub/x.stl"], b"loose");
}
