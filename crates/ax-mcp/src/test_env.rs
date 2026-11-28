//! Environment isolation for this crate's unit tests.
//!
//! Many code paths read process-wide variables such as `AX_HOME_DIR` and
//! `AX_USAGE_DB`, and Rust runs tests in parallel threads of one process. Every
//! test that reaches those paths takes [`shared`]; a test that changes a
//! variable takes [`exclusive`] and restores it with [`EnvVar`] before it lets
//! go. Both point `AX_HOME_DIR` and `AX_USAGE_DB` at a temporary directory for
//! the whole test process first, so no test reads or writes the developer's
//! real home or usage database.

use std::ffi::OsString;
use std::path::PathBuf;
use std::sync::OnceLock;

use tokio::sync::{RwLock, RwLockReadGuard, RwLockWriteGuard};

static ENV: RwLock<()> = RwLock::const_new(());
static BASE: OnceLock<PathBuf> = OnceLock::new();

/// Called with the write lock held, so no test reads the variables while
/// they change.
fn isolate_process() -> &'static PathBuf {
    BASE.get_or_init(|| {
        let base = std::env::temp_dir().join(format!("ax-mcp-tests-{}", uuid::Uuid::new_v4()));
        let home = base.join("home");
        std::fs::create_dir_all(&home).expect("create the test home");
        std::env::set_var("AX_HOME_DIR", &home);
        std::env::set_var("AX_USAGE_DB", base.join("usage.db"));
        base
    })
}

/// For a test that reads the environment: runs alongside other readers,
/// never while a test changes a variable.
pub(crate) async fn shared() -> RwLockReadGuard<'static, ()> {
    if BASE.get().is_none() {
        let _write = ENV.write().await;
        isolate_process();
    }
    ENV.read().await
}

/// For a test that changes a variable; restore it with [`EnvVar`] before
/// the guard is dropped.
pub(crate) async fn exclusive() -> RwLockWriteGuard<'static, ()> {
    let guard = ENV.write().await;
    isolate_process();
    guard
}

/// The temporary home every test sees unless it sets its own.
pub(crate) fn process_home() -> PathBuf {
    BASE.get()
        .expect("take test_env::shared or exclusive first")
        .join("home")
}

/// Sets a variable and puts the previous value back on drop. Only use it
/// while holding [`exclusive`].
pub(crate) struct EnvVar {
    key: &'static str,
    previous: Option<OsString>,
}

impl EnvVar {
    pub(crate) fn set(key: &'static str, value: impl AsRef<std::ffi::OsStr>) -> Self {
        let previous = std::env::var_os(key);
        std::env::set_var(key, value);
        Self { key, previous }
    }
}

impl Drop for EnvVar {
    fn drop(&mut self) {
        match self.previous.take() {
            Some(value) => std::env::set_var(self.key, value),
            None => std::env::remove_var(self.key),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn tests_never_see_the_real_home_or_usage_database() {
        let _env = shared().await;
        let home = std::env::var_os("AX_HOME_DIR").expect("AX_HOME_DIR is set");
        assert_eq!(PathBuf::from(home), process_home());
        let usage = PathBuf::from(std::env::var_os("AX_USAGE_DB").expect("AX_USAGE_DB is set"));
        assert!(usage.starts_with(std::env::temp_dir()), "{usage:?}");
    }

    #[tokio::test]
    async fn env_var_restores_the_previous_value() {
        let _env = exclusive().await;
        let before = std::env::var_os("AX_HOME_DIR");
        {
            let _home = EnvVar::set("AX_HOME_DIR", "/tmp/ax-test-env-other-home");
            assert_eq!(
                std::env::var_os("AX_HOME_DIR").as_deref(),
                Some(std::ffi::OsStr::new("/tmp/ax-test-env-other-home"))
            );
        }
        assert_eq!(std::env::var_os("AX_HOME_DIR"), before);
    }
}
