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
