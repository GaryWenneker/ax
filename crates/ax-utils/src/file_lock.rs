//! Cross-process file lock using ax.lock.

use std::fs::{File, OpenOptions};
use std::path::{Path, PathBuf};

use fs2::FileExt;
use thiserror::Error;

use crate::errors::AxError;

#[derive(Debug, Error)]
#[error("lock unavailable: {path}")]
pub struct LockUnavailableError {
    pub path: PathBuf,
}

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

    pub fn acquire(&mut self) -> Result<(), AxError> {
        if self.file.is_some() {
            return Ok(());
        }
        let file = OpenOptions::new()
            .create(true)
            .read(true)
            .write(true)
            .open(&self.path)
            .map_err(|e| AxError::File(crate::errors::FileError::with_path(e.to_string(), self.path.display().to_string())))?;

        match file.try_lock_exclusive() {
            Ok(()) => {
                self.file = Some(file);
                Ok(())
            }
            Err(_) => Err(AxError::LockUnavailable(self.path.display().to_string())),
        }
    }

    pub fn release(&mut self) -> Result<(), AxError> {
        if let Some(file) = self.file.take() {
            file.unlock()
                .map_err(|e| AxError::File(crate::errors::FileError::with_path(e.to_string(), self.path.display().to_string())))?;
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