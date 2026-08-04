use std::path::PathBuf;

pub mod github;
pub mod gitlab_api;
pub mod onedrive;

#[derive(Debug, Default)]
pub struct PullResult {
    pub pack_dir: Option<PathBuf>,
    pub memory_file: Option<PathBuf>,
    pub files_copied: usize,
}
