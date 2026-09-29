//! Disk access for `folders/<name>/...`. Every path is resolved inside the folder root;
//! `..`, absolute parts, and symlinks that leave the root are refused.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

use dav_server::fs::{FsError, FsResult};

fn io_err(e: std::io::Error) -> FsError {
    match e.kind() {
        std::io::ErrorKind::NotFound => FsError::NotFound,
        std::io::ErrorKind::AlreadyExists => FsError::Exists,
        std::io::ErrorKind::PermissionDenied => FsError::Forbidden,
        _ => FsError::GeneralFailure,
    }
}

/// Run a disk operation off the async runtime.
pub async fn blocking<T, F>(op: F) -> FsResult<T>
where
    T: Send + 'static,
    F: FnOnce() -> FsResult<T> + Send + 'static,
{
    tokio::task::spawn_blocking(op)
        .await
        .map_err(|_| FsError::GeneralFailure)?
}

fn modified_ms(meta: &std::fs::Metadata) -> i64 {
    meta.modified()
        .ok()
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

/// `root` joined with `rest`, refused unless the result (after following symlinks)
/// stays inside `root`. A path that does not exist yet is checked through its parent.
pub fn resolve(root: &Path, rest: &str) -> FsResult<PathBuf> {
    let base = root.canonicalize().map_err(|_| FsError::NotFound)?;
    let mut path = base.clone();
    for part in rest.split('/').filter(|p| !p.is_empty()) {
        if part == "." || part == ".." || part.contains('\\') || Path::new(part).is_absolute() {
            return Err(FsError::Forbidden);
        }
        path.push(part);
    }
    if path == base {
        return Ok(path);
    }
    let checked = if path.symlink_metadata().is_ok() {
        path.canonicalize().map_err(|_| FsError::Forbidden)?
    } else {
        let parent = path.parent().ok_or(FsError::Forbidden)?;
        parent.canonicalize().map_err(|_| FsError::NotFound)?
    };
    if checked.starts_with(&base) {
        Ok(path)
    } else {
        Err(FsError::Forbidden)
    }
}

pub enum Found {
    Dir(i64),
    File(Vec<u8>, i64),
}

pub fn lookup(root: &Path, rest: &str) -> FsResult<Found> {
    let path = resolve(root, rest)?;
    let meta = std::fs::metadata(&path).map_err(io_err)?;
    if meta.is_dir() {
        Ok(Found::Dir(modified_ms(&meta)))
    } else {
        let bytes = std::fs::read(&path).map_err(io_err)?;
        Ok(Found::File(bytes, modified_ms(&meta)))
    }
}

/// Children as `name -> (is_dir, len, modified_ms)`. Symlinks are not listed.
pub fn children(root: &Path, rest: &str) -> FsResult<BTreeMap<String, (bool, u64, i64)>> {
    let path = resolve(root, rest)?;
    let mut out = BTreeMap::new();
    for entry in std::fs::read_dir(&path).map_err(io_err)?.flatten() {
        let Ok(meta) = entry.path().symlink_metadata() else {
            continue;
        };
        if meta.file_type().is_symlink() {
            continue;
        }
        let name = entry.file_name().to_string_lossy().into_owned();
        out.insert(name, (meta.is_dir(), meta.len(), modified_ms(&meta)));
    }
    Ok(out)
}

pub fn write(root: &Path, rest: &str, bytes: &[u8]) -> FsResult<()> {
    let path = resolve(root, rest)?;
    if path.is_dir() {
        return Err(FsError::Forbidden);
    }
    std::fs::write(path, bytes).map_err(io_err)
}

pub fn mkdir(root: &Path, rest: &str) -> FsResult<()> {
    std::fs::create_dir(resolve(root, rest)?).map_err(io_err)
}

pub fn remove(root: &Path, rest: &str) -> FsResult<()> {
    let path = resolve(root, rest)?;
    let meta = path.symlink_metadata().map_err(io_err)?;
    if meta.is_dir() {
        std::fs::remove_dir_all(path).map_err(io_err)
    } else {
        std::fs::remove_file(path).map_err(io_err)
    }
}

pub fn rename(root: &Path, from: &str, to: &str) -> FsResult<()> {
    let from = resolve(root, from)?;
    let to = resolve(root, to)?;
    std::fs::rename(from, to).map_err(io_err)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dot_dot_is_refused_even_when_it_stays_inside() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir(dir.path().join("sub")).unwrap();
        std::fs::write(dir.path().join("a.md"), "a").unwrap();
        assert!(matches!(
            resolve(dir.path(), "sub/../a.md"),
            Err(FsError::Forbidden)
        ));
        assert!(matches!(
            resolve(dir.path(), "../x"),
            Err(FsError::Forbidden)
        ));
        assert!(resolve(dir.path(), "sub/a.md").is_ok());
    }
}
