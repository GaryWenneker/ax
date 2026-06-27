//! File watcher, git hooks, and worktree support.

pub mod git_hooks;
pub mod watcher;
pub mod watch_policy;
pub mod worktree;

pub use watcher::{FileWatcher, WatcherOptions};
pub use git_hooks::{install_git_sync_hooks, remove_git_sync_hooks};
pub use worktree::{detect_worktree_index_mismatch, worktree_mismatch_warning};
