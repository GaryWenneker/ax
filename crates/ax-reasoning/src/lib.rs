//! Optional remote reasoning offload for ax explore.

mod config;
mod reasoner;

pub use config::{
    config_dir, config_path, is_offload_enabled, read_offload_config, resolve_offload,
    write_offload_config, OffloadConfig,
};
pub use reasoner::{
    maybe_synthesize_explore, offload_status, strip_agent_directives, synthesize_offload,
};
