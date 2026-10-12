//! Source attribution for exact pending policy content, separate from policy semantics.

use ax_utils::errors::AxError;
use std::{io::Write, path::Path};

#[derive(Debug, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct PendingOrigin {
    content_hash: String,
    origin: String,
}

/// Called by blocking pending-file writers. Never attribute an edited file.
pub fn record_pending_origin(path: &Path, incoming: &str, origin: &str) -> Result<(), AxError> {
    let fail = |e: std::io::Error| AxError::Other(e.to_string());
    if std::fs::symlink_metadata(path)
        .map_err(fail)?
        .file_type()
        .is_symlink()
        || std::fs::read_to_string(path).map_err(fail)? != incoming
    {
        return Err(AxError::Other(
            "pending content changed before recording its source".into(),
        ));
    }
    let parent = path
        .parent()
        .ok_or_else(|| AxError::Other("invalid pending path".into()))?;
    let value = PendingOrigin {
        content_hash: crate::revisions::content_hash(incoming),
        origin: origin.chars().take(1024).collect(),
    };
    let raw = serde_json::to_vec(&value).map_err(|e| AxError::Other(e.to_string()))?;
    let mut temp = tempfile::NamedTempFile::new_in(parent).map_err(fail)?;
    temp.write_all(&raw).map_err(fail)?;
    temp.as_file().sync_all().map_err(fail)?;
    temp.persist(path.with_extension("origin.json"))
        .map_err(|e| fail(e.error))?;
    Ok(())
}

/// Missing, corrupt or stale optional metadata means unknown, never a guess.
pub async fn pending_origin(path: &Path, incoming: &str) -> Option<String> {
    use tokio::io::AsyncReadExt;
    let source = path.with_extension("origin.json");
    let meta = tokio::fs::symlink_metadata(&source).await.ok()?;
    if !meta.is_file() || meta.file_type().is_symlink() || meta.len() > 8192 {
        return None;
    }
    let file = tokio::fs::File::open(source).await.ok()?;
    let mut raw = Vec::new();
    file.take(8193).read_to_end(&mut raw).await.ok()?;
    if raw.len() > 8192 {
        return None;
    }
    let value: PendingOrigin = serde_json::from_slice(&raw).ok()?;
    (value.content_hash == crate::revisions::content_hash(incoming)
        && !value.origin.trim().is_empty())
    .then_some(value.origin)
}

/// Remove only this pending item's optional metadata.
pub async fn remove_pending_origin(path: &Path) -> Result<(), AxError> {
    match tokio::fs::remove_file(path.with_extension("origin.json")).await {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(AxError::Other(e.to_string())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn origin_survives_reopen_and_only_matches_exact_content() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("same.mdc");
        std::fs::write(&path, "incoming").unwrap();
        record_pending_origin(&path, "incoming", "Replica office-mac").unwrap();
        assert_eq!(
            pending_origin(&path, "incoming").await.as_deref(),
            Some("Replica office-mac")
        );
        assert_eq!(pending_origin(&path, "edited").await, None);
        std::fs::write(path.with_extension("origin.json"), "broken").unwrap();
        assert_eq!(pending_origin(&path, "incoming").await, None);
    }

    #[tokio::test]
    async fn equal_rule_and_skill_names_keep_separate_sources() {
        let root = tempfile::tempdir().unwrap();
        let rule = root.path().join("same.mdc");
        let skill = root.path().join("same/SKILL.md");
        std::fs::create_dir_all(skill.parent().unwrap()).unwrap();
        for (path, source) in [(&rule, "Pack main"), (&skill, "Replica laptop")] {
            std::fs::write(path, "incoming").unwrap();
            record_pending_origin(path, "incoming", source).unwrap();
        }
        assert_eq!(
            pending_origin(&rule, "incoming").await.as_deref(),
            Some("Pack main")
        );
        assert_eq!(
            pending_origin(&skill, "incoming").await.as_deref(),
            Some("Replica laptop")
        );
    }

    #[test]
    fn changed_pending_file_cannot_be_attributed_to_old_content() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("sample.mdc");
        std::fs::write(&path, "edited").unwrap();
        assert!(record_pending_origin(&path, "original", "Wrong source").is_err());
    }

    #[tokio::test]
    async fn replacement_is_atomic_and_cleanup_is_idempotent() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("sample.mdc");
        std::fs::write(&path, "incoming").unwrap();
        record_pending_origin(&path, "incoming", "First").unwrap();
        record_pending_origin(&path, "incoming", "Second").unwrap();
        assert_eq!(
            pending_origin(&path, "incoming").await.as_deref(),
            Some("Second")
        );
        remove_pending_origin(&path).await.unwrap();
        remove_pending_origin(&path).await.unwrap();
        assert!(path.exists());
        assert_eq!(pending_origin(&path, "incoming").await, None);
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn symlinked_metadata_and_oversized_metadata_are_unknown() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("sample.mdc");
        let target = root.path().join("target");
        std::fs::write(&path, "incoming").unwrap();
        record_pending_origin(&path, "incoming", "Source").unwrap();
        std::fs::rename(path.with_extension("origin.json"), &target).unwrap();
        std::os::unix::fs::symlink(&target, path.with_extension("origin.json")).unwrap();
        assert_eq!(pending_origin(&path, "incoming").await, None);
        std::fs::remove_file(path.with_extension("origin.json")).unwrap();
        std::fs::write(path.with_extension("origin.json"), "x".repeat(8193)).unwrap();
        assert_eq!(pending_origin(&path, "incoming").await, None);
    }
}
