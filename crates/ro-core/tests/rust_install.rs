use std::path::Path;

use ro_core::install::RustInstall;

#[test]
fn skins_live_beside_the_install_rather_than_inside_it() {
    // steamapps/common/Rust and steamapps/workshop are siblings, so the cache is two levels up
    // and back down. Reached from the install so it cannot disagree about which Steam library the
    // game is in.
    let install = RustInstall::new(Path::new("C:").join("Steam").join("steamapps").join("common").join("Rust"));

    let expected = Path::new("C:").join("Steam").join("steamapps").join("workshop").join("content").join("252490");
    assert_eq!(install.skins_directory(), std::path::absolute(expected).unwrap());
}

#[test]
fn an_absent_cache_is_reported_rather_than_thrown() {
    let dir = tempfile::tempdir().unwrap();
    let install = RustInstall::new(dir.path().join("missing"));

    assert!(!install.has_skins());
    assert!(!install.has_item_bundles());
}
