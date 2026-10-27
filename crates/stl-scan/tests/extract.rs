mod common;

use stl_scan::extract::{SourceKind, extract_entry};

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
