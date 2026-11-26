//! Cross-process file lock using ax.lock (PID stamped).
//!
//! Index/sync writers wait for the lock instead of failing immediately, so a
//! concurrent `ax sync`, MCP catch-up, or sync --watch does not surface as
//! `database is locked` / LockUnavailable under normal contention.
//!
//! The lock is an advisory `flock` on one stable file. The file is never
//! deleted: unlinking it while another process holds the lock lets a third
//! process create a new inode and lock that one, so two writers would run at
//! once. The kernel drops the lock when its holder exits (also after
//! `kill -9`), so a leftover file never blocks anyone. The PID inside is only
//! for messages and `ax unlock`.

use std::fs::{File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use fs2::FileExt;

use crate::errors::AxError;
use crate::process::is_pid_alive;

/// How long writers wait for `.ax/ax.lock` before giving up.
pub const DEFAULT_LOCK_WAIT: Duration = Duration::from_secs(180);

pub struct FileLock {
    path: PathBuf,
    file: Option<File>,
}

impl FileLock {
    pub fn new(ax_dir: &Path) -> Self {
        Self {
            path: ax_dir.join("ax.lock"),
            file: None,
        }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Kept for API compatibility; a leftover lock file needs no cleanup.
    pub fn prepare(&self) {}

    /// Wait up to [`DEFAULT_LOCK_WAIT`] for an exclusive lock.
    pub fn acquire(&mut self) -> Result<(), AxError> {
        self.acquire_wait(DEFAULT_LOCK_WAIT)
    }

    /// Wait up to `timeout` for an exclusive lock.
    pub fn acquire_wait(&mut self, timeout: Duration) -> Result<(), AxError> {
        if self.file.is_some() {
            return Ok(());
        }
        let deadline = Instant::now() + timeout;
        let mut attempt = 0u32;
        loop {
            match self.try_acquire_once() {
                Ok(()) => return Ok(()),
                Err(AxError::LockUnavailable(_)) => {
                    if Instant::now() >= deadline {
                        return Err(self.lock_unavailable_error());
                    }
                    let sleep_ms = 50u64.saturating_mul((attempt + 1).min(20) as u64);
                    std::thread::sleep(Duration::from_millis(sleep_ms));
                    attempt = attempt.saturating_add(1);
                }
                Err(e) => return Err(e),
            }
        }
    }

    fn try_acquire_once(&mut self) -> Result<(), AxError> {
        if self.file.is_some() {
            return Ok(());
        }

        if let Some(parent) = self.path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| {
                AxError::File(crate::errors::FileError::with_path(
                    e.to_string(),
                    parent.display().to_string(),
                ))
            })?;
        }

        let file = OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(&self.path)
            .map_err(|e| {
                AxError::File(crate::errors::FileError::with_path(
                    e.to_string(),
                    self.path.display().to_string(),
                ))
            })?;

        match file.try_lock_exclusive() {
            Ok(()) => {
                // An older ax may have unlinked the path after we opened it. A
                // lock on an unlinked inode excludes nobody, so try again.
                if !same_file(&file, &self.path) {
                    let _ = file.unlock();
                    return Err(self.lock_unavailable_error());
                }
                stamp_pid(&file)?;
                self.file = Some(file);
                Ok(())
            }
            Err(_) => Err(self.lock_unavailable_error()),
        }
    }

    fn lock_unavailable_error(&self) -> AxError {
        let holder = read_lock_pid(&self.path);
        let hint = match holder {
            Some(pid) if is_pid_alive(pid) => {
                format!(
                    "lock unavailable: {} (held by PID {pid} — wait for that sync to finish, or run `ax unlock` / `ax daemon restart`)",
                    self.path.display()
                )
            }
            Some(pid) => {
                format!(
                    "lock unavailable: {} (stale PID {pid} — run `ax unlock`)",
                    self.path.display()
                )
            }
            None => format!(
                "lock unavailable: {} (run `ax unlock` or `ax daemon restart`)",
                self.path.display()
            ),
        };
        AxError::LockUnavailable(hint)
    }

    /// Unlock if this instance holds the lock. Never touches a lock it does
    /// not hold, and never deletes the file.
    pub fn release(&mut self) -> Result<(), AxError> {
        if let Some(file) = self.file.take() {
            // Clear the PID first so `ax unlock` and messages do not name us.
            let _ = file.set_len(0);
            file.unlock().map_err(|e| {
                AxError::File(crate::errors::FileError::with_path(
                    e.to_string(),
                    self.path.display().to_string(),
                ))
            })?;
        }
        Ok(())
    }

    pub fn is_held(&self) -> bool {
        self.file.is_some()
    }
}

impl Drop for FileLock {
    fn drop(&mut self) {
        let _ = self.release();
    }
}

/// Clear a leftover PID from a lock file nobody holds. Returns true when the
/// file existed and was free.
///
/// Whether the lock is held is decided by `flock` itself, not by the PID: a
/// PID from another container's PID namespace, or a missing `kill` binary,
/// must never make a live lock look stale. The file is never deleted.
pub fn clear_stale_lock(lock_path: &Path) -> bool {
    let Ok(file) = OpenOptions::new().read(true).write(true).open(lock_path) else {
        return false;
    };
    if file.try_lock_exclusive().is_err() {
        return false;
    }
    let _ = file.set_len(0);
    let _ = file.unlock();
    true
}

#[cfg(unix)]
fn same_file(file: &File, path: &Path) -> bool {
    use std::os::unix::fs::MetadataExt;
    match (file.metadata(), std::fs::metadata(path)) {
        (Ok(a), Ok(b)) => a.dev() == b.dev() && a.ino() == b.ino(),
        _ => false,
    }
}

#[cfg(not(unix))]
fn same_file(_file: &File, path: &Path) -> bool {
    path.exists()
}

fn read_lock_pid(path: &Path) -> Option<u32> {
    let raw = std::fs::read_to_string(path).ok()?;
    raw.trim().parse().ok()
}

fn stamp_pid(file: &File) -> Result<(), AxError> {
    let pid = std::process::id();
    file.set_len(0).map_err(|e| AxError::Other(e.to_string()))?;
    let mut f = file;
    write!(f, "{pid}").map_err(|e| AxError::Other(e.to_string()))?;
    f.sync_all().map_err(|e| AxError::Other(e.to_string()))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn clear_stale_lock_clears_unheld_file_without_deleting_it() {
        let dir = std::env::temp_dir().join(format!("ax-file-lock-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let lock = dir.join("ax.lock");
        fs::write(&lock, "999999999\n").unwrap();
        assert!(clear_stale_lock(&lock));
        assert!(lock.exists());
        assert_eq!(fs::read_to_string(&lock).unwrap(), "");
        let _ = fs::remove_dir_all(&dir);
    }

    /// A held lock is never cleared, whatever PID the file names (another
    /// container's PID namespace, or no `kill` binary in a distroless image).
    #[cfg(unix)]
    #[test]
    fn clear_stale_lock_keeps_held_lock_with_foreign_pid() {
        let dir = std::env::temp_dir().join(format!("ax-file-lock-held-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let mut holder = FileLock::new(&dir);
        holder.acquire().unwrap();
        let lock = dir.join("ax.lock");
        fs::write(&lock, "999999999").unwrap();
        assert!(!clear_stale_lock(&lock));
        assert!(lock.exists());
        drop(holder);
        let _ = fs::remove_dir_all(&dir);
    }

    /// Dropping a `FileLock` that never acquired must not delete the holder's
    /// file; before the fix the next writer then locked a fresh inode.
    #[cfg(unix)]
    #[test]
    fn dropping_unheld_lock_keeps_the_holders_lock() {
        let dir = std::env::temp_dir().join(format!("ax-file-lock-drop-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let mut holder = FileLock::new(&dir);
        holder.acquire().unwrap();
        drop(FileLock::new(&dir));
        let mut bystander = FileLock::new(&dir);
        let _ = bystander.release();
        drop(bystander);
        assert!(dir.join("ax.lock").exists());
        let mut writer = FileLock::new(&dir);
        let err = writer.acquire_wait(Duration::from_millis(200)).unwrap_err();
        assert!(matches!(err, AxError::LockUnavailable(_)), "{err:?}");
        drop(holder);
        writer.acquire_wait(Duration::from_secs(2)).unwrap();
        let _ = fs::remove_dir_all(&dir);
    }

    /// Windows `LockFileEx` does not conflict across handles in the same process;
    /// this test only verifies cross-handle wait semantics on Unix.
    #[cfg(unix)]
    #[test]
    fn acquire_wait_times_out_while_held() {
        let dir = std::env::temp_dir().join(format!("ax-file-lock-wait-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let mut holder = FileLock::new(&dir);
        holder.acquire().unwrap();
        let mut waiter = FileLock::new(&dir);
        let err = waiter.acquire_wait(Duration::from_millis(200)).unwrap_err();
        assert!(matches!(err, AxError::LockUnavailable(_)), "{err:?}");
        drop(holder);
        let _ = fs::remove_dir_all(&dir);
    }
}
