//! `unsafe_index_root_reason` refuses filesystem roots and the home directory,
//! however the path or HOME is spelled.
//!
//! One test function: it sets HOME, which is process-wide.

#[cfg(unix)]
#[test]
fn home_and_roots_are_refused_however_spelled() {
    use ax_context::directory::unsafe_index_root_reason;

    let tmp = tempfile::tempdir().unwrap();
    let real_home = tmp.path().join("home-real");
    std::fs::create_dir_all(real_home.join("project")).unwrap();
    let link = tmp.path().join("home-link");
    std::os::unix::fs::symlink(&real_home, &link).unwrap();

    std::env::remove_var("USERPROFILE");

    // HOME is a symlink to the real home directory.
    std::env::set_var("HOME", &link);
    assert!(
        unsafe_index_root_reason(&real_home).is_some(),
        "home reached through its real path"
    );
    assert!(
        unsafe_index_root_reason(&link).is_some(),
        "home through HOME"
    );

    // HOME with a trailing slash.
    std::env::set_var("HOME", format!("{}/", real_home.display()));
    assert!(
        unsafe_index_root_reason(&real_home).is_some(),
        "trailing slash"
    );

    // A project below home and the root itself.
    assert_eq!(unsafe_index_root_reason(&real_home.join("project")), None);
    assert!(unsafe_index_root_reason(std::path::Path::new("/")).is_some());
}
