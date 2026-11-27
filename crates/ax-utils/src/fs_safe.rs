//! File-system helpers that keep writes inside a root, never leave a
//! half-written file behind and never read more than a caller allows.

use std::fs::{self, File, OpenOptions};
use std::io::{self, Read, Write};
use std::path::{Component, Path, PathBuf};

use fs2::FileExt;

/// Why a relative path was refused.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum PathEscape {
    #[error("empty path")]
    Empty,
    #[error("absolute path is not allowed: {0}")]
    Absolute(String),
    #[error("path leaves its folder: {0}")]
    ParentDir(String),
    #[error("path points outside its folder through a link: {0}")]
    Symlink(String),
    #[error("cannot check path {0}: {1}")]
    Io(String, String),
}

/// Checks that `rel` is a plain relative path: not empty, not absolute, no
/// drive or UNC prefix, no `..`. Backslashes count as separators, so
/// `..\\x` and `C:\\x` are refused on every platform. Use this for names that
/// come from archives, requests or frontmatter before joining them.
pub fn safe_relative(rel: &str) -> Result<PathBuf, PathEscape> {
    let normalized = rel.replace('\\', "/");
    if normalized.trim().is_empty() {
        return Err(PathEscape::Empty);
    }
    if normalized.starts_with('/')
        || normalized.as_bytes().get(1) == Some(&b':')
        || Path::new(&normalized).is_absolute()
    {
        return Err(PathEscape::Absolute(rel.to_string()));
    }
    let mut out = PathBuf::new();
    for part in normalized.split('/') {
        match part {
            "" | "." => continue,
            ".." => return Err(PathEscape::ParentDir(rel.to_string())),
            p => out.push(p),
        }
    }
    if out.as_os_str().is_empty() {
        return Err(PathEscape::Empty);
    }
    Ok(out)
}

/// Joins `rel` onto `root` and refuses any result outside `root`: absolute
/// paths, `..`, and existing symbolic links that point out of `root`.
/// The target itself does not need to exist.
pub fn contained_join(root: &Path, rel: &str) -> Result<PathBuf, PathEscape> {
    let rel = safe_relative(rel)?;
    let joined = root.join(&rel);
    ensure_within(root, &joined)?;
    Ok(joined)
}

/// Refuses `path` when it, or its deepest existing ancestor, resolves outside
/// `root` (following symbolic links). `path` must already be lexically under
/// `root`.
pub fn ensure_within(root: &Path, path: &Path) -> Result<(), PathEscape> {
    if path
        .components()
        .any(|c| matches!(c, Component::ParentDir))
    {
        return Err(PathEscape::ParentDir(path.display().to_string()));
    }
    if !path.starts_with(root) {
        return Err(PathEscape::Absolute(path.display().to_string()));
    }
    let real_root = match root.canonicalize() {
        Ok(r) => r,
        // A root that does not exist yet has no links to escape through.
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(()),
        Err(e) => return Err(PathEscape::Io(root.display().to_string(), e.to_string())),
    };
    let mut probe = path.to_path_buf();
    loop {
        match probe.canonicalize() {
            Ok(real) => {
                return if real.starts_with(&real_root) {
                    Ok(())
                } else {
                    Err(PathEscape::Symlink(path.display().to_string()))
                };
            }
            Err(e) if e.kind() == io::ErrorKind::NotFound => {
                // A dangling link still points somewhere; refuse it.
                if fs::symlink_metadata(&probe).is_ok() {
                    return Err(PathEscape::Symlink(path.display().to_string()));
                }
                if !probe.pop() {
                    return Ok(());
                }
            }
            Err(e) => return Err(PathEscape::Io(path.display().to_string(), e.to_string())),
        }
    }
}

/// Writes `bytes` to `path` so that readers see either the old or the new
/// content, never a partial file: write a temporary file next to it, flush it
/// to disk, then rename it over `path`. Keeps the old file's permissions.
pub fn atomic_write(path: &Path, bytes: &[u8]) -> io::Result<()> {
    let dir = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    fs::create_dir_all(dir)?;
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| "file".into());
    // Unique per process and per call, so threads writing the same file at
    // once do not share a temporary file.
    static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let seq = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let tmp = dir.join(format!(".{name}.{}.{seq}.tmp", std::process::id()));
    let result = (|| {
        let mut f = File::create(&tmp)?;
        f.write_all(bytes)?;
        f.sync_all()?;
        if let Ok(meta) = fs::metadata(path) {
            fs::set_permissions(&tmp, meta.permissions())?;
        }
        fs::rename(&tmp, path)
    })();
    if result.is_err() {
        let _ = fs::remove_file(&tmp);
    }
    result
}

/// Read-modify-write of a shared file under an exclusive advisory lock on
/// `<path>.lock`, so parallel processes (hooks, CLI calls) do not lose each
/// other's updates. `update` receives the current content (`None` when the
/// file does not exist) and returns the new content, or `None` to leave the
/// file unchanged. The write is atomic.
pub fn update_locked<F>(path: &Path, update: F) -> io::Result<bool>
where
    F: FnOnce(Option<Vec<u8>>) -> io::Result<Option<Vec<u8>>>,
{
    if let Some(dir) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
        fs::create_dir_all(dir)?;
    }
    let mut lock_name = path.as_os_str().to_owned();
    lock_name.push(".lock");
    let lock = OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(PathBuf::from(lock_name))?;
    lock.lock_exclusive()?;
    let current = match fs::read(path) {
        Ok(b) => Some(b),
        Err(e) if e.kind() == io::ErrorKind::NotFound => None,
        Err(e) => {
            let _ = FileExt::unlock(&lock);
            return Err(e);
        }
    };
    let outcome = update(current).and_then(|next| match next {
        Some(bytes) => atomic_write(path, &bytes).map(|_| true),
        None => Ok(false),
    });
    let _ = FileExt::unlock(&lock);
    outcome
}

/// Reads at most `max_bytes` from `path`; a larger file is an error
/// (`InvalidData`) instead of an unbounded allocation.
pub fn read_bounded(path: &Path, max_bytes: u64) -> io::Result<Vec<u8>> {
    let file = File::open(path)?;
    let mut buf = Vec::new();
    file.take(max_bytes.saturating_add(1)).read_to_end(&mut buf)?;
    if buf.len() as u64 > max_bytes {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("{} is larger than {max_bytes} bytes", path.display()),
        ));
    }
    Ok(buf)
}

/// Reads at most `max_bytes` from any reader (archive entries, request
/// bodies); more is an error.
pub fn read_to_end_bounded<R: Read>(reader: R, max_bytes: u64) -> io::Result<Vec<u8>> {
    let mut buf = Vec::new();
    reader.take(max_bytes.saturating_add(1)).read_to_end(&mut buf)?;
    if buf.len() as u64 > max_bytes {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("content is larger than {max_bytes} bytes"),
        ));
    }
    Ok(buf)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn safe_relative_refuses_every_escape_form() {
        for bad in [
            "", "  ", ".", "/etc/passwd", "//etc/passwd", "\\\\server\\share", "C:\\x", "c:/x",
            "..", "../x", "a/../../x", "a\\..\\x", "a/./../b",
        ] {
            assert!(safe_relative(bad).is_err(), "{bad:?} must be refused");
        }
        assert_eq!(safe_relative("a/b.md").unwrap(), PathBuf::from("a/b.md"));
        assert_eq!(safe_relative("./a//b").unwrap(), PathBuf::from("a/b"));
        assert_eq!(safe_relative("a\\b").unwrap(), PathBuf::from("a/b"));
        assert_eq!(safe_relative("x..y/z").unwrap(), PathBuf::from("x..y/z"));
    }

    #[test]
    fn contained_join_stays_inside_the_root() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        assert_eq!(contained_join(root, "rules/a.md").unwrap(), root.join("rules/a.md"));
        assert!(contained_join(root, "../a.md").is_err());
        assert!(contained_join(root, "/tmp/a.md").is_err());
    }

    #[cfg(unix)]
    #[test]
    fn contained_join_refuses_links_that_leave_the_root() {
        let dir = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        let root = dir.path().join("root");
        fs::create_dir_all(root.join("inner")).unwrap();
        std::os::unix::fs::symlink(outside.path(), root.join("out")).unwrap();
        std::os::unix::fs::symlink(root.join("inner"), root.join("in")).unwrap();
        std::os::unix::fs::symlink(outside.path().join("missing"), root.join("dangling")).unwrap();
        assert!(matches!(contained_join(&root, "out/x.md"), Err(PathEscape::Symlink(_))));
        assert!(matches!(contained_join(&root, "out"), Err(PathEscape::Symlink(_))));
        assert!(matches!(contained_join(&root, "dangling"), Err(PathEscape::Symlink(_))));
        assert!(contained_join(&root, "in/x.md").is_ok());
        assert!(contained_join(&root, "new/deeper/x.md").is_ok());
    }

    #[test]
    fn atomic_write_replaces_content_and_leaves_no_temp_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("sub/f.json");
        atomic_write(&path, b"one").unwrap();
        atomic_write(&path, b"two").unwrap();
        assert_eq!(fs::read(&path).unwrap(), b"two");
        let names: Vec<_> = fs::read_dir(path.parent().unwrap())
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        assert_eq!(names, vec!["f.json".to_string()]);
    }

    #[test]
    fn concurrent_atomic_writes_always_leave_one_complete_version() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("f");
        let threads: Vec<_> = (0..8u8)
            .map(|i| {
                let path = path.clone();
                std::thread::spawn(move || {
                    for _ in 0..50 {
                        atomic_write(&path, &vec![b'a' + i; 4096]).unwrap();
                    }
                })
            })
            .collect();
        for t in threads {
            t.join().unwrap();
        }
        let data = fs::read(&path).unwrap();
        assert_eq!(data.len(), 4096);
        assert!(data.iter().all(|b| *b == data[0]), "mixed content");
        assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 1, "temp files left");
    }

    #[cfg(unix)]
    #[test]
    fn atomic_write_keeps_the_file_mode() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("secret.json");
        fs::write(&path, b"{}").unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).unwrap();
        atomic_write(&path, b"{\"a\":1}").unwrap();
        assert_eq!(fs::metadata(&path).unwrap().permissions().mode() & 0o777, 0o600);
    }

    #[test]
    fn update_locked_does_not_lose_concurrent_updates() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("counter");
        let threads: Vec<_> = (0..8)
            .map(|_| {
                let path = path.clone();
                std::thread::spawn(move || {
                    for _ in 0..25 {
                        update_locked(&path, |cur| {
                            let n: u32 = cur
                                .map(|b| String::from_utf8(b).unwrap().parse().unwrap())
                                .unwrap_or(0);
                            Ok(Some((n + 1).to_string().into_bytes()))
                        })
                        .unwrap();
                    }
                })
            })
            .collect();
        for t in threads {
            t.join().unwrap();
        }
        assert_eq!(fs::read_to_string(&path).unwrap(), "200");
    }

    #[test]
    fn update_locked_can_leave_the_file_alone() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("f");
        assert!(!update_locked(&path, |_| Ok(None)).unwrap());
        assert!(!path.exists());
    }

    #[test]
    fn bounded_reads_refuse_oversized_input() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("f");
        fs::write(&path, vec![b'x'; 10]).unwrap();
        assert_eq!(read_bounded(&path, 10).unwrap().len(), 10);
        assert_eq!(
            read_bounded(&path, 9).unwrap_err().kind(),
            io::ErrorKind::InvalidData
        );
        assert_eq!(read_to_end_bounded(&b"abc"[..], 3).unwrap(), b"abc");
        assert!(read_to_end_bounded(&b"abcd"[..], 3).is_err());
    }
}
