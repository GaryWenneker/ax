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
    match raw
        .trim()
        .to_ascii_lowercase()
        .replace([' ', '-'], "")
        .as_str()
    {
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

/// Set `mcp.outputFormat` and keep every other setting. A config file that
/// does not parse as a JSON object is left untouched and reported as an error.
pub fn save_output_format_at(path: &Path, format: McpOutputFormat) -> Result<(), String> {
    let invalid = |message: String| std::io::Error::new(std::io::ErrorKind::InvalidData, message);
    ax_utils::update_locked(path, |existing| {
        let mut root: Value = match existing {
            Some(bytes) if !bytes.iter().all(u8::is_ascii_whitespace) => {
                serde_json::from_slice(&bytes).map_err(|e| {
                    invalid(format!(
                        "{} is not valid JSON ({e}); fix it before changing the output format",
                        path.display()
                    ))
                })?
            }
            _ => json!({}),
        };
        let obj = root
            .as_object_mut()
            .ok_or_else(|| invalid("config root is not an object".into()))?;
        let mcp = obj
            .entry("mcp")
            .or_insert_with(|| json!({}))
            .as_object_mut()
            .ok_or_else(|| invalid("mcp is not an object".into()))?;
        mcp.insert("outputFormat".into(), json!(format.as_str()));
        serde_json::to_vec_pretty(&root)
            .map(Some)
            .map_err(std::io::Error::other)
    })
    .map(|_| ())
    .map_err(|e| e.to_string())
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

/// One `<ax_...>` or `</ax_...>` tag found in a reply.
struct AxTag<'a> {
    start: usize,
    end: usize,
    name: &'a str,
    closing: bool,
}

/// Find the next ax tag. Any other `<` (code such as `a < b`, HTML in a
/// snippet) is ordinary text.
fn next_ax_tag(text: &str) -> Option<AxTag<'_>> {
    let mut from = 0;
    while let Some(offset) = text[from..].find('<') {
        let start = from + offset;
        let rest = &text[start + 1..];
        let (closing, body) = match rest.strip_prefix('/') {
            Some(b) => (true, b),
            None => (false, rest),
        };
        if body.starts_with("ax_") {
            let name_len = body
                .find(|c: char| !(c.is_ascii_alphanumeric() || c == '_' || c == '-'))
                .unwrap_or(body.len());
            let name = &body[..name_len];
            let after = &body[name_len..];
            if let Some(close) = after.find(['>', '<', '\n']) {
                let attrs_ok = if closing {
                    after[..close].trim().is_empty()
                } else {
                    close == 0 || after.starts_with(char::is_whitespace)
                };
                if after.as_bytes()[close] == b'>' && attrs_ok {
                    let end = start + 1 + usize::from(closing) + name_len + close + 1;
                    return Some(AxTag {
                        start,
                        end,
                        name,
                        closing,
                    });
                }
            }
        }
        from = start + 1;
    }
    None
}

fn to_plaintext(text: &str) -> String {
    let mut out = String::new();
    let mut rest = text;
    while let Some(tag) = next_ax_tag(rest) {
        out.push_str(&rest[..tag.start]);
        rest = &rest[tag.end..];
    }
    out.push_str(rest);
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
    while let Some(tag) = next_ax_tag(rest) {
        out.push_str(&rest[..tag.start]);
        if !tag.closing {
            out.push_str(&format!("\n\n## {}\n\n", tag.name));
        }
        rest = &rest[tag.end..];
    }
    out.push_str(rest);
    out
}

fn to_richtext(text: &str) -> String {
    let mut body = String::new();
    let mut rest = text;
    while let Some(tag) = next_ax_tag(rest) {
        body.push_str(&escape_html(&rest[..tag.start]));
        if !tag.closing {
            body.push_str(&format!("<h2>{}</h2>", escape_html(tag.name)));
        }
        rest = &rest[tag.end..];
    }
    body.push_str(&escape_html(rest));
    format!("<article class=\"ax-mcp\">{body}</article>")
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

    #[test]
    fn save_refuses_to_replace_an_unparsable_config() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.json");
        let original = "{\"budget\": {\"monthlyUsd\": 50},}\n";
        fs::write(&path, original).unwrap();
        assert!(save_output_format_at(&path, McpOutputFormat::Markdown).is_err());
        assert_eq!(fs::read_to_string(&path).unwrap(), original);
    }

    #[test]
    fn save_keeps_other_settings() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.json");
        fs::write(&path, "{\"budget\": {\"monthlyUsd\": 50}}").unwrap();
        save_output_format_at(&path, McpOutputFormat::Markdown).unwrap();
        let root: Value = serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(root["budget"]["monthlyUsd"], 50);
        assert_eq!(
            load_output_format_from(&path),
            Some(McpOutputFormat::Markdown)
        );
    }

    #[test]
    fn only_ax_tags_are_rewritten() {
        let raw = "<ax_policy>\nif a < b && c > d { x }\n</ax_policy>\n";
        let plain = render_mcp_output(raw, McpOutputFormat::Plaintext);
        assert!(plain.contains("if a < b && c > d { x }"), "{plain}");
        assert!(!plain.contains("ax_policy"));
        let md = render_mcp_output(raw, McpOutputFormat::Markdown);
        assert!(md.contains("if a < b && c > d { x }"), "{md}");
        assert!(md.contains("## ax_policy"));
        assert!(!md.contains("## b"));
        let rich = render_mcp_output(raw, McpOutputFormat::RichText);
        assert!(
            rich.contains("if a &lt; b &amp;&amp; c &gt; d { x }"),
            "{rich}"
        );
        assert!(!rich.contains("<h2>b</h2>"));
    }
}
