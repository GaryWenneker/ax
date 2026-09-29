//! `AX_HOME_DIR` is process-global, so both cases run in one test.

use std::path::PathBuf;

use ax_utils::paths::home_dir;

#[test]
fn ax_home_dir_overrides_the_system_home() {
    std::env::set_var("AX_HOME_DIR", "/tmp/ax-home-test");
    assert_eq!(home_dir(), Some(PathBuf::from("/tmp/ax-home-test")));

    std::env::set_var("AX_HOME_DIR", "");
    assert_eq!(home_dir(), dirs::home_dir(), "an empty override is ignored");

    std::env::remove_var("AX_HOME_DIR");
    assert_eq!(home_dir(), dirs::home_dir());
}
