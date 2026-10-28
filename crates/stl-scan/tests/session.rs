mod common;

use stl_scan::extract::{SourceKind, extract_entry};
use stl_scan::session::ExtractSession;

fn chain(names: &[&str]) -> Vec<String> {
    names.iter().map(|s| s.to_string()).collect()
}

fn get(s: &mut ExtractSession, kind: SourceKind, p: &std::path::Path, c: &[&str]) -> Vec<u8> {
    let mut out = Vec::new();
    let n = s.extract(kind, p, &chain(c), &mut out).unwrap();
    assert_eq!(n, out.len() as u64);
    out
}

#[test]
fn session_matches_extract_entry() {
    let dir = tempfile::tempdir().unwrap();
    let p = common::write(dir.path(), "o.zip", &common::nested_fixture());
    let mut s = ExtractSession::new();
    for c in [
        &["Pack/inner.7z", "Model/deep.zip", "part.stl"][..],
        &["Pack/inner.7z", "Model/preview.jpg"],
        &["Pack/readme.txt"],
        &["Pack/inner.7z", "Model/deep.zip", "part.stl"],
    ] {
        let mut want = Vec::new();
        extract_entry(SourceKind::Archive, &p, &chain(c), &mut want).unwrap();
        assert_eq!(get(&mut s, SourceKind::Archive, &p, c), want, "{c:?}");
    }
    common::write(dir.path(), "loose/x.stl", b"loose");
    assert_eq!(get(&mut s, SourceKind::Folder, &dir.path().join("loose"), &["x.stl"]), b"loose");
}

#[test]
fn session_reuses_open_archives() {
    let dir = tempfile::tempdir().unwrap();
    let n = common::zip(&[("a.stl", b"a"), ("b.stl", b"b"), ("c.stl", b"c")]);
    let outer = common::sevenz(&[("n.zip", &n), ("top.stl", b"top")]);
    let p = common::write(dir.path(), "o.7z", &outer);
    let q = common::write(dir.path(), "q.zip", &common::zip(&[("q.stl", b"q")]));
    let mut s = ExtractSession::new();
    for name in ["a.stl", "b.stl", "c.stl"] {
        assert_eq!(get(&mut s, SourceKind::Archive, &p, &["n.zip", name]), name[..1].as_bytes());
    }
    assert_eq!(s.opens(), 2);
    assert_eq!(get(&mut s, SourceKind::Archive, &p, &["top.stl"]), b"top");
    assert_eq!(s.opens(), 2);
    assert_eq!(get(&mut s, SourceKind::Archive, &q, &["q.stl"]), b"q");
    assert_eq!(s.opens(), 3);
    // back to the nested one: the outer reopens, then the nested zip
    assert_eq!(get(&mut s, SourceKind::Archive, &p, &["n.zip", "a.stl"]), b"a");
    assert_eq!(s.opens(), 5);
}

#[test]
fn session_recovers_after_error() {
    let dir = tempfile::tempdir().unwrap();
    let p = common::write(dir.path(), "o.zip", &common::nested_fixture());
    let mut s = ExtractSession::new();
    let mut out = Vec::new();
    assert!(s.extract(SourceKind::Archive, &p, &chain(&["Pack/inner.7z", "nope.stl"]), &mut out).is_err());
    assert!(s.extract(SourceKind::Archive, &p, &chain(&[]), &mut out).is_err());
    assert_eq!(get(&mut s, SourceKind::Archive, &p, &["Pack/inner.7z", "Model/deep.zip", "part.stl"]), common::PART);
}
