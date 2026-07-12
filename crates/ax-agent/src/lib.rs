//! Agent terminal: config, profiles, and built-in chat runner.

pub mod chat;
pub mod config;
pub mod cursor_auth;
pub mod profiles;
pub mod r#loop;

pub use chat::{ChatEvent, ChatRunner};
pub use config::{load_agents_config, save_agents_config, AgentsConfig};
pub use cursor_auth::{
    active_profile_name, cursor_process_running, cursor_roaming_dir, decode_jwt_payload,
    enrich_snapshot_metadata, jwt_issued_at, jwt_subject, list_profiles as list_cursor_auth_profiles,
    load_profile as load_cursor_auth_profile, read_legacy_auth_json_snapshot, read_live_snapshot,
    save_profile as save_cursor_auth_profile, use_profile as use_cursor_auth_profile,
    CursorAuthSnapshot, SavedProfileMeta,
};
pub use profiles::{
    create_profile, ensure_profile_env, list_profiles, profile_data_dir, remove_profile,
    set_active_profile,
    AuthStatus, ProfileEntry,
};
pub use r#loop::{run_agent_turn, stream_answer_chunks, AgentTurnResult, ToolEvent};
