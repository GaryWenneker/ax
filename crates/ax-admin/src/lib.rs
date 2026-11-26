//! ax-admin: a local admin application for a remote Ax.
//!
//! The browser talks to this process on loopback. This process is the only
//! thing that talks to the remote Ax, and it does so through the remote MCP
//! endpoint (`https://host/projects/<id>/mcp`) and nothing else.
pub mod login;
pub mod mcp;
pub mod secrets;
pub mod server;
pub mod store;
