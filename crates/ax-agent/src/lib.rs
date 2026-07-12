//! Agent terminal: config, profiles, and built-in chat runner.

pub mod chat;
pub mod config;
pub mod profiles;
pub mod r#loop;

pub use chat::{ChatEvent, ChatRunner};
pub use config::{load_agents_config, save_agents_config, AgentsConfig};
pub use profiles::{
    create_profile, ensure_profile_env, list_profiles, profile_data_dir, remove_profile,
    set_active_profile,
    AuthStatus, ProfileEntry,
};
pub use r#loop::{run_agent_turn, stream_answer_chunks, AgentTurnResult, ToolEvent};
