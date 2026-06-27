//! MCP server for ax.

pub mod daemon;
pub mod daemon_paths;
pub mod daemon_lock;
pub mod query_pool;
pub mod daemon_conn;
pub mod engine;
pub mod liveness_watchdog;
pub mod ppid_watchdog;
pub mod proxy;
pub mod server;
pub mod tools;
pub mod transport;

pub use daemon::run_daemon;
pub use engine::McpEngine;
pub use proxy::attach_or_spawn;
pub use server::run_stdio_server;
pub use liveness_watchdog::run_watchdog_child;
pub use transport::{JsonRpcRequest, JsonRpcResponse, StdioTransport};