//! Cross-process file lock using ax.lock (PID stamped, stale recovery).

use std::fs::{File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};

use fs2::FileExt;

use crate::errors::AxError;
use crate::process::is_pid_alive;

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

    /// Clear a stale lock file before attempting to acquire (safe to call always).
    pub fn prepare(&self) {
        if self.file.is_none() {
            let _ = clear_stale_lock(&self.path);
        }
    }

    pub fn acquire(&mut self) -> Result<(), AxError> {
        self.prepare();
        self.acquire_inner(false)
    }

    fn acquire_inner(&mut self, retried: bool) -> Result<(), AxError> {
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
                stamp_pid(&file)?;
                self.file = Some(file);
                Ok(())
            }
            Err(_) => {
                if !retried {
                    clear_stale_lock(&self.path);
                    return self.acquire_inner(true);
                }
                let holder = read_lock_pid(&self.path);
                let hint = match holder {
                    Some(pid) if is_pid_alive(pid) => {
                        format!(
                            "lock unavailable: {} (held by PID {pid} — run `ax unlock` or stop that process)",
                            self.path.display()
                        )
                    }
                    Some(pid) => {
                        format!(
                            "lock unavailable: {} (stale PID {pid} — run `ax unlock`)",
                            self.path.display()
                        )
                    }
                    None => format!("lock unavailable: {} (run `ax unlock`)", self.path.display()),
                };
                Err(AxError::LockUnavailable(hint))
            }
        }
    }

    pub fn release(&mut self) -> Result<(), AxError> {
        if let Some(file) = self.file.take() {
            file.unlock().map_err(|e| {
                AxError::File(crate::errors::FileError::with_path(
                    e.to_string(),
                    self.path.display().to_string(),
                ))
            })?;
        }
        let _ = std::fs::remove_file(&self.path);
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

/// Remove lock file when the holder PID is dead, missing, or unreadable.
pub fn clear_stale_lock(lock_path: &Path) -> bool {
    if !lock_path.exists() {
        return false;
    }
    match read_lock_pid(lock_path) {
        Some(pid) if is_pid_alive(pid) => false,
        Some(_) | None => {
            let _ = std::fs::remove_file(lock_path);
            true
        }
    }
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
    fn clear_stale_lock_removes_dead_pid() {
        let dir = std::env::temp_dir().join(format!("ax-file-lock-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let lock = dir.join("ax.lock");
        fs::write(&lock, "999999999\n").unwrap();
        assert!(clear_stale_lock(&lock));
        assert!(!lock.exists());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn clear_stale_lock_keeps_live_pid() {
        let dir = std::env::temp_dir().join(format!("ax-file-lock-live-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let lock = dir.join("ax.lock");
        fs::write(&lock, format!("{}\n", std::process::id())).unwrap();
        assert!(!clear_stale_lock(&lock));
        assert!(lock.exists());
        let _ = fs::remove_dir_all(&dir);
    }
}
