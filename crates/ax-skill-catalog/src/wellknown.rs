#[cfg(test)]
use crate::install::content_hash;
use crate::install::{install_staged, InstallReceipt, COPY_MAX_BYTES};
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
    crate::validate::validate_skill_name(&skill.name)?;
    if dest_skills.join(&skill.name).exists() && !replace {
        return Err(format!("skill {} already installed", skill.name));
    }
    install_staged(dest_skills, &skill.name, |dir| {
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
                extract_zip(&bytes, dir)?;
            } else if url.ends_with(".tar.gz") || url.ends_with(".tgz") {
                extract_tar_gz(&bytes, dir)?;
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
        Ok(())
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
        // Only plain names (and `.`) may appear: an absolute path, a drive
        // prefix or `..` would make `dest.join` land outside `dest`.
        let mut rel = std::path::PathBuf::new();
        for component in path.components() {
            match component {
                std::path::Component::Normal(part) => rel.push(part),
                std::path::Component::CurDir => {}
                _ => return Err("tar entry escapes the archive".into()),
            }
        }
        count += 1;
        if count > crate::install::COPY_MAX_FILES {
            return Err("archive has too many files".into());
        }
        if rel.as_os_str().is_empty() {
            continue;
        }
        let target = dest.join(&rel);
        let kind = entry.header().entry_type();
        if kind.is_dir() {
            fs::create_dir_all(&target).map_err(|e| e.to_string())?;
            continue;
        }
        // Links, devices and other special entries are not skill content.
        if !kind.is_file() {
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
    const PDF_V1: &[u8] = b"---\nname: pdf\ndescription: Read PDFs.\n---\n\nv1\n";
    const PDF_INDEX: &str = r#"{"skills":[{"name":"pdf","description":"Read PDFs.","files":["SKILL.md","scripts/run.py"]}]}"#;

    fn install_v1(dest: &Path) {
        materialize_well_known(PDF_INDEX, "pdf", dest, false, |file| {
            Ok(match file {
                "SKILL.md" => PDF_V1.to_vec(),
                _ => b"print(1)\n".to_vec(),
            })
        })
        .unwrap();
    }

    fn leftovers(dest: &Path) -> Vec<String> {
        std::fs::read_dir(dest)
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .filter(|name| name != "pdf")
            .collect()
    }

    #[test]
    fn a_failed_replace_keeps_the_installed_skill() {
        let dest = tempfile::tempdir().unwrap();
        install_v1(dest.path());
        let err = materialize_well_known(PDF_INDEX, "pdf", dest.path(), true, |file| match file {
            "SKILL.md" => Ok(b"---\nname: pdf\ndescription: Read PDFs.\n---\n\nv2\n".to_vec()),
            _ => Err("500 Internal Server Error".into()),
        })
        .unwrap_err();
        assert!(err.contains("500"), "{err}");
        let kept = std::fs::read(dest.path().join("pdf/SKILL.md")).unwrap();
        assert_eq!(kept, PDF_V1);
        assert!(dest.path().join("pdf/scripts/run.py").is_file());
        assert!(
            leftovers(dest.path()).is_empty(),
            "{:?}",
            leftovers(dest.path())
        );
    }

    #[test]
    fn a_skill_whose_frontmatter_names_another_skill_is_refused() {
        let dest = tempfile::tempdir().unwrap();
        install_v1(dest.path());
        let err = materialize_well_known(PDF_INDEX, "pdf", dest.path(), true, |file| {
            Ok(match file {
                "SKILL.md" => b"---\nname: other\ndescription: Not pdf.\n---\n".to_vec(),
                _ => b"print(2)\n".to_vec(),
            })
        })
        .unwrap_err();
        assert!(err.contains("other"), "{err}");
        assert_eq!(
            std::fs::read(dest.path().join("pdf/SKILL.md")).unwrap(),
            PDF_V1
        );
        assert!(leftovers(dest.path()).is_empty());
    }

    #[test]
    fn a_failed_first_install_leaves_nothing_behind() {
        let dest = tempfile::tempdir().unwrap();
        let err = materialize_well_known(PDF_INDEX, "pdf", dest.path(), false, |file| match file {
            "SKILL.md" => Ok(PDF_V1.to_vec()),
            _ => Err("timeout".into()),
        })
        .unwrap_err();
        assert!(err.contains("timeout"));
        assert!(!dest.path().join("pdf").exists());
        assert!(leftovers(dest.path()).is_empty());
        // A retry is not blocked by a half-written directory.
        install_v1(dest.path());
    }

    #[test]
    fn an_index_name_that_is_not_a_skill_name_is_refused() {
        let root = tempfile::tempdir().unwrap();
        let dest = root.path().join("skills");
        std::fs::create_dir_all(&dest).unwrap();
        let index = r#"{"skills":[{"name":"../escape","description":"d","files":["SKILL.md"]}]}"#;
        let err = materialize_well_known(index, "../escape", &dest, false, |_| Ok(PDF_V1.to_vec()))
            .unwrap_err();
        assert!(err.contains("skill name"), "{err}");
        assert!(!root.path().join("escape").exists());
    }

    #[test]
    fn a_replace_swaps_in_the_new_version() {
        let dest = tempfile::tempdir().unwrap();
        install_v1(dest.path());
        let v2 = b"---\nname: pdf\ndescription: Read PDFs.\n---\n\nv2\n";
        let receipt = materialize_well_known(PDF_INDEX, "pdf", dest.path(), true, |file| {
            Ok(match file {
                "SKILL.md" => v2.to_vec(),
                _ => b"print(2)\n".to_vec(),
            })
        })
        .unwrap();
        assert_eq!(receipt.directory, dest.path().join("pdf"));
        assert_eq!(std::fs::read(dest.path().join("pdf/SKILL.md")).unwrap(), v2);
        assert_eq!(receipt.hash, content_hash(&receipt.directory).unwrap());
        assert!(leftovers(dest.path()).is_empty());
    }

    /// A gzip tar with raw member names, as a hostile archive would carry them.
    fn raw_tar_gz(entries: &[(&str, tar::EntryType, &[u8])]) -> Vec<u8> {
        let mut builder = tar::Builder::new(Vec::new());
        for (name, kind, data) in entries {
            let mut header = tar::Header::new_old();
            {
                let old = header.as_old_mut();
                old.name[..name.len()].copy_from_slice(name.as_bytes());
            }
            header.set_entry_type(*kind);
            header.set_size(data.len() as u64);
            header.set_mode(0o644);
            header.set_cksum();
            builder.append(&header, *data).unwrap();
        }
        let tar = builder.into_inner().unwrap();
        let mut gz = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
        std::io::Write::write_all(&mut gz, &tar).unwrap();
        gz.finish().unwrap()
    }

    fn install_tar(bytes: Vec<u8>) -> (tempfile::TempDir, Result<InstallReceipt, String>) {
        let dest = tempfile::tempdir().unwrap();
        let index =
            r#"{"skills":[{"name":"pdf","description":"d","url":"https://host/pdf.tar.gz"}]}"#;
        let result =
            materialize_well_known(index, "pdf", dest.path(), false, |_| Ok(bytes.clone()));
        (dest, result)
    }

    #[test]
    fn a_tar_entry_with_an_absolute_path_is_refused() {
        let outside = tempfile::tempdir().unwrap();
        let victim = outside.path().join("victim.txt");
        std::fs::write(&victim, "keep me").unwrap();
        let name = victim.display().to_string();
        let bytes = raw_tar_gz(&[
            (
                "SKILL.md",
                tar::EntryType::Regular,
                b"---\nname: pdf\ndescription: Read PDFs.\n---\n",
            ),
            (name.as_str(), tar::EntryType::Regular, b"pwned"),
        ]);
        let (_dest, result) = install_tar(bytes);
        assert_eq!(std::fs::read_to_string(&victim).unwrap(), "keep me");
        assert!(result.is_err());
    }

    #[test]
    fn a_tar_entry_with_a_parent_dir_is_refused() {
        let bytes = raw_tar_gz(&[("../escape.txt", tar::EntryType::Regular, b"x")]);
        let (dest, result) = install_tar(bytes);
        assert!(result.is_err());
        assert!(!dest.path().join("escape.txt").exists());
    }

    #[test]
    fn a_tar_link_entry_is_skipped() {
        let bytes = raw_tar_gz(&[
            (
                "SKILL.md",
                tar::EntryType::Regular,
                b"---\nname: pdf\ndescription: Read PDFs.\n---\n",
            ),
            ("link", tar::EntryType::Symlink, b""),
        ]);
        let (dest, result) = install_tar(bytes);
        let receipt = result.unwrap();
        assert!(receipt.directory.join("SKILL.md").is_file());
        assert!(std::fs::symlink_metadata(dest.path().join("pdf/link")).is_err());
    }

    #[test]
    fn a_plain_tar_installs() {
        let bytes = raw_tar_gz(&[
            (
                "./SKILL.md",
                tar::EntryType::Regular,
                b"---\nname: pdf\ndescription: Read PDFs.\n---\n",
            ),
            ("scripts/run.py", tar::EntryType::Regular, b"print(1)\n"),
        ]);
        let (_dest, result) = install_tar(bytes);
        let receipt = result.unwrap();
        assert!(receipt.directory.join("scripts/run.py").is_file());
    }
}
