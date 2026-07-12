//! Optional remote reasoning offload for ax explore.

mod config;
mod reasoner;

pub use config::{
    config_dir, config_path, is_offload_enabled, read_offload_config, resolve_offload,
    seed_offload_on_init, write_offload_config, OffloadConfig, OffloadInitReport,
    OffloadProviderEntry, OFFLOAD_PROVIDERS,
};
pub use reasoner::{
    maybe_synthesize_explore, offload_status, strip_agent_directives, synthesize_offload,
    ExploreOffloadMeta,
};
