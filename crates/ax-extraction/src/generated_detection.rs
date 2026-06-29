//! Detect auto-generated source files.

pub fn is_generated(content: &str) -> bool {
    let head = content.chars().take(2048).collect::<String>();
    let patterns = [
        "// Code generated",
        "// DO NOT EDIT",
        "# Code generated",
        "AUTO-GENERATED",
        "@generated",
        "//go:build ignore",
    ];
    patterns.iter().any(|p| head.contains(p))
}
