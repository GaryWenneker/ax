//! Import resolution - path aliases, workspace packages, relative modules.

use std::path::{Path, PathBuf};

/// One `compilerOptions.paths` entry. A pattern holds at most one `*`;
/// without one it matches only the exact import string.
struct PathAlias {
    prefix: String,
    /// `None` for an exact pattern, else the text after the `*`.
    suffix: Option<String>,
    /// Targets in config order, already joined to `baseUrl`.
    targets: Vec<String>,
}

impl PathAlias {
    /// The text the `*` stands for, or `""` for an exact match.
    fn capture<'a>(&self, import_path: &'a str) -> Option<&'a str> {
        match &self.suffix {
            None => (import_path == self.prefix).then_some(""),
            Some(suffix) => {
                let rest = import_path.strip_prefix(self.prefix.as_str())?;
                rest.strip_suffix(suffix.as_str())
            }
        }
    }
}

pub struct ImportResolver {
    project_root: PathBuf,
    /// Exact patterns first, then wildcard patterns by longest prefix, the
    /// order TypeScript uses, so the result never depends on hash order.
    path_aliases: Vec<PathAlias>,
    /// `compilerOptions.baseUrl`, relative to the project root, when set.
    base_url: Option<String>,
}

impl ImportResolver {
    pub fn new(project_root: &Path) -> Self {
        let mut resolver = Self {
            project_root: project_root.to_path_buf(),
            path_aliases: Vec::new(),
            base_url: None,
        };
        resolver.load_tsconfig(project_root);
        resolver.load_jsconfig(project_root);
        // Stable: equal prefixes keep tsconfig before jsconfig order.
        resolver.path_aliases.sort_by_key(|a| match a.suffix {
            None => (0, 0),
            Some(_) => (1, usize::MAX - a.prefix.len()),
        });
        resolver
    }

    pub fn project_root(&self) -> &Path {
        &self.project_root
    }

    fn load_tsconfig(&mut self, root: &Path) {
        let tsconfig = root.join("tsconfig.json");
        if tsconfig.exists() {
            self.load_paths_config(&tsconfig);
        }
    }
    fn load_jsconfig(&mut self, root: &Path) {
        let jsconfig = root.join("jsconfig.json");
        if jsconfig.exists() {
            self.load_paths_config(&jsconfig);
        }
    }

    fn load_paths_config(&mut self, path: &Path) {
        let Ok(content) = std::fs::read_to_string(path) else {
            return;
        };
        let stripped = strip_json_comments(&content);
        let Ok(json) = serde_json::from_str::<serde_json::Value>(&stripped) else {
            return;
        };
        let Some(options) = json.get("compilerOptions") else {
            return;
        };
        let base_url = options.get("baseUrl").and_then(|b| b.as_str());
        if let Some(b) = base_url {
            if self.base_url.is_none() {
                self.base_url = Some(normalize_rel(b));
            }
        }
        // Without baseUrl, targets are relative to the config file's folder.
        let base = normalize_rel(base_url.unwrap_or("."));
        let Some(map) = options.get("paths").and_then(|p| p.as_object()) else {
            return;
        };
        for (pattern, value) in map {
            let targets: Vec<String> = value
                .as_array()
                .map(|arr| {
                    arr.iter()
                        .filter_map(|t| t.as_str())
                        .map(|t| join_rel(&base, t))
                        .collect()
                })
                .unwrap_or_default();
            if targets.is_empty() {
                continue;
            }
            let (prefix, suffix) = match pattern.split_once('*') {
                Some((p, s)) => (p.to_string(), Some(s.to_string())),
                None => (pattern.clone(), None),
            };
            self.path_aliases.push(PathAlias {
                prefix,
                suffix,
                targets,
            });
        }
    }

    /// Resolve an import string to a project-relative file path if the file exists on disk.
    pub fn resolve_to_indexed_file(&self, from_file: &str, import_path: &str) -> Option<String> {
        let import_path = clean_import_path(import_path);
        if import_path.is_empty() {
            return None;
        }

        for logical in self.resolve_candidates(&import_path, from_file) {
            for cand in self.file_candidates(&logical) {
                let full = self.project_root.join(&cand);
                if full.is_file() {
                    return Some(self.to_indexed_rel_path(&full));
                }
            }
        }
        None
    }

    /// The first logical path an import maps to (see [`Self::resolve_candidates`]).
    pub fn resolve_import(&self, import_path: &str, from_file: &str) -> Option<String> {
        self.resolve_candidates(import_path, from_file)
            .into_iter()
            .next()
    }

    /// Every logical path an import may map to, in the order to try them:
    /// the targets of the best matching `paths` pattern, else the relative
    /// path, else the path under `baseUrl`.
    pub fn resolve_candidates(&self, import_path: &str, from_file: &str) -> Vec<String> {
        for alias in &self.path_aliases {
            if let Some(star) = alias.capture(import_path) {
                return alias
                    .targets
                    .iter()
                    .map(|t| t.replacen('*', star, 1))
                    .collect();
            }
        }
        if import_path.starts_with('.') {
            let from_dir = Path::new(from_file).parent().unwrap_or(Path::new(""));
            let joined = from_dir.join(import_path);
            return vec![joined.to_string_lossy().replace('\\', "/")];
        }
        match &self.base_url {
            Some(base) => vec![join_rel(base, import_path)],
            None => Vec::new(),
        }
    }

    fn file_candidates(&self, logical: &str) -> Vec<String> {
        let base = logical.replace('\\', "/");
        let mut out = vec![
            base.clone(),
            format!("{}.ts", base),
            format!("{}.tsx", base),
            format!("{}.js", base),
            format!("{}.mjs", base),
            format!("{}/index.ts", base),
            format!("{}/index.tsx", base),
            format!("{}/index.js", base),
        ];
        if base.ends_with(".ts") || base.ends_with(".tsx") || base.ends_with(".js") {
            out.insert(0, base);
        }
        out
    }

    /// Match orchestrator-relative paths (e.g. `greet.ts`, not `./greet.ts`).
    fn to_indexed_rel_path(&self, full: &Path) -> String {
        full.strip_prefix(&self.project_root)
            .unwrap_or(full)
            .to_string_lossy()
            .replace('\\', "/")
            .trim_start_matches("./")
            .to_string()
    }
}

/// A config-relative path without `./` segments (`./web/` becomes `web`).
fn normalize_rel(path: &str) -> String {
    let path = path.replace('\\', "/");
    let mut parts: Vec<&str> = Vec::new();
    for seg in path.split('/') {
        match seg {
            "" | "." => {}
            ".." if parts.last().is_some_and(|p| *p != "..") => {
                parts.pop();
            }
            _ => parts.push(seg),
        }
    }
    if parts.is_empty() {
        ".".to_string()
    } else {
        parts.join("/")
    }
}

fn join_rel(base: &str, path: &str) -> String {
    if base == "." {
        normalize_rel(path)
    } else {
        normalize_rel(&format!("{base}/{path}"))
    }
}

fn clean_import_path(import_path: &str) -> String {
    import_path
        .trim()
        .trim_matches('"')
        .trim_matches('\'')
        .to_string()
}

/// Strip `//` and `/* */` comments so tsconfig/jsconfig with comments parse (CG jsonc-parser).
fn strip_json_comments(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let bytes = input.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'"' {
            out.push('"');
            i += 1;
            while i < bytes.len() {
                let c = bytes[i];
                out.push(c as char);
                if c == b'\\' && i + 1 < bytes.len() {
                    i += 1;
                    out.push(bytes[i] as char);
                } else if c == b'"' {
                    break;
                }
                i += 1;
            }
            i += 1;
        } else if bytes.get(i..i + 2) == Some(b"//") {
            while i < bytes.len() && bytes[i] != b'\n' {
                i += 1;
            }
        } else if bytes.get(i..i + 2) == Some(b"/*") {
            i += 2;
            while i + 1 < bytes.len() && bytes.get(i..i + 2) != Some(b"*/") {
                i += 1;
            }
            i += 2;
        } else {
            out.push(bytes[i] as char);
            i += 1;
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn strip_json_comments_valid_json() {
        let raw = r#"{
  // alias
  "compilerOptions": { "paths": { "@app/*": ["src/*"] } }
}"#;
        let stripped = strip_json_comments(raw);
        assert!(serde_json::from_str::<serde_json::Value>(&stripped).is_ok());
    }

    #[test]
    fn tsconfig_paths_resolve_with_comments() {
        let dir = tempfile::tempdir().expect("tempdir");
        let ts = r#"{
  // path maps
  "compilerOptions": { "paths": { "@app/*": ["src/*"] } }
}"#;
        fs::write(dir.path().join("tsconfig.json"), ts).expect("write");
        fs::create_dir_all(dir.path().join("src")).expect("mkdir");
        fs::write(dir.path().join("src/foo.ts"), "export const x = 1").expect("write");
        let resolver = ImportResolver::new(dir.path());
        let resolved = resolver.resolve_to_indexed_file("src/main.ts", "@app/foo");
        assert_eq!(resolved, Some("src/foo.ts".to_string()));
    }
}

#[cfg(test)]
mod alias_tests {
    use super::*;
    use std::fs;

    fn project(tsconfig: &str, files: &[&str]) -> tempfile::TempDir {
        let dir = tempfile::tempdir().expect("tempdir");
        fs::write(dir.path().join("tsconfig.json"), tsconfig).expect("write");
        for f in files {
            let full = dir.path().join(f);
            fs::create_dir_all(full.parent().unwrap()).expect("mkdir");
            fs::write(full, "export const x = 1").expect("write");
        }
        dir
    }

    #[test]
    fn longest_alias_wins_every_time() {
        let dir = project(
            r#"{"compilerOptions":{"paths":{"@/*":["src/*"],"@app/*":["app/*"]}}}"#,
            &["src/app/foo.ts", "app/foo.ts", "src/util.ts"],
        );
        // Each resolver has its own hash seed, so a hash-order bug shows up.
        for _ in 0..32 {
            let r = ImportResolver::new(dir.path());
            assert_eq!(
                r.resolve_to_indexed_file("src/main.ts", "@app/foo"),
                Some("app/foo.ts".to_string())
            );
            assert_eq!(
                r.resolve_to_indexed_file("src/main.ts", "@/util"),
                Some("src/util.ts".to_string())
            );
        }
    }

    #[test]
    fn alias_matches_only_on_a_segment_boundary() {
        let dir = project(
            r#"{"compilerOptions":{"paths":{"@/*":["src/*"],"@lib/*":["lib/*"]}}}"#,
            &["src/nestjs/common.ts", "src/libx/a.ts"],
        );
        let r = ImportResolver::new(dir.path());
        assert_eq!(r.resolve_import("@nestjs/common", "src/main.ts"), None);
        assert_eq!(r.resolve_import("@libx/a", "src/main.ts"), None);
    }

    #[test]
    fn exact_alias_matches_only_itself() {
        let dir = project(
            r#"{"compilerOptions":{"paths":{"config":["src/config/index.ts"]}}}"#,
            &["src/config/index.ts"],
        );
        let r = ImportResolver::new(dir.path());
        assert_eq!(
            r.resolve_to_indexed_file("src/main.ts", "config"),
            Some("src/config/index.ts".to_string())
        );
        assert_eq!(r.resolve_import("config/other", "src/main.ts"), None);
    }

    #[test]
    fn every_target_is_tried_in_order() {
        let dir = project(
            r#"{"compilerOptions":{"paths":{"@lib/*":["missing/*","lib/*"]}}}"#,
            &["lib/x.ts"],
        );
        let r = ImportResolver::new(dir.path());
        assert_eq!(
            r.resolve_to_indexed_file("src/main.ts", "@lib/x"),
            Some("lib/x.ts".to_string())
        );
    }

    #[test]
    fn targets_are_relative_to_base_url() {
        let dir = project(
            r#"{"compilerOptions":{"baseUrl":"./web","paths":{"~/*":["src/*"]}}}"#,
            &["web/src/a.ts"],
        );
        let r = ImportResolver::new(dir.path());
        assert_eq!(
            r.resolve_to_indexed_file("web/src/main.ts", "~/a"),
            Some("web/src/a.ts".to_string())
        );
        // A bare import is looked up under baseUrl too.
        assert_eq!(
            r.resolve_to_indexed_file("web/src/main.ts", "src/a"),
            Some("web/src/a.ts".to_string())
        );
    }

    #[test]
    fn cargo_projects_get_no_made_up_alias() {
        let dir = tempfile::tempdir().expect("tempdir");
        fs::write(dir.path().join("Cargo.toml"), "[package]\nname = \"a\"\n").expect("write");
        fs::write(dir.path().join("foo.ts"), "export const x = 1").expect("write");
        let r = ImportResolver::new(dir.path());
        assert_eq!(r.resolve_to_indexed_file("src/main.ts", "@crate/foo"), None);
    }
}
