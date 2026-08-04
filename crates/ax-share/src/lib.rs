pub mod auth;
pub mod config;
pub mod paths;
pub mod providers;
pub mod status;
pub mod sync;

pub use auth::microsoft::{
    auth_status as microsoft_auth_status, clear_tokens as microsoft_clear_tokens,
    encode_share_id, get_access_token, poll_device_flow_once, save_tokens, start_device_flow,
    write_ms_client_id_to_config, DeviceFlowStart, MicrosoftAuthStatus, MicrosoftTokenStore,
};
pub use config::{
    load_share_config, project_config_path, write_project_share_config, GithubShareConfig,
    OneDriveShareConfig, ShareConfig, ShareContentConfig, ShareImportMode, ShareProvider,
    DEFAULT_ONEDRIVE_SHARE_URL,
};
pub use status::{load_status as load_share_status, save_status as save_share_status, ShareSyncStatus};
pub use sync::{
    open_policy_pool, run_sync, share_config_for_api, share_status_for_api, SyncDirection,
    SyncRunResult,
};
