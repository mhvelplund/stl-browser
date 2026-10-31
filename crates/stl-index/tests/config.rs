//! `config.yaml`: defaults, the template, a custom list and a broken file.
use stl_core::vtree::filter::Shown;
use stl_index::config::{FILE_NAME, load_from};

#[test]
fn missing_file_gives_defaults_and_writes_the_template() {
    let dir = tempfile::tempdir().unwrap();
    let c = load_from(dir.path());
    assert_eq!(c.shown, Shown::defaults());
    assert!(c.warning.is_none());
    // the template parses back to the defaults
    assert!(std::fs::read_to_string(dir.path().join(FILE_NAME)).unwrap().contains("shown_extensions:"));
    let again = load_from(dir.path());
    assert_eq!(again.shown, Shown::defaults());
    assert!(again.warning.is_none());
}

#[test]
fn missing_key_gives_defaults() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join(FILE_NAME), "# nothing here\nother: 1\n").unwrap();
    let c = load_from(dir.path());
    assert_eq!(c.shown, Shown::defaults());
    assert!(c.warning.is_none());
}

#[test]
fn custom_list_replaces_the_defaults() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join(FILE_NAME), "shown_extensions: [stl, .EXE]\n").unwrap();
    assert_eq!(load_from(dir.path()).shown, Shown::new(["stl", "exe"]));
}

#[test]
fn broken_yaml_gives_defaults_and_a_warning() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join(FILE_NAME), "shown_extensions: [stl\n").unwrap();
    let c = load_from(dir.path());
    assert_eq!(c.shown, Shown::defaults());
    assert!(c.warning.unwrap().contains(FILE_NAME));
}
