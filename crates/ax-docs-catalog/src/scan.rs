//! Scan wiki clone and workspace documentation sources.

use std::path::Path;

use walkdir::WalkDir;

pub fn wiki_page_paths(root: &Path) -> Vec<String> {
    if !root.is_dir() {
        return vec![];
    }
    WalkDir::new(root)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().is_file())
        .filter(|e| {
            e.path()
                .extension()
                .is_some_and(|ext| ext.eq_ignore_ascii_case("md"))
        })
        .filter_map(|e| {
            e.path()
                .strip_prefix(root)
                .ok()
                .map(|p| p.to_string_lossy().replace('\\', "/"))
        })
        .collect()
}

pub fn wiki_top_sections(apps_root: &Path) -> Vec<String> {
    if !apps_root.is_dir() {
        return vec![];
    }
    let mut sections = Vec::new();
    if let Ok(read_dir) = std::fs::read_dir(apps_root) {
        for entry in read_dir.flatten() {
            let path = entry.path();
            if path.is_dir() {
                if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                    sections.push(format!("{name}/"));
                }
            } else if path
                .extension()
                .is_some_and(|ext| ext.eq_ignore_ascii_case("md"))
            {
                if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                    sections.push(name.to_string());
                }
            }
        }
    }
    sections.sort();
    sections
}

/// First-cell values of every Markdown table row in `path`, minus header rows,
/// separator rows, cells starting with `--`, and values listed in `skip`.
pub fn product_names(path: &Path, skip: &[String]) -> Vec<String> {
    let Ok(content) = std::fs::read_to_string(path) else {
        return vec![];
    };
    let lines: Vec<&str> = content.lines().map(str::trim).collect();
    let mut names = Vec::new();
    for (i, line) in lines.iter().enumerate() {
        if !line.starts_with('|') || is_separator_row(line) {
            continue;
        }
        if lines.get(i + 1).is_some_and(|next| is_separator_row(next)) {
            continue;
        }
        if let Some(name) = parse_table_first_cell(line) {
            if name.starts_with("--") || skip.iter().any(|s| s == &name) {
                continue;
            }
            names.push(name);
        }
    }
    names
}

fn is_separator_row(line: &str) -> bool {
    line.starts_with('|')
        && line.contains('-')
        && line
            .chars()
            .all(|c| c == '|' || c == '-' || c == ':' || c.is_whitespace())
}

fn parse_table_first_cell(line: &str) -> Option<String> {
    // | **Bold Name** | col2 |
    if let Some(rest) = line.strip_prefix('|') {
        let cell = rest.split('|').next()?.trim();
        let name = cell.trim_start_matches("**").trim_end_matches("**").trim();
        if name.is_empty() {
            return None;
        }
        return Some(name.to_string());
    }
    None
}

pub fn docs_sections(docs_root: &Path) -> Vec<String> {
    if !docs_root.is_dir() {
        return vec![];
    }
    let mut sections = Vec::new();
    if let Ok(read_dir) = std::fs::read_dir(docs_root) {
        for entry in read_dir.flatten() {
            if entry.path().is_dir() {
                if let Some(name) = entry.file_name().to_str() {
                    sections.push(format!("{name}/"));
                }
            }
        }
    }
    sections.sort();
    sections
}

pub fn skill_names(skills_root: &Path) -> Vec<String> {
    if !skills_root.is_dir() {
        return vec![];
    }
    let mut names = Vec::new();
    if let Ok(read_dir) = std::fs::read_dir(skills_root) {
        for entry in read_dir.flatten() {
            let path = entry.path();
            if path.is_dir() && path.join("SKILL.md").is_file() {
                if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                    names.push(name.to_string());
                }
            }
        }
    }
    names.sort();
    names
}

pub fn script_dirs_with_readme(scripts_root: &Path) -> Vec<String> {
    if !scripts_root.is_dir() {
        return vec![];
    }
    let mut dirs = Vec::new();
    if let Ok(read_dir) = std::fs::read_dir(scripts_root) {
        for entry in read_dir.flatten() {
            let path = entry.path();
            if path.is_dir() && path.join("README.md").is_file() {
                if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                    dirs.push(format!("{name}/"));
                }
            }
        }
    }
    dirs.sort();
    dirs
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn products_file(tag: &str, lines: &[&str]) -> (std::path::PathBuf, std::path::PathBuf) {
        let dir = std::env::temp_dir().join(format!(
            "ax-docs-catalog-products-{tag}-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("Products.md");
        let mut f = std::fs::File::create(&path).unwrap();
        for line in lines {
            writeln!(f, "{line}").unwrap();
        }
        (dir, path)
    }

    #[test]
    fn products_parses_plain_and_bold_rows_and_skips_header() {
        let (dir, path) = products_file(
            "plain",
            &[
                "| Product | Repo |",
                "| --- | --- |",
                "|Contoso Portal|contoso-portal|",
                "| **Fabrikam App** | fabrikam |",
            ],
        );
        let names = product_names(&path, &[]);
        let _ = std::fs::remove_dir_all(&dir);
        assert_eq!(
            names,
            vec!["Contoso Portal".to_string(), "Fabrikam App".to_string()]
        );
    }

    #[test]
    fn products_skips_configured_values_and_double_dash_cells() {
        let (dir, path) = products_file(
            "skip",
            &[
                "| Name | Repo |",
                "|:---|---:|",
                "| Contoso Portal | a |",
                "| Name -- Shared | |",
                "| -- retired | |",
                "| Fabrikam App | b |",
            ],
        );
        let names = product_names(&path, &["Name -- Shared".to_string()]);
        let _ = std::fs::remove_dir_all(&dir);
        assert_eq!(
            names,
            vec!["Contoso Portal".to_string(), "Fabrikam App".to_string()]
        );
    }

    #[test]
    fn products_missing_file_is_empty() {
        assert!(product_names(Path::new("/nonexistent/Products.md"), &[]).is_empty());
    }
}
