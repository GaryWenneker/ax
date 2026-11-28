use crate::install::discover_skills;
use crate::validate::read_catalog_meta;
use std::fs;
use std::path::Path;

/// Copy exactly one installed skill into `dest/skills/<name>/`.
pub fn stage_one_skill(skills_root: &Path, name: &str, dest: &Path) -> Result<(), String> {
    if name.is_empty() || name == "*" || name.contains(',') || name.contains(' ') {
        return Err("publish one skill at a time".into());
    }
    let found = discover_skills(skills_root)?;
    let skill = found
        .iter()
        .find(|s| s.name == name)
        .ok_or_else(|| format!("skill not installed: {name}"))?;
    let target = dest.join("skills").join(name);
    if target.exists() {
        fs::remove_dir_all(&target).map_err(|e| e.to_string())?;
    }
    fs::create_dir_all(&target).map_err(|e| e.to_string())?;
    // Check only the staged skill: the destination may already hold skills
    // published earlier. A failed copy or check leaves no half-staged skill.
    let staged = copy_dir(&skill.directory, &target).and_then(|()| staged_name(&target));
    match staged {
        Ok(staged) if staged == name => Ok(()),
        Ok(staged) => {
            let _ = fs::remove_dir_all(&target);
            Err(format!("staged skill is named {staged}, not {name}"))
        }
        Err(err) => {
            let _ = fs::remove_dir_all(&target);
            Err(err)
        }
    }
}

fn staged_name(target: &Path) -> Result<String, String> {
    let raw = fs::read_to_string(target.join("SKILL.md")).map_err(|e| e.to_string())?;
    Ok(read_catalog_meta(&raw)?.name)
}

fn copy_dir(src: &Path, dest: &Path) -> Result<(), String> {
    for entry in fs::read_dir(src).map_err(|e| e.to_string())? {
        let entry = entry.map_err(|e| e.to_string())?;
        let path = entry.path();
        let name = entry.file_name();
        let target = dest.join(name);
        let meta = fs::symlink_metadata(&path).map_err(|e| e.to_string())?;
        if meta.file_type().is_symlink() {
            return Err("refusing to publish a symlink".into());
        }
        if meta.is_dir() {
            fs::create_dir_all(&target).map_err(|e| e.to_string())?;
            copy_dir(&path, &target)?;
        } else if meta.is_file() {
            fs::copy(&path, &target).map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn refuses_bulk_and_stages_one() {
        let root = tempfile::tempdir().unwrap();
        let dest = tempfile::tempdir().unwrap();
        for name in ["pdf", "review"] {
            let dir = root.path().join(name);
            fs::create_dir_all(&dir).unwrap();
            fs::write(
                dir.join("SKILL.md"),
                format!("---\nname: {name}\ndescription: Use {name}.\n---\n"),
            )
            .unwrap();
        }
        assert!(stage_one_skill(root.path(), "*", dest.path()).is_err());
        assert!(stage_one_skill(root.path(), "pdf,review", dest.path()).is_err());
        stage_one_skill(root.path(), "pdf", dest.path()).unwrap();
        assert!(dest.path().join("skills/pdf/SKILL.md").is_file());
        assert!(!dest.path().join("skills/review").exists());
    }

    #[test]
    fn stages_next_to_skills_published_earlier() {
        let root = tempfile::tempdir().unwrap();
        let dest = tempfile::tempdir().unwrap();
        for name in ["pdf", "review"] {
            let dir = root.path().join(name);
            fs::create_dir_all(&dir).unwrap();
            fs::write(
                dir.join("SKILL.md"),
                format!("---\nname: {name}\ndescription: Use {name}.\n---\n"),
            )
            .unwrap();
        }
        stage_one_skill(root.path(), "pdf", dest.path()).unwrap();
        stage_one_skill(root.path(), "review", dest.path()).unwrap();
        assert!(dest.path().join("skills/pdf/SKILL.md").is_file());
        assert!(dest.path().join("skills/review/SKILL.md").is_file());
    }
}
