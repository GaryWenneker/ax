//! Gate: the query path must not read the filesystem.
//!
//! A graph query has to be answerable from `ax.db`. If a query-path module reads
//! the working tree, snippet freshness stops being checkable and query cost
//! starts depending on a filesystem sweep — the failure this repo audited in
//! `docs/audits/2026-08-19-preflight-graph-only/`.
//!
//! Run: cargo test -p ax-context --test no_query_time_disk_reads
//!
//! **Fail closed.** A missing or unreadable file is a failure, never a pass: a
//! gate that silently skips its input is worse than no gate, because it reports
//! green while checking nothing.
//!
//! **What this proves, precisely.** It proves these specific spellings do not
//! appear in these specific files. It does not prove no disk read is reachable —
//! a read through a helper crate, a macro, or a differently-named API would pass.
//! It catches the regression that actually happened (someone reaching for
//! `fs::read_to_string` in a snippet path) and nothing more.

use std::path::{Path, PathBuf};

/// Query-path modules: these run while answering an agent's question.
///
/// `directory.rs` is excluded deliberately — locating `.ax/ax.db` is filesystem
/// work by definition and happens before any query. Index-time crates
/// (`ax-extraction`) are out of scope for the same reason: they must walk.
const QUERY_PATH_FILES: &[&str] = &[
    "src/explore.rs",
    "src/builder.rs",
    "src/source_store.rs",
    "src/formatter.rs",
    "src/explore_format.rs",
    "src/markers.rs",
];

/// Spellings that read the filesystem. Substring match, so `std::fs::read_to_string`
/// and a bare `read_to_string(` are both caught.
const FORBIDDEN: &[&str] = &[
    "read_to_string",
    "File::open",
    "fs::read",
    "read_dir",
    "WalkBuilder",
    "WalkDir",
    "std::fs::metadata",
    "fs::write",
    "include_str!",
];

fn crate_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

/// Read a gate input. Any failure aborts the gate — see the fail-closed note.
fn read_checked(path: &Path) -> String {
    match std::fs::read_to_string(path) {
        Ok(s) => s,
        Err(e) => panic!(
            "gate cannot read {} ({e}). Fix the path list in this test rather than \
             letting the gate skip a file — a skipped file passes silently.",
            path.display()
        ),
    }
}

/// Strip `//` and `/* */` so a forbidden word inside prose (including this
/// test's own explanations) cannot trip the gate. Deliberately simple: it does
/// not understand string literals, so a forbidden spelling inside a string still
/// fails, which is the safe direction.
fn strip_comments(src: &str) -> String {
    let mut out = String::with_capacity(src.len());
    let mut chars = src.chars().peekable();
    let mut in_block = false;
    while let Some(c) = chars.next() {
        if in_block {
            if c == '*' && chars.peek() == Some(&'/') {
                chars.next();
                in_block = false;
            }
            continue;
        }
        if c == '/' {
            match chars.peek() {
                Some('/') => {
                    for n in chars.by_ref() {
                        if n == '\n' {
                            out.push('\n');
                            break;
                        }
                    }
                    continue;
                }
                Some('*') => {
                    chars.next();
                    in_block = true;
                    continue;
                }
                _ => {}
            }
        }
        out.push(c);
    }
    out
}

fn find_violations(code: &str) -> Vec<(usize, String, String)> {
    let mut hits = Vec::new();
    for (i, line) in code.lines().enumerate() {
        for pattern in FORBIDDEN {
            if line.contains(pattern) {
                hits.push((i + 1, (*pattern).to_string(), line.trim().to_string()));
            }
        }
    }
    hits
}

#[test]
fn query_path_modules_do_not_touch_the_filesystem() {
    assert!(
        !QUERY_PATH_FILES.is_empty(),
        "the gate has no files to check — it would pass vacuously"
    );
    assert!(
        !FORBIDDEN.is_empty(),
        "the gate has no patterns to enforce — it would pass vacuously"
    );

    let root = crate_root();
    let mut failures: Vec<String> = Vec::new();

    for rel in QUERY_PATH_FILES {
        let path = root.join(rel);
        if !path.is_file() {
            panic!(
                "gate input {rel} does not exist. Update QUERY_PATH_FILES when \
                 modules move; do not let the gate check nothing."
            );
        }
        let code = strip_comments(&read_checked(&path));
        for (line_no, pattern, text) in find_violations(&code) {
            failures.push(format!("{rel}:{line_no}: `{pattern}` in `{text}`"));
        }
    }

    assert!(
        failures.is_empty(),
        "query-path modules must serve source from ax.db, not the filesystem.\n\
         Use ax_context::source_store instead.\n\n{}",
        failures.join("\n")
    );
}

/// Every module in `src/` is either checked or explicitly exempt. Without this,
/// adding a new query-path module quietly escapes the gate.
#[test]
fn every_source_module_is_classified() {
    /// Exempt, with the reason it cannot be graph-only.
    const EXEMPT: &[(&str, &str)] = &[
        ("src/lib.rs", "module declarations only"),
        ("src/directory.rs", "locating .ax/ax.db is filesystem work by design"),
    ];

    let src = crate_root().join("src");
    let entries = match std::fs::read_dir(&src) {
        Ok(e) => e,
        Err(e) => panic!("gate cannot list {} ({e})", src.display()),
    };

    let mut unclassified = Vec::new();
    for entry in entries {
        let entry = entry.expect("read_dir entry");
        let name = entry.file_name().to_string_lossy().to_string();
        if !name.ends_with(".rs") {
            continue;
        }
        let rel = format!("src/{name}");
        let checked = QUERY_PATH_FILES.contains(&rel.as_str());
        let exempt = EXEMPT.iter().any(|(p, _)| *p == rel);
        if !checked && !exempt {
            unclassified.push(rel);
        }
    }

    assert!(
        unclassified.is_empty(),
        "new ax-context module(s) are neither gated nor exempt: {}\n\
         Add them to QUERY_PATH_FILES, or to EXEMPT with a reason.",
        unclassified.join(", ")
    );
}

/// The gate's own machinery must be able to fail. This is the negative control
/// kept in the repo: it feeds known-bad source through the same matcher the real
/// test uses and asserts it is rejected.
///
/// Note what this does and does not buy: it proves the matcher reaches its
/// failure path for these spellings. It does not prove the matcher recognises
/// every possible filesystem read.
#[test]
fn gate_rejects_known_bad_source() {
    let bad = "let content = std::fs::read_to_string(&full).unwrap();";
    let hits = find_violations(&strip_comments(bad));
    assert!(!hits.is_empty(), "matcher failed to flag a plain disk read");

    for spelling in [
        "let f = File::open(p)?;",
        "let b = fs::read(p)?;",
        "for e in read_dir(p)? {}",
        "WalkBuilder::new(root).build()",
    ] {
        assert!(
            !find_violations(&strip_comments(spelling)).is_empty(),
            "matcher failed to flag: {spelling}"
        );
    }

    // And it must not flag clean graph-only code.
    let good = "let resolved = resolve_source(&self.queries, &node.file_path).await?;";
    assert!(
        find_violations(&strip_comments(good)).is_empty(),
        "matcher flagged clean graph-only code — it would block correct work"
    );

    // Prose must not trip the gate, or every doc comment becomes a violation.
    let commented = "// we deliberately avoid read_to_string here\nlet x = 1;";
    assert!(
        find_violations(&strip_comments(commented)).is_empty(),
        "comment stripping is not working; the gate would fire on documentation"
    );
}
