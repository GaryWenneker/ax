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

pub fn digitale_producten_names(path: &Path) -> Vec<String> {
    if !path.is_file() {
        return vec![];
    }
    let content = match std::fs::read_to_string(path) {
        Ok(c) => c,
        Err(_) => return vec![],
    };
    let skip_headers = [
        "Naam",
        "Naam -- VfPf Shared",
        "Naam -- Overig",
        "Naam -- Onbekend",
        "-",
        "actief",
    ];
    let mut names = Vec::new();
    for line in content.lines() {
        let line = line.trim();
        if !line.starts_with('|') {
            continue;
        }
        // Table separator row
        if line.chars().all(|c| c == '|' || c == '-' || c == ':' || c.is_whitespace()) {
            continue;
        }
        if let Some(name) = parse_table_first_cell(line) {
            if skip_headers.contains(&name.as_str()) {
                continue;
            }
            if name.starts_with("--") {
                continue;
            }
            names.push(name);
        }
    }
    names
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

    #[test]
    fn digitale_producten_parses_plain_table_rows() {
        let dir = std::env::temp_dir().join("ax-docs-catalog-test");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("Digitale-Producten.md");
        {
            let mut f = std::fs::File::create(&path).unwrap();
            writeln!(f, "| Naam | Repo |").unwrap();
            writeln!(f, "| --- | --- |").unwrap();
            writeln!(f, "|Adviseurportaal|AdviseurPortaal|").unwrap();
            writeln!(f, "| **Klantbeeld** | Klantbeeld |").unwrap();
        }
        let names = digitale_producten_names(&path);
        let _ = std::fs::remove_dir_all(&dir);
        assert!(names.contains(&"Adviseurportaal".to_string()));
        assert!(names.contains(&"Klantbeeld".to_string()));
    }
}
