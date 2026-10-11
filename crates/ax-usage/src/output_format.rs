//! MCP tool text format. Stored at `mcp.outputFormat` in `~/.ax/config.json`.

use std::fs;
use std::path::Path;

use serde_json::{json, Value};

use crate::budget::global_config_path;

/// How ax turns a tool reply into the text the client shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum McpOutputFormat {
    Xml,
    Plaintext,
    RichText,
    Markdown,
}

impl McpOutputFormat {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Xml => "xml",
            Self::Plaintext => "plaintext",
            Self::RichText => "richtext",
            Self::Markdown => "markdown",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Xml => "XML",
            Self::Plaintext => "Plaintext",
            Self::RichText => "Rich text",
            Self::Markdown => "Markdown",
        }
    }
}

pub fn parse_output_format(raw: &str) -> Result<McpOutputFormat, String> {
    match raw.trim().to_ascii_lowercase().replace([' ', '-'], "").as_str() {
        "xml" => Ok(McpOutputFormat::Xml),
        "plaintext" | "plain" | "text" => Ok(McpOutputFormat::Plaintext),
        "richtext" | "rich" | "html" => Ok(McpOutputFormat::RichText),
        "markdown" | "md" => Ok(McpOutputFormat::Markdown),
        other => Err(format!(
            "output format must be xml, plaintext, richtext, or markdown, got {other}"
        )),
    }
}

pub fn load_output_format() -> McpOutputFormat {
    global_config_path()
        .as_deref()
        .and_then(load_output_format_from)
        .unwrap_or(McpOutputFormat::Xml)
}

pub fn load_output_format_from(path: &Path) -> Option<McpOutputFormat> {
    let text = fs::read_to_string(path).ok()?;
    let root: Value = serde_json::from_str(&text).ok()?;
    let raw = root
        .get("mcp")
        .and_then(|v| v.get("outputFormat"))
        .and_then(|v| v.as_str())?;
    parse_output_format(raw).ok()
}

pub fn save_output_format(format: McpOutputFormat) -> Result<(), String> {
    let path = global_config_path().ok_or("no home directory for ~/.ax/config.json")?;
    save_output_format_at(&path, format)
}

pub fn save_output_format_at(path: &Path, format: McpOutputFormat) -> Result<(), String> {
    let mut root: Value = fs::read_to_string(path)
        .ok()
        .and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or_else(|| json!({}));
    let obj = root
        .as_object_mut()
        .ok_or("config root is not an object")?;
    let mcp = obj
        .entry("mcp")
        .or_insert_with(|| json!({}))
        .as_object_mut()
        .ok_or("mcp is not an object")?;
    mcp.insert("outputFormat".into(), json!(format.as_str()));
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let body = serde_json::to_string_pretty(&root).map_err(|e| e.to_string())?;
    fs::write(path, body).map_err(|e| e.to_string())
}

/// Rewrite a tool reply. XML is the tagged preflight ax already emits.
pub fn render_mcp_output(text: &str, format: McpOutputFormat) -> String {
    match format {
        McpOutputFormat::Xml => text.to_string(),
        McpOutputFormat::Plaintext => to_plaintext(text),
        McpOutputFormat::RichText => to_richtext(text),
        McpOutputFormat::Markdown => to_markdown(text),
    }
}

fn to_plaintext(text: &str) -> String {
    let mut out = String::new();
    let mut in_tag = false;
    for ch in text.chars() {
        if ch == '<' {
            in_tag = true;
        } else if ch == '>' {
            in_tag = false;
        } else if !in_tag {
            out.push(ch);
        }
    }
    let mut collapsed = String::new();
    let mut blank = 0;
    for line in out.lines() {
        let line = line.trim();
        if line.is_empty() {
            blank += 1;
            if blank <= 1 && !collapsed.is_empty() {
                collapsed.push('\n');
            }
            continue;
        }
        blank = 0;
        if !collapsed.is_empty() && !collapsed.ends_with('\n') {
            collapsed.push('\n');
        }
        collapsed.push_str(line);
        collapsed.push('\n');
    }
    collapsed
}

fn to_markdown(text: &str) -> String {
    let mut out = String::new();
    let mut rest = text;
    while let Some(start) = rest.find('<') {
        out.push_str(&rest[..start]);
        let after = &rest[start + 1..];
        let Some(end) = after.find('>') else {
            out.push_str(&rest[start..]);
            break;
        };
        let tag = &after[..end];
        rest = &after[end + 1..];
        if let Some(name) = opening_tag_name(tag) {
            out.push_str(&format!("\n\n## {name}\n\n"));
        }
    }
    out.push_str(rest);
    out
}

fn to_richtext(text: &str) -> String {
    let mut body = String::new();
    let mut rest = text;
    while let Some(start) = rest.find('<') {
        body.push_str(&escape_html(&rest[..start]));
        let after = &rest[start + 1..];
        let Some(end) = after.find('>') else {
            body.push_str(&escape_html(&rest[start..]));
            break;
        };
        let tag = &after[..end];
        rest = &after[end + 1..];
        if let Some(name) = opening_tag_name(tag) {
            body.push_str(&format!("<h2>{}</h2>", escape_html(name)));
        }
    }
    body.push_str(&escape_html(rest));
    format!("<article class=\"ax-mcp\">{body}</article>")
}

fn opening_tag_name(tag: &str) -> Option<&str> {
    let tag = tag.trim();
    if tag.starts_with('/') || tag.starts_with('!') {
        return None;
    }
    let name = tag.split_whitespace().next().unwrap_or("");
    (!name.is_empty()).then_some(name)
}

fn escape_html(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_the_four_formats() {
        assert_eq!(parse_output_format("xml").unwrap(), McpOutputFormat::Xml);
        assert_eq!(
            parse_output_format("Plain text").unwrap(),
            McpOutputFormat::Plaintext
        );
        assert_eq!(
            parse_output_format("rich-text").unwrap(),
            McpOutputFormat::RichText
        );
        assert_eq!(
            parse_output_format("markdown").unwrap(),
            McpOutputFormat::Markdown
        );
        assert!(parse_output_format("pdf").is_err());
    }

    #[test]
    fn xml_is_unchanged_and_the_others_drop_tags() {
        let raw = "<ax_policy>\nKeep this.\n</ax_policy>\n";
        assert_eq!(render_mcp_output(raw, McpOutputFormat::Xml), raw);
        let plain = render_mcp_output(raw, McpOutputFormat::Plaintext);
        assert!(plain.contains("Keep this."));
        assert!(!plain.contains("<ax_policy"));
        let md = render_mcp_output(raw, McpOutputFormat::Markdown);
        assert!(md.contains("## ax_policy"));
        assert!(md.contains("Keep this."));
        let rich = render_mcp_output(raw, McpOutputFormat::RichText);
        assert!(rich.contains("<h2>ax_policy</h2>"));
        assert!(rich.contains("Keep this."));
        assert!(!rich.contains("<ax_policy"));
    }
}
