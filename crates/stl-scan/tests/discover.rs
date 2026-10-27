mod common;

use stl_scan::discover::{SourceInfo, discover};
use stl_scan::extract::SourceKind;

fn find<'a>(s: &'a [SourceInfo], rel: &str) -> &'a SourceInfo {
    s.iter().find(|x| x.rel_path == rel).unwrap_or_else(|| panic!("no {rel} in {:?}", s.iter().map(|x| &x.rel_path).collect::<Vec<_>>()))
}

#[test]
fn archives_and_loose() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().join("Vol");
    common::write(&root, "A/x.zip", &common::zip(&[("a.stl", b"1")]));
    common::write(&root, "A/img.jpg", b"jpg");
    common::write(&root, "B/sub/y.7z", &common::sevenz(&[("b.stl", b"2")]));
    common::write(&root, "top.stl", b"stl");
    let s = discover(&root, &mut |_| {}).unwrap();
    assert_eq!(s.len(), 4, "{s:?}");

    let x = find(&s, "Vol/A/x.zip");
    assert_eq!((x.kind, x.name.as_str(), x.collection.as_str(), x.coll_path.as_str()), (SourceKind::Archive, "x", "Vol/A", "x.zip"));
    assert_eq!(x.path, root.join("A").join("x.zip"));
    assert!(x.size > 0 && x.mtime > 0.0);

    let y = find(&s, "Vol/B/sub/y.7z");
    assert_eq!((y.collection.as_str(), y.coll_path.as_str()), ("Vol/B", "sub/y.7z"));

    let a = find(&s, "Vol/A");
    assert_eq!((a.kind, a.name.as_str(), a.collection.as_str()), (SourceKind::Folder, "A", "Vol/A"));
    assert_eq!(a.loose_files, ["img.jpg"]);
    assert_eq!(a.size, 3);

    let v = find(&s, "Vol");
    assert_eq!((v.kind, v.collection.as_str()), (SourceKind::Folder, "Vol"));
    assert_eq!(v.path, root);
    assert_eq!(v.loose_files, ["top.stl"]);
}

#[test]
fn skips_dot_dirs_and_junk() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().join("Vol");
    common::write(&root, ".3d-model-library/z.zip", &common::zip(&[("a.stl", b"1")]));
    common::write(&root, "A/desktop.ini", b"junk");
    assert!(discover(&root, &mut |_| {}).unwrap().is_empty());
}

#[test]
fn multipart_rar() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().join("Vol");
    common::write(&root, "A/k.part1.rar", b"r1");
    common::write(&root, "A/k.part2.rar", b"r2");
    let s = discover(&root, &mut |_| {}).unwrap();
    assert_eq!(s.len(), 1, "{s:?}");
    assert_eq!(s[0].name, "k");
}

#[cfg(windows)]
#[test]
fn unreadable_dir_warns_and_continues() {
    use std::process::Command;
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().join("Vol");
    common::write(&root, "A/a.zip", &common::zip(&[("a.stl", b"1")]));
    common::write(&root, "Locked/b.zip", &common::zip(&[("b.stl", b"2")]));
    let locked = root.join("Locked");
    let everyone = "*S-1-1-0";
    let deny = Command::new("icacls").arg(&locked).args(["/deny", &format!("{everyone}:(RX)")]).output().unwrap();
    assert!(deny.status.success(), "{deny:?}");
    let mut warnings = Vec::new();
    let r = discover(&root, &mut |w| warnings.push(w));
    Command::new("icacls").arg(&locked).args(["/remove:d", everyone]).output().unwrap();
    let s = r.unwrap();
    assert_eq!(s.len(), 1, "{s:?}");
    assert_eq!(s[0].rel_path, "Vol/A/a.zip");
    assert_eq!(warnings.len(), 1, "{warnings:?}");
    assert!(warnings[0].contains("Locked"), "{}", warnings[0]);
}

#[test]
fn unicode_dirs() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().join("Vol");
    common::write(&root, "03. ГОБЛИНЫ/a.zip", &common::zip(&[("a.stl", b"1")]));
    let s = discover(&root, &mut |_| {}).unwrap();
    assert_eq!(s[0].collection, "Vol/03. ГОБЛИНЫ");
}
