use crate::validate::read_catalog_meta;
use std::fs;
use std::path::{Component, Path, PathBuf};
use walkdir::WalkDir;

pub const COPY_MAX_FILES: usize = 1000;
pub const COPY_MAX_BYTES: u64 = 25 * 1024 * 1024;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InstallReceipt {
    pub name: String,
    pub hash: String,
    pub directory: PathBuf,
}

#[derive(Debug, Clone)]
pub struct FoundSkill {
    pub name: String,
    pub directory: PathBuf,
}

pub fn discover_skills(root: &Path) -> Result<Vec<FoundSkill>, String> {
    discover_skills_for(root, None)
}

fn discover_skills_for(root: &Path, only: Option<&str>) -> Result<Vec<FoundSkill>, String> {
    if !root.exists() {
        return Err(format!("skill source does not exist: {}", root.display()));
    }
    let mut found = Vec::new();
    let walker = WalkDir::new(root)
        .max_depth(6)
        .into_iter()
        .filter_entry(|e| {
            let name = e.file_name().to_string_lossy();
            if name == ".git" || name == "node_modules" {
                return false;
            }
            if e.file_type().is_dir() {
                if let Some(parent) = e.path().parent() {
                    if parent.join("SKILL.md").is_file() {
                        return false;
                    }
                }
            }
            true
        });
    for entry in walker {
        let entry = entry.map_err(|e| e.to_string())?;
        if !entry.file_type().is_file() || entry.file_name() != "SKILL.md" {
            continue;
        }
        let dir = entry.path().parent().unwrap_or(root);
        let dir_name = dir.file_name().and_then(|name| name.to_str()).unwrap_or("");
        let raw = fs::read_to_string(entry.path()).map_err(|e| e.to_string())?;
        let meta = match read_catalog_meta(&raw) {
            Ok(meta) => meta,
            Err(err) => {
                if only.is_some_and(|name| name != "*" && name != dir_name) {
                    continue;
                }
                return Err(err);
            }
        };
        if dir.file_name().and_then(|n| n.to_str()) != Some(meta.name.as_str()) {
            let requested = only.is_some_and(|name| {
                name != "*" && (name == meta.name.as_str() || name == dir_name)
            });
            if only.is_none() || only == Some("*") || requested {
                return Err(format!(
                    "skill name {} must match directory {}",
                    meta.name,
                    dir.display()
                ));
            }
            continue;
        }
        found.push(FoundSkill {
            name: meta.name.clone(),
            directory: dir.to_path_buf(),
        });
    }
    found.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(found)
}

pub fn install_tree(
    source_root: &Path,
    dest_skills: &Path,
    only: Option<&str>,
    replace: bool,
) -> Result<Vec<InstallReceipt>, String> {
    let found = discover_skills_for(source_root, only)?;
    let chosen: Vec<&FoundSkill> = match only {
        Some("*") => found.iter().collect(),
        Some(name) => {
            let skill = found
                .iter()
                .find(|s| s.name == name)
                .ok_or_else(|| format!("skill not in source: {name}"))?;
            vec![skill]
        }
        None if found.len() == 1 => found.iter().collect(),
        None => {
            let names: Vec<&str> = found.iter().map(|s| s.name.as_str()).collect();
            return Err(format!(
                "source has {} skills ({}); pass --skill NAME",
                names.len(),
                names.join(", ")
            ));
        }
    };
    let mut receipts = Vec::new();
    for skill in chosen {
        receipts.push(copy_skill(
            &skill.directory,
            dest_skills,
            &skill.name,
            replace,
        )?);
    }
    Ok(receipts)
}

pub fn content_hash(dir: &Path) -> Result<String, String> {
    let mut hasher = blake3::Hasher::new();
    let mut files = Vec::new();
    collect_files(dir, dir, &mut files, &mut 0, &mut 0)?;
    files.sort();
    for (rel, bytes) in files {
        hasher.update(rel.as_bytes());
        hasher.update(&[0]);
        hasher.update(&bytes);
    }
    Ok(hasher.finalize().to_hex().to_string())
}

fn copy_skill(
    src: &Path,
    dest_skills: &Path,
    name: &str,
    replace: bool,
) -> Result<InstallReceipt, String> {
    let dest = dest_skills.join(name);
    let hash = content_hash(src)?;
    if dest.exists() {
        let existing = content_hash(&dest)?;
        if existing == hash {
            return Ok(InstallReceipt {
                name: name.to_string(),
                hash,
                directory: dest,
            });
        }
        if !replace {
            return Err(format!(
                "skill {name} already installed with hash {existing}; pass --yes to replace with {hash}"
            ));
        }
        fs::remove_dir_all(&dest).map_err(|e| e.to_string())?;
    }
    fs::create_dir_all(&dest).map_err(|e| e.to_string())?;
    let mut files = Vec::new();
    let mut count = 0usize;
    let mut bytes = 0u64;
    collect_files(src, src, &mut files, &mut count, &mut bytes)?;
    for (rel, data) in files {
        let target = dest.join(&rel);
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        fs::write(target, data).map_err(|e| e.to_string())?;
    }
    Ok(InstallReceipt {
        name: name.to_string(),
        hash,
        directory: dest,
    })
}

fn collect_files(
    root: &Path,
    dir: &Path,
    out: &mut Vec<(String, Vec<u8>)>,
    count: &mut usize,
    bytes: &mut u64,
) -> Result<(), String> {
    for entry in fs::read_dir(dir).map_err(|e| e.to_string())? {
        let entry = entry.map_err(|e| e.to_string())?;
        let path = entry.path();
        let rel = path
            .strip_prefix(root)
            .map_err(|e| e.to_string())?
            .components()
            .map(|c| match c {
                Component::Normal(s) => Ok(s.to_string_lossy().to_string()),
                _ => Err("skill file path escapes the skill directory".to_string()),
            })
            .collect::<Result<Vec<_>, _>>()?
            .join("/");
        let meta = fs::symlink_metadata(&path).map_err(|e| e.to_string())?;
        if meta.file_type().is_symlink() {
            return Err(format!("refusing symlink in skill: {rel}"));
        }
        if meta.is_dir() {
            collect_files(root, &path, out, count, bytes)?;
            continue;
        }
        if !meta.is_file() {
            continue;
        }
        *count += 1;
        if *count > COPY_MAX_FILES {
            return Err(format!("skill has more than {COPY_MAX_FILES} files"));
        }
        *bytes += meta.len();
        if *bytes > COPY_MAX_BYTES {
            return Err(format!("skill is larger than {COPY_MAX_BYTES} bytes"));
        }
        let data = fs::read(&path).map_err(|e| e.to_string())?;
        out.push((rel, data));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn skill(dir: &Path, name: &str, body: &str) {
        let folder = dir.join(name);
        fs::create_dir_all(folder.join("scripts")).unwrap();
        fs::write(
            folder.join("SKILL.md"),
            format!("---\nname: {name}\ndescription: Use {name}.\n---\n\n{body}\n"),
        )
        .unwrap();
        fs::write(folder.join("scripts").join("run.py"), b"print(1)\n").unwrap();
    }

    #[test]
    fn copies_one_skill_and_refuses_a_different_hash() {
        let src = tempfile::tempdir().unwrap();
        let dest = tempfile::tempdir().unwrap();
        skill(src.path(), "pdf", "one");
        let receipts = install_tree(src.path(), dest.path(), None, false).unwrap();
        assert_eq!(receipts.len(), 1);
        assert!(dest.path().join("pdf/scripts/run.py").is_file());
        skill(src.path(), "pdf", "two");
        let err = install_tree(src.path(), dest.path(), Some("pdf"), false).unwrap_err();
        assert!(err.contains("--yes"));
        install_tree(src.path(), dest.path(), Some("pdf"), true).unwrap();
    }

    #[test]
    fn installs_one_skill_when_another_directory_name_does_not_match() {
        let src = tempfile::tempdir().unwrap();
        let dest = tempfile::tempdir().unwrap();
        skill(src.path(), "pdf", "read pdfs");
        let template = src.path().join("template");
        fs::create_dir_all(&template).unwrap();
        fs::write(
            template.join("SKILL.md"),
            "---\nname: template-skill\ndescription: A template.\n---\n\nIgnore.\n",
        )
        .unwrap();
        let broken = src.path().join("broken");
        fs::create_dir_all(&broken).unwrap();
        fs::write(
            broken.join("SKILL.md"),
            format!(
                "---\nname: broken\ndescription: {}\n---\n\nIgnore.\n",
                "x".repeat(1100)
            ),
        )
        .unwrap();
        let receipts = install_tree(src.path(), dest.path(), Some("pdf"), false).unwrap();
        assert_eq!(receipts.len(), 1);
        assert_eq!(receipts[0].name, "pdf");
        assert!(dest.path().join("pdf/SKILL.md").is_file());
        assert!(!dest.path().join("template-skill").exists());
        let err = install_tree(src.path(), dest.path(), Some("template-skill"), false).unwrap_err();
        assert!(err.contains("template-skill"));
        assert!(err.contains("must match directory"));
    }

    #[test]
    fn many_skills_need_a_name() {
        let src = tempfile::tempdir().unwrap();
        let dest = tempfile::tempdir().unwrap();
        skill(src.path(), "pdf", "a");
        skill(src.path(), "review", "b");
        let err = install_tree(src.path(), dest.path(), None, false).unwrap_err();
        assert!(err.contains("--skill"));
    }
}
