//! Agent terminal: config, profiles, and built-in chat runner.

pub mod chat;
pub mod config;
pub mod cursor_auth;
pub mod r#loop;
pub mod profiles;

pub use chat::{ChatEvent, ChatRunner};
pub use config::{load_agents_config, save_agents_config, AgentsConfig};
pub use cursor_auth::{
    active_profile_name, cursor_process_running, cursor_roaming_dir, decode_jwt_payload,
    enrich_snapshot_metadata, jwt_issued_at, jwt_subject,
    list_profiles as list_cursor_auth_profiles, load_profile as load_cursor_auth_profile,
    read_legacy_auth_json_snapshot, read_live_snapshot, save_profile as save_cursor_auth_profile,
    use_profile as use_cursor_auth_profile, CursorAuthSnapshot, SavedProfileMeta,
};
pub use profiles::{
    create_profile, ensure_profile_env, list_profiles, profile_data_dir, remove_profile,
    set_active_profile, AuthStatus, ProfileEntry,
};
pub use r#loop::{run_agent_turn, stream_answer_chunks, AgentTurnResult, ToolEvent};

#[cfg(test)]
pub(crate) mod test_home {
    //! Points `AX_HOME_DIR` at a fresh folder for one test at a time.
    use std::path::PathBuf;
    use std::sync::{Mutex, MutexGuard};

    static LOCK: Mutex<()> = Mutex::new(());

    pub struct TestHome {
        pub dir: PathBuf,
        _guard: MutexGuard<'static, ()>,
    }

    impl Drop for TestHome {
        fn drop(&mut self) {
            std::env::remove_var("AX_HOME_DIR");
            let _ = std::fs::remove_dir_all(&self.dir);
        }
    }

    pub fn fresh(name: &str) -> TestHome {
        let guard = LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let dir = std::env::temp_dir().join(format!(
            "ax-agent-home-{name}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        std::env::set_var("AX_HOME_DIR", &dir);
        TestHome { dir, _guard: guard }
    }
}
