//! Import resolution - path aliases, workspace packages, relative modules.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

pub struct ImportResolver {
    project_root: PathBuf,
    path_aliases: HashMap<String, String>,
}

impl ImportResolver {
    pub fn new(project_root: &Path) -> Self {
        let mut resolver = Self {
            project_root: project_root.to_path_buf(),
            path_aliases: HashMap::new(),
        };
        resolver.load_tsconfig(project_root);
        resolver.load_jsconfig(project_root);
        resolver.load_cargo_workspace(project_root);
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
        if let Ok(content) = std::fs::read_to_string(path) {
            let stripped = strip_json_comments(&content);
            if let Ok(json) = serde_json::from_str::<serde_json::Value>(&stripped) {
                if let Some(paths) = json.get("compilerOptions").and_then(|c| c.get("paths")) {
                    if let Some(map) = paths.as_object() {
                        for (k, v) in map {
                            if let Some(arr) = v.as_array() {
                                if let Some(first) = arr.first().and_then(|x| x.as_str()) {
                                    let alias = k.replace("/*", "");
                                    let target = first.replace("/*", "");
                                    self.path_aliases.insert(alias, target);
                                }
                            }
                        }
                    }
                }
            }
        }
    }


    fn load_cargo_workspace(&mut self, root: &Path) {
        let cargo = root.join("Cargo.toml");
        if cargo.exists() {
            self.path_aliases.insert("@crate".to_string(), ".".to_string());
        }
    }

    /// Resolve an import string to a project-relative file path if the file exists on disk.
    pub fn resolve_to_indexed_file(&self, from_file: &str, import_path: &str) -> Option<String> {
        let import_path = clean_import_path(import_path);
        if import_path.is_empty() {
            return None;
        }

        let logical = self.resolve_import(&import_path, from_file)?;
        let candidates = self.file_candidates(&logical);
        for cand in candidates {
            let full = self.project_root.join(&cand);
            if full.is_file() {
                return Some(self.to_indexed_rel_path(&full));
            }
        }
        None
    }

    pub fn resolve_import(&self, import_path: &str, from_file: &str) -> Option<String> {
        for (alias, target) in &self.path_aliases {
            if import_path.starts_with(alias) {
                let rest = import_path.strip_prefix(alias).unwrap_or("").trim_start_matches('/');
                if rest.is_empty() {
                    return Some(target.clone());
                }
                let base = target.trim_end_matches('/');
                return Some(format!("{}/{}", base, rest));
            }
        }
        if import_path.starts_with('.') {
            let from_dir = Path::new(from_file).parent().unwrap_or(Path::new(""));
            let joined = from_dir.join(import_path);
            return Some(joined.to_string_lossy().replace('\\', "/"));
        }
        None
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

fn clean_import_path(import_path: &str) -> String {
    import_path.trim().trim_matches('"').trim_matches('\'').to_string()
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