//! diff_vs_base against real temporary git repositories.

use std::path::{Path, PathBuf};
use std::process::Command;

struct TempRepo(PathBuf);

impl Drop for TempRepo {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn git(dir: &Path, args: &[&str]) {
    let out = Command::new("git")
        .current_dir(dir)
        .args(args)
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .output()
        .expect("git runs");
    assert!(
        out.status.success(),
        "git {args:?}: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}

fn write(dir: &Path, rel: &str, body: &str) {
    let p = dir.join(rel);
    std::fs::create_dir_all(p.parent().unwrap()).unwrap();
    std::fs::write(p, body).unwrap();
}

fn repo(name: &str) -> TempRepo {
    let dir = std::env::temp_dir().join(format!(
        "ax-git-diff-{name}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(&dir).unwrap();
    git(&dir, &["init", "-q", "-b", "main"]);
    git(&dir, &["config", "user.email", "t@example.com"]);
    git(&dir, &["config", "user.name", "t"]);
    git(&dir, &["config", "commit.gpgsign", "false"]);
    TempRepo(dir)
}

const BODY: &str = "pub fn run() {\n    let a = 1;\n    let b = 2;\n    let c = 3;\n    let d = 4;\n    let e = 5;\n}\n";

#[test]
fn renamed_copied_and_non_ascii_paths_are_reported_as_plain_paths() {
    let r = repo("paths");
    let d = &r.0;
    write(d, "src/a.rs", &BODY.repeat(5));
    write(d, "src/keep.rs", BODY);
    git(d, &["add", "."]);
    git(d, &["commit", "-q", "-m", "base"]);
    git(d, &["branch", "base"]);
    git(d, &["mv", "src/a.rs", "src/b.rs"]);
    write(d, "src/café.rs", BODY);
    write(d, "src/my file.rs", BODY);
    git(d, &["add", "."]);
    git(d, &["commit", "-q", "-m", "change"]);

    let diff = ax_git::diff_vs_base(d, "base").unwrap();
    let paths: Vec<&str> = diff.files.iter().map(|f| f.path.as_str()).collect();
    assert!(paths.contains(&"src/b.rs"), "{paths:?}");
    assert!(paths.contains(&"src/café.rs"), "{paths:?}");
    assert!(paths.contains(&"src/my file.rs"), "{paths:?}");
    let renamed = diff.files.iter().find(|f| f.path == "src/b.rs").unwrap();
    assert_eq!(renamed.change, "renamed");
    assert!(paths.iter().all(|p| !p.contains('\t') && !p.contains('"')));

    let hunk_paths: Vec<&str> = diff.hunks.iter().map(|h| h.path.as_str()).collect();
    assert!(hunk_paths.contains(&"src/café.rs"), "{hunk_paths:?}");
    assert!(hunk_paths.contains(&"src/my file.rs"), "{hunk_paths:?}");
}

#[test]
fn user_prefix_config_does_not_drop_hunks() {
    let r = repo("noprefix");
    let d = &r.0;
    write(d, "src/lib.rs", BODY);
    git(d, &["add", "."]);
    git(d, &["commit", "-q", "-m", "base"]);
    git(d, &["branch", "base"]);
    write(d, "src/lib.rs", &BODY.replace("let c = 3;", "let c = 30;"));
    git(d, &["commit", "-qam", "change"]);
    git(d, &["config", "diff.noprefix", "true"]);

    let diff = ax_git::diff_vs_base(d, "base").unwrap();
    assert_eq!(diff.hunks.len(), 1, "{:?}", diff.hunks);
    assert_eq!(diff.hunks[0].path, "src/lib.rs");
    assert_eq!(diff.hunks[0].new_start, 4);
}

#[test]
fn deleted_file_hunks_are_not_given_to_the_previous_file() {
    let r = repo("deleted");
    let d = &r.0;
    write(d, "src/a.rs", BODY);
    write(d, "src/z.rs", BODY);
    git(d, &["add", "."]);
    git(d, &["commit", "-q", "-m", "base"]);
    git(d, &["branch", "base"]);
    write(d, "src/a.rs", &BODY.replace("let a = 1;", "let a = 10;"));
    git(d, &["rm", "-q", "src/z.rs"]);
    git(d, &["commit", "-qam", "change"]);

    let diff = ax_git::diff_vs_base(d, "base").unwrap();
    let for_a: Vec<_> = diff.hunks.iter().filter(|h| h.path == "src/a.rs").collect();
    assert_eq!(for_a.len(), 1, "{:?}", diff.hunks);
    assert!(diff
        .hunks
        .iter()
        .all(|h| h.path != "src/z.rs" || h.new_lines == 0));
}
