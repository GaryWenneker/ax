//! Authenticated remote MCP and native browser sign-in. Stdio remains unchanged.
pub mod auth;
pub mod client;
pub mod config;
pub mod credentials;
pub mod server;

pub use config::RemoteConfig;
pub use server::serve;
