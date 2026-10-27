//! RAR files can't be created without WinRAR, so these run against a real archive:
//! `STL_BROWSER_RAR_FIXTURE=<path to a .rar> cargo test -p stl-scan --test rar -- --ignored`
use stl_scan::archive::{self, ArchiveInput};

#[test]
#[ignore]
fn rar_lists_and_extracts() {
    let path = std::env::var_os("STL_BROWSER_RAR_FIXTURE").expect("set STL_BROWSER_RAR_FIXTURE");
    let mut a = archive::open(ArchiveInput::Path(path.into())).unwrap();
    let members = a.list().unwrap();
    assert!(!members.is_empty());
    let m = members.iter().min_by_key(|m| m.size).unwrap();
    let mut out = Vec::new();
    a.extract(&m.raw_name, &mut out).unwrap();
    assert_eq!(out.len() as u64, m.size);
    assert_eq!(format!("{:08X}", crc32fast::hash(&out)), m.crc);
    assert!(!m.path.contains('\\'), "{}", m.path);
}
