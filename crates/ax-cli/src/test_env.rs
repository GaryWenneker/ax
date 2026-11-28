//! Process environment changes for unit tests.
//!
//! Tests in this binary run on parallel threads and share one process environment.
//! A test that sets a variable takes [`scoped_env`], which holds one lock for the
//! whole test and puts every variable back when the guard drops. Tests that touch
//! the usage database point `AX_USAGE_DB` at a temp file this way, so they never
//! write to the developer's `~/.ax/usage.db`.

use std::ffi::{OsStr, OsString};

static ENV_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

/// Holds the environment lock and restores the variables it changed on drop.
pub(crate) struct EnvGuard {
    saved: Vec<(String, Option<OsString>)>,
    _lock: tokio::sync::MutexGuard<'static, ()>,
}

/// Set `vars` for the rest of the calling test. Other tests that use this helper
/// wait until the returned guard drops.
pub(crate) async fn scoped_env(vars: &[(&str, &OsStr)]) -> EnvGuard {
    let lock = ENV_LOCK.lock().await;
    let saved = vars
        .iter()
        .map(|(name, value)| {
            let previous = std::env::var_os(name);
            std::env::set_var(name, value);
            ((*name).to_string(), previous)
        })
        .collect();
    EnvGuard { saved, _lock: lock }
}

impl Drop for EnvGuard {
    fn drop(&mut self) {
        for (name, previous) in self.saved.drain(..).rev() {
            match previous {
                Some(value) => std::env::set_var(&name, value),
                None => std::env::remove_var(&name),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn guard_restores_previous_values_on_drop() {
        let name = "AX_TEST_ENV_GUARD_RESTORE";
        {
            let _outer = scoped_env(&[(name, OsStr::new("set"))]).await;
            assert_eq!(std::env::var(name).unwrap(), "set");
        }
        assert!(std::env::var_os(name).is_none());
    }
}
