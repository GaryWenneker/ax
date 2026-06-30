//! owo-colors helpers for CLI messages.

use owo_colors::OwoColorize;

use super::glyphs::glyphs;

pub fn ok_line(msg: impl AsRef<str>) -> String {
    format!("{} {}", glyphs().ok.green().bold(), msg.as_ref())
}

pub fn err_line(msg: impl AsRef<str>) -> String {
    format!("{} {}", glyphs().err.red().bold(), msg.as_ref())
}

pub fn warn_line(msg: impl AsRef<str>) -> String {
    format!("{} {}", glyphs().warn.yellow().bold(), msg.as_ref())
}

pub fn info_line(msg: impl AsRef<str>) -> String {
    format!("{} {}", glyphs().info.cyan().bold(), msg.as_ref())
}

pub fn dim(msg: impl AsRef<str>) -> String {
    msg.as_ref().dimmed().to_string()
}

pub fn bold(msg: impl AsRef<str>) -> String {
    msg.as_ref().bold().to_string()
}

pub fn accent(msg: impl AsRef<str>) -> String {
    msg.as_ref().cyan().bold().to_string()
}

pub fn kv_line(label: impl AsRef<str>, value: impl AsRef<str>) -> String {
    format!(
        "{} {}",
        format!("{}:", label.as_ref()).dimmed(),
        value.as_ref().bold()
    )
}
