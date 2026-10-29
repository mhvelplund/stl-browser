//! Keeping the user's place across views: match folders by their real paths.
use stl_core::vtree::locate::locate;
use stl_core::vtree::{NodeId, NodeKind, Tree};

fn node(t: &mut Tree, parent: NodeId, name: &str, kind: NodeKind, real: &[&str]) -> NodeId {
    let id = t.add(parent, name, kind);
    for r in real {
        t.add_real(id, r);
    }
    id
}

const VOL: &str = r"D:\Models\Vol";
const COLL: &str = r"D:\Models\Vol\Coll";
const RAR: &str = r"D:\Models\Vol\Coll\Grimlax.rar";
const SARC: &str = r"D:\Models\Vol\Coll\Grimlax.rar!Grimlax/Sarcophagus";

/// Files view: `vol/coll/grimlax.rar/grimlax/{sarcophagus/body.stl, extras/}`.
fn files() -> Tree {
    let mut t = Tree::new();
    let vol = node(&mut t, 0, "vol", NodeKind::Volume, &[VOL]);
    let coll = node(&mut t, vol, "coll", NodeKind::Folder, &[COLL]);
    let rar = node(&mut t, coll, "grimlax.rar", NodeKind::Archive, &[RAR]);
    let inner = node(&mut t, rar, "grimlax", NodeKind::Folder, &[r"D:\Models\Vol\Coll\Grimlax.rar!Grimlax"]);
    let sarc = node(&mut t, inner, "sarcophagus", NodeKind::Folder, &[SARC]);
    node(&mut t, sarc, "body.stl", NodeKind::File, &[r"D:\Models\Vol\Coll\Grimlax.rar!Grimlax/Sarcophagus/body.stl"]);
    node(&mut t, inner, "extras", NodeKind::Folder, &[r"D:\Models\Vol\Coll\Grimlax.rar!Grimlax/Extras"]);
    t
}

/// Models view: `vol/coll/{ancient sarcophagus (model), other (model elsewhere)}`.
fn models() -> Tree {
    let mut t = Tree::new();
    let vol = node(&mut t, 0, "vol", NodeKind::Volume, &[VOL]);
    let coll = node(&mut t, vol, "coll", NodeKind::Collection, &[COLL]);
    node(&mut t, coll, "ancient sarcophagus", NodeKind::Model, &[SARC]);
    node(&mut t, coll, "other", NodeKind::Model, &[r"D:\Models\Vol\Coll\Other.zip"]);
    t
}

#[test]
fn a_model_maps_to_its_folder_in_files_and_back() {
    let (f, m) = (files(), models());
    assert_eq!(locate(&m, &["vol", "coll", "ancient sarcophagus"], &f), ["vol", "coll", "grimlax.rar", "grimlax", "sarcophagus"]);
    assert_eq!(locate(&f, &["vol", "coll", "grimlax.rar", "grimlax", "sarcophagus"], &m), ["vol", "coll", "ancient sarcophagus"]);
}

#[test]
fn a_folder_with_no_model_maps_to_its_nearest_container() {
    let (f, m) = (files(), models());
    assert_eq!(locate(&f, &["vol", "coll", "grimlax.rar", "grimlax", "extras"], &m), ["vol", "coll"]);
}

#[test]
fn a_container_with_nothing_above_it_maps_to_the_folder_below() {
    // Models has no node for the archive, but its model lives below it
    let mut m = Tree::new();
    node(&mut m, 0, "ancient sarcophagus", NodeKind::Model, &[SARC]);
    assert_eq!(locate(&files(), &["vol", "coll", "grimlax.rar"], &m), ["ancient sarcophagus"]);
}

#[test]
fn case_and_separators_do_not_matter() {
    let mut m = Tree::new();
    node(&mut m, 0, "x", NodeKind::Model, &[r"d:/models/vol/coll/grimlax.rar!grimlax\sarcophagus"]);
    assert_eq!(locate(&files(), &["vol", "coll", "grimlax.rar", "grimlax", "sarcophagus"], &m), ["x"]);
}

#[test]
fn nothing_in_common_goes_to_the_root() {
    let mut m = Tree::new();
    node(&mut m, 0, "x", NodeKind::Model, &[r"E:\Other"]);
    assert!(locate(&files(), &["vol", "coll"], &m).is_empty());
    assert!(locate(&files(), &[] as &[&str], &m).is_empty());
}

#[test]
fn a_gone_path_uses_what_still_exists() {
    let (f, m) = (files(), models());
    assert_eq!(locate(&f, &["vol", "coll", "gone", "x"], &m), ["vol", "coll"]);
}
