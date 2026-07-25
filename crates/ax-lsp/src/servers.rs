//! Known language servers ax can spawn for enrichment.

use ax_types::Language;

#[derive(Debug, Clone)]
pub struct ServerSpec {
    pub id: &'static str,
    pub language: Language,
    pub command: &'static str,
    pub args: &'static [&'static str],
    /// File extensions this server handles (without dot).
    pub extensions: &'static [&'static str],
}

pub const SERVERS: &[ServerSpec] = &[
    ServerSpec {
        id: "rust-analyzer",
        language: Language::Rust,
        command: "rust-analyzer",
        args: &[],
        extensions: &["rs"],
    },
    ServerSpec {
        id: "typescript-language-server",
        language: Language::Typescript,
        command: "typescript-language-server",
        args: &["--stdio"],
        extensions: &["ts", "tsx", "js", "jsx", "mts", "cts"],
    },
    ServerSpec {
        id: "pyright",
        language: Language::Python,
        command: "pyright-langserver",
        args: &["--stdio"],
        extensions: &["py", "pyi"],
    },
    ServerSpec {
        id: "gopls",
        language: Language::Go,
        command: "gopls",
        args: &[],
        extensions: &["go"],
    },
];

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ServerStatus {
    pub id: String,
    pub command: String,
    pub available: bool,
    pub path: Option<String>,
    pub languages: Vec<String>,
}

pub fn discover_servers() -> Vec<ServerStatus> {
    SERVERS
        .iter()
        .map(|s| {
            let path = which::which(s.command).ok();
            ServerStatus {
                id: s.id.into(),
                command: s.command.into(),
                available: path.is_some(),
                path: path.map(|p| p.display().to_string()),
                languages: vec![format!("{:?}", s.language).to_ascii_lowercase()],
            }
        })
        .collect()
}

pub fn spec_for_extension(ext: &str) -> Option<&'static ServerSpec> {
    let e = ext.trim_start_matches('.').to_ascii_lowercase();
    SERVERS.iter().find(|s| s.extensions.iter().any(|x| *x == e))
}
