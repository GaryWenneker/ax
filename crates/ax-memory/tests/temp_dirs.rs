//! Gate: tests create their scratch directories with `tempfile::tempdir()`,
//! which removes them on drop, even when the test panics. A hand-built
//! `temp_dir()` + pid path is left behind by any failing assert.

#[test]
fn tests_use_tempfile_not_a_hand_built_temp_dir() {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests");
    let needle = ["std::env::", "temp_dir()"].concat();
    let mut offenders = Vec::new();
    for entry in std::fs::read_dir(&dir).expect("tests dir") {
        let path = entry.expect("entry").path();
        if path.extension().is_some_and(|e| e == "rs") {
            let text = std::fs::read_to_string(&path).expect("read test file");
            if text.contains(&needle) {
                offenders.push(path.file_name().unwrap().to_string_lossy().to_string());
            }
        }
    }
    offenders.sort();
    assert!(
        offenders.is_empty(),
        "use tempfile::tempdir() in {offenders:?}"
    );
}
