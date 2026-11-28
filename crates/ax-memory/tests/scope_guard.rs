use ax_memory::scope_paths;
use std::path::PathBuf;

#[test]
fn absolute_paths_outside_the_project_are_rejected() {
    let root = PathBuf::from("/work/app");
    assert!(scope_paths(&root, &[]).is_empty());
    assert!(scope_paths(&root, &["src/a.rs".into()]).is_empty());
    assert!(scope_paths(&root, &["/work/app/src/a.rs".into()]).is_empty());
    let foreign = scope_paths(&root, &["/other/project/src/a.rs".into()]);
    assert_eq!(foreign, vec!["/other/project/src/a.rs".to_string()]);
}

#[test]
fn parent_components_escape_the_project_even_in_absolute_paths() {
    let root = PathBuf::from("/work/app");
    for path in [
        "/work/app/../../other/secret.env",
        "/work/app/src/../../other.rs",
        "../outside.rs",
        "src/../../outside.rs",
    ] {
        assert_eq!(
            scope_paths(&root, &[path.into()]),
            vec![path.to_string()],
            "{path}"
        );
    }
}

#[cfg(windows)]
#[test]
fn drive_relative_paths_are_rejected() {
    let root = PathBuf::from(r"C:\work\app");
    let foreign = scope_paths(&root, &["C:other.rs".into(), "D:x".into()]);
    assert_eq!(foreign.len(), 2);
    assert!(scope_paths(&root, &[r"C:\work\app\src\a.rs".into()]).is_empty());
}
