//! Out-of-tree extractor plugins for ax.
//!
//! Plugins live under `.ax/plugins/<name>/` with a `plugin.toml`:
//!
//! ```toml
//! name = "terraform"
//! extensions = [".tf", ".tfvars"]
//! command = "python"
//! args = ["extract.py"]
//! ```
//!
//! Or with the `wasm` feature:
//!
//! ```toml
//! name = "hcl"
//! extensions = [".hcl"]
//! wasm = "extractor.wasm"
//! ```
//!
//! Process plugins speak JSON on stdin/stdout:
//! input `{ "path", "content" }` → output ExtractionResult JSON.
//!
//! Project plugins only run once trusted (`ax plugins trust`); see [`trust`].

mod host;
mod manifest;
pub mod trust;
#[cfg(feature = "wasm")]
mod wasm_host;

pub use host::{load_plugins, PluginHost, PluginRunError};
pub use manifest::{discover_plugins, plugin_content_hash, LoadedPlugin, PluginManifest};
pub use trust::{is_trusted, trust_plugin, trust_store_path, untrust_plugin};
