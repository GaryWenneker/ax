use crate::install::{content_hash, InstallReceipt, COPY_MAX_BYTES};
use std::fs;
use std::io::Read;
use std::path::Path;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WellKnownSkill {
    pub name: String,
    pub description: String,
    pub files: Vec<String>,
    pub archive_url: Option<String>,
}

pub fn parse_well_known_index(body: &str) -> Result<Vec<WellKnownSkill>, String> {
    let value: serde_json::Value =
        serde_json::from_str(body).map_err(|e| format!("well-known index: {e}"))?;
    let rows = value
        .get("skills")
        .and_then(|v| v.as_array())
        .ok_or_else(|| "well-known index has no skills array".to_string())?;
    let mut skills = Vec::new();
    for row in rows {
        let name = row
            .get("name")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        if name.is_empty() {
            continue;
        }
        let description = row
            .get("description")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        let files = row
            .get("files")
            .and_then(|v| v.as_array())
            .map(|files| {
                files
                    .iter()
                    .filter_map(|f| {
                        f.as_str()
                            .map(str::to_string)
                            .or_else(|| f.get("path").and_then(|p| p.as_str()).map(str::to_string))
                    })
                    .collect()
            })
            .unwrap_or_default();
        let archive_url = row.get("url").and_then(|v| v.as_str()).map(str::to_string);
        skills.push(WellKnownSkill {
            name,
            description,
            files,
            archive_url,
        });
    }
    Ok(skills)
}

pub fn materialize_well_known(
    index_body: &str,
    skill_name: &str,
    dest_skills: &Path,
    replace: bool,
    mut fetch: impl FnMut(&str) -> Result<Vec<u8>, String>,
) -> Result<InstallReceipt, String> {
    let skills = parse_well_known_index(index_body)?;
    let skill = skills
        .iter()
        .find(|s| s.name == skill_name)
        .ok_or_else(|| format!("well-known index has no skill {skill_name}"))?;
    let dir = dest_skills.join(&skill.name);
    if dir.exists() {
        if !replace {
            return Err(format!("skill {} already installed", skill.name));
        }
        fs::remove_dir_all(&dir).map_err(|e| e.to_string())?;
    }
    fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    if skill.files.is_empty() {
        let url = skill
            .archive_url
            .as_deref()
            .ok_or_else(|| format!("skill {} has no files", skill.name))?;
        let bytes = fetch(url)?;
        if bytes.len() as u64 > COPY_MAX_BYTES {
            return Err("well-known skill is larger than the size cap".into());
        }
        if url.ends_with(".md") {
            fs::write(dir.join("SKILL.md"), &bytes).map_err(|e| e.to_string())?;
        } else if url.ends_with(".zip") {
            extract_zip(&bytes, &dir)?;
        } else if url.ends_with(".tar.gz") || url.ends_with(".tgz") {
            extract_tar_gz(&bytes, &dir)?;
        } else {
            return Err(format!("unsupported well-known artifact: {url}"));
        }
    } else {
        for file in &skill.files {
            if file.contains("..") || file.starts_with('/') {
                return Err(format!("refusing well-known path {file}"));
            }
            let bytes = fetch(file)?;
            let target = dir.join(file);
            if let Some(parent) = target.parent() {
                fs::create_dir_all(parent).map_err(|e| e.to_string())?;
            }
            fs::write(target, bytes).map_err(|e| e.to_string())?;
        }
    }
    if !dir.join("SKILL.md").is_file() {
        return Err("well-known skill has no SKILL.md".into());
    }
    let hash = content_hash(&dir)?;
    Ok(InstallReceipt {
        name: skill.name.clone(),
        hash,
        directory: dir,
    })
}

fn extract_zip(bytes: &[u8], dest: &Path) -> Result<(), String> {
    let cursor = std::io::Cursor::new(bytes);
    let mut archive = zip::ZipArchive::new(cursor).map_err(|e| e.to_string())?;
    if archive.len() > crate::install::COPY_MAX_FILES {
        return Err("zip has too many files".into());
    }
    for i in 0..archive.len() {
        let mut file = archive.by_index(i).map_err(|e| e.to_string())?;
        let name = file
            .enclosed_name()
            .ok_or_else(|| "zip entry escapes the archive".to_string())?;
        let target = dest.join(name);
        if file.is_dir() {
            fs::create_dir_all(&target).map_err(|e| e.to_string())?;
            continue;
        }
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        let mut out = fs::File::create(&target).map_err(|e| e.to_string())?;
        std::io::copy(&mut file, &mut out).map_err(|e| e.to_string())?;
    }
    Ok(())
}

fn extract_tar_gz(bytes: &[u8], dest: &Path) -> Result<(), String> {
    let decoder = flate2::read::GzDecoder::new(bytes);
    let mut archive = tar::Archive::new(decoder);
    let mut count = 0usize;
    for entry in archive.entries().map_err(|e| e.to_string())? {
        let mut entry = entry.map_err(|e| e.to_string())?;
        let path = entry.path().map_err(|e| e.to_string())?;
        if path
            .components()
            .any(|c| matches!(c, std::path::Component::ParentDir))
        {
            return Err("tar entry escapes the archive".into());
        }
        count += 1;
        if count > crate::install::COPY_MAX_FILES {
            return Err("archive has too many files".into());
        }
        let target = dest.join(path.as_ref());
        if entry.header().entry_type().is_dir() {
            fs::create_dir_all(&target).map_err(|e| e.to_string())?;
            continue;
        }
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        let mut data = Vec::new();
        entry.read_to_end(&mut data).map_err(|e| e.to_string())?;
        fs::write(target, data).map_err(|e| e.to_string())?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn materializes_named_files() {
        let dest = tempfile::tempdir().unwrap();
        let index = r#"{"skills":[{"name":"pdf","description":"Read PDFs.","files":["SKILL.md","scripts/run.py"]}]}"#;
        let receipt = materialize_well_known(index, "pdf", dest.path(), false, |file| {
            Ok(match file {
                "SKILL.md" => b"---\nname: pdf\ndescription: Read PDFs.\n---\n".to_vec(),
                "scripts/run.py" => b"print(1)\n".to_vec(),
                other => return Err(format!("missing {other}")),
            })
        })
        .unwrap();
        assert_eq!(receipt.name, "pdf");
        assert!(receipt.directory.join("scripts/run.py").is_file());
    }
}
