mod common;

use stl_scan::nested::{self, ListOptions};

fn paths(entries: &[nested::ScannedEntry]) -> Vec<&str> {
    entries.iter().map(|e| e.path.as_str()).collect()
}

#[test]
fn zip_in_7z_in_zip() {
    let dir = tempfile::tempdir().unwrap();
    let p = common::write(dir.path(), "o.zip", &common::nested_fixture());
    let (entries, warnings) = nested::list_archive(&p, &ListOptions::default()).unwrap();
    assert!(warnings.is_empty(), "{warnings:?}");
    let deep = entries.iter().find(|e| e.path == "Pack/inner.7z/Model/deep.zip/part.stl").unwrap_or_else(|| panic!("{:?}", paths(&entries)));
    assert_eq!(deep.chain, ["Pack/inner.7z", "Model/deep.zip", "part.stl"]);
    assert_eq!(deep.size, common::PART.len() as u64);
    // the nested archive members themselves stay listed
    assert!(paths(&entries).contains(&"Pack/inner.7z"));
    assert!(paths(&entries).contains(&"Pack/inner.7z/Model/deep.zip"));
}

#[test]
fn depth_limit() {
    let dir = tempfile::tempdir().unwrap();
    let p = common::write(dir.path(), "o.zip", &common::nested_fixture());
    let opts = ListOptions { max_depth: 1, ..ListOptions::default() };
    let (entries, _) = nested::list_archive(&p, &opts).unwrap();
    assert!(paths(&entries).contains(&"Pack/inner.7z/Model/deep.zip"));
    assert!(!entries.iter().any(|e| e.path.contains("deep.zip/")), "{:?}", paths(&entries));
}

#[test]
fn corrupt_nested_warns() {
    let dir = tempfile::tempdir().unwrap();
    let p = common::write(dir.path(), "o.zip", &common::zip(&[("bad.7z", b"definitely not a 7z archive"), ("a.stl", b"x")]));
    let (entries, warnings) = nested::list_archive(&p, &ListOptions::default()).unwrap();
    assert!(paths(&entries).contains(&"a.stl"));
    assert!(paths(&entries).contains(&"bad.7z"));
    assert_eq!(warnings.len(), 1, "{warnings:?}");
    assert!(warnings[0].starts_with("nested bad.7z"), "{}", warnings[0]);
}

#[test]
fn encrypted_nested_archive_is_locked() {
    let dir = tempfile::tempdir().unwrap();
    let inner = common::sevenz_encrypted(false);
    let p = common::write(dir.path(), "o.zip", &common::zip(&[("Kit/locked.7z", &inner), ("Kit/a.stl", b"x")]));
    let (entries, warnings) = nested::list_archive(&p, &ListOptions::default()).unwrap();
    assert!(warnings.is_empty(), "{warnings:?}");
    let locked = entries.iter().find(|e| e.path == "Kit/locked.7z").unwrap();
    assert!(locked.locked);
    assert!(!entries.iter().find(|e| e.path == "Kit/a.stl").unwrap().locked);
    assert!(!entries.iter().any(|e| e.path.starts_with("Kit/locked.7z/")), "{:?}", paths(&entries));
}

#[test]
fn spills_above_mem_limit() {
    let dir = tempfile::tempdir().unwrap();
    let p = common::write(dir.path(), "o.zip", &common::nested_fixture());
    let (normal, _) = nested::list_archive(&p, &ListOptions::default()).unwrap();
    let before = nested::spill_count();
    let (spilled, warnings) = nested::list_archive(&p, &ListOptions { mem_limit: 10, ..ListOptions::default() }).unwrap();
    assert!(warnings.is_empty(), "{warnings:?}");
    assert_eq!(normal, spilled);
    assert!(nested::spill_count() > before);
}

#[test]
fn defaults_match_spec() {
    let o = ListOptions::default();
    assert_eq!(o.max_depth, 3);
    assert_eq!(o.mem_limit, 268_435_456);
}
