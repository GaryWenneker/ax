//! Security-related constants and checks.

use ax_types::{Language, Node, NodeKind};

/// Paths that must never be indexed.
pub const SENSITIVE_PATHS: &[&str] = &[
    "/",
    "/etc",
    "/usr",
    "/bin",
    "/sbin",
    "/var",
    "/proc",
    "/sys",
    "/dev",
    "C:\\",
    "C:\\Windows",
    "C:\\Program Files",
    "C:\\Program Files (x86)",
];

/// Languages whose leaf nodes should not expose values (secrets).
pub const CONFIG_LEAF_LANGUAGES: &[Language] = &[Language::Yaml, Language::Properties];

/// Check if a node is a config leaf that should not expose values.
pub fn is_config_leaf_node(node: &Node) -> bool {
    CONFIG_LEAF_LANGUAGES.contains(&node.language)
        && matches!(
            node.kind,
            NodeKind::Property | NodeKind::Field | NodeKind::Variable | NodeKind::Constant
        )
}
