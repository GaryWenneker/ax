//! Markdown / documentation ingestion into the knowledge graph.
//!
//! Unlike the tree-sitter path, docs are parsed with `pulldown-cmark` in a
//! dedicated post-extraction pass. Each `.md`/`.mdx` file becomes a `Doc` node.
//! Relative links between docs become `References` edges (read directly from the
//! source → `Extracted`), and inline code spans that exactly match a code
//! symbol's name/qualified-name become `Documents` edges from the doc to that
//! symbol (`Inferred`, since the match is heuristic). `Documents` edges stay
//! visible in the graph but are excluded from community/god-node analysis.

use std::path::{Path, PathBuf};

use ax_db::queries::QueryBuilder;
use ax_types::{Edge, EdgeConfidence, EdgeKind, Language, Node, NodeKind, Provenance};
use ignore::gitignore::{Gitignore, GitignoreBuilder};
use ignore::WalkBuilder;
use pulldown_cmark::{CowStr, Event, HeadingLevel, Parser, Tag, TagEnd};

const SKIP_DIRS: &[&str] = &[
    "node_modules", "target", "dist", "build", ".git", "vendor", "tmp", "temp", ".ax",
    ".fastembed_cache",
];

/// Minimum length for an inline-code mention to be considered a code symbol.
const MIN_MENTION_LEN: usize = 3;
/// A mention that resolves to more than this many nodes is too ambiguous to link.
const MAX_MENTION_TARGETS: i64 = 4;

pub fn doc_node_id(rel_path: &str) -> String {
    format!("doc:{rel_path}")
}

struct ParsedDoc {
    rel_path: String,
    title: Option<String>,
    outline: Vec<String>,
    end_line: i32,
    /// (destination, line) for markdown links.
    links: Vec<(String, i32)>,
    /// (symbol, line) for inline code spans, de-duplicated.
    mentions: Vec<(String, i32)>,
}

/// Scan, parse, and persist all markdown docs under `project_root`.
///
/// Returns the number of doc nodes written. Existing doc nodes are cleared
/// first so the pass is idempotent.
pub async fn index_markdown(
    project_root: &Path,
    queries: &QueryBuilder,
    exclude: &[String],
) -> Result<usize, ax_utils::errors::AxError> {
    let files = scan_markdown_files(project_root, exclude);
    if files.is_empty() {
        queries.delete_doc_nodes().await?;
        return Ok(0);
    }

    let now = now_ms();
    let mut parsed: Vec<ParsedDoc> = Vec::with_capacity(files.len());
    for (rel, full) in &files {
        let content = match ax_utils::read_text_file(full) {
            Ok(c) => c,
            Err(_) => continue,
        };
        parsed.push(parse_doc(rel.clone(), &content));
    }

    // Refresh: drop the previous doc layer, then rewrite it wholesale.
    queries.delete_doc_nodes().await?;

    let mut nodes: Vec<Node> = Vec::with_capacity(parsed.len());
    let doc_paths: std::collections::HashSet<String> =
        parsed.iter().map(|d| d.rel_path.clone()).collect();

    for doc in &parsed {
        nodes.push(doc_node(doc, now));
    }
    queries.upsert_nodes(&nodes).await?;

    let mut edges: Vec<Edge> = Vec::new();
    for doc in &parsed {
        let source_id = doc_node_id(&doc.rel_path);

        // doc -> doc relative links (read directly from source => Extracted).
        for (dest, line) in &doc.links {
            if let Some(target_rel) = resolve_doc_link(&doc.rel_path, dest, &doc_paths) {
                edges.push(Edge {
                    source: source_id.clone(),
                    target: doc_node_id(&target_rel),
                    kind: EdgeKind::References,
                    metadata: None,
                    line: Some(*line),
                    column: None,
                    provenance: Some(Provenance::Heuristic),
                    confidence: Some(EdgeConfidence::Extracted),
                });
            }
        }

        // doc -> code symbol mentions (name match => Inferred).
        for (symbol, line) in &doc.mentions {
            let targets = queries
                .get_node_ids_by_symbol(symbol, MAX_MENTION_TARGETS + 1)
                .await?;
            if targets.is_empty() || targets.len() as i64 > MAX_MENTION_TARGETS {
                continue;
            }
            for target in targets {
                edges.push(Edge {
                    source: source_id.clone(),
                    target,
                    kind: EdgeKind::Documents,
                    metadata: None,
                    line: Some(*line),
                    column: None,
                    provenance: Some(Provenance::Heuristic),
                    confidence: Some(EdgeConfidence::Inferred),
                });
            }
        }
    }

    if !edges.is_empty() {
        queries.upsert_edges(&edges).await?;
    }

    Ok(nodes.len())
}

fn doc_node(doc: &ParsedDoc, now: i64) -> Node {
    let name = doc
        .rel_path
        .rsplit('/')
        .next()
        .unwrap_or(&doc.rel_path)
        .to_string();
    let signature = if doc.outline.is_empty() {
        None
    } else {
        Some(doc.outline.join(" > "))
    };
    Node {
        id: doc_node_id(&doc.rel_path),
        kind: NodeKind::Doc,
        name,
        qualified_name: doc.rel_path.clone(),
        file_path: doc.rel_path.clone(),
        language: Language::Unknown,
        start_line: 1,
        end_line: doc.end_line.max(1),
        start_column: 0,
        end_column: 0,
        docstring: doc.title.clone(),
        signature,
        visibility: None,
        is_exported: None,
        is_async: None,
        is_static: None,
        is_abstract: None,
        decorators: None,
        type_parameters: None,
        return_type: None,
        updated_at: now,
    }
}

fn parse_doc(rel_path: String, content: &str) -> ParsedDoc {
    let line_starts = line_start_table(content);
    let end_line = line_starts.len() as i32;

    let mut title: Option<String> = None;
    let mut outline: Vec<String> = Vec::new();
    let mut links: Vec<(String, i32)> = Vec::new();
    let mut seen_mentions: std::collections::HashSet<String> = std::collections::HashSet::new();
    let mut mentions: Vec<(String, i32)> = Vec::new();

    let mut heading: Option<(HeadingLevel, String)> = None;

    for (event, range) in Parser::new(content).into_offset_iter() {
        let line = byte_to_line(&line_starts, range.start);
        match event {
            Event::Start(Tag::Heading { level, .. }) => {
                heading = Some((level, String::new()));
            }
            Event::End(TagEnd::Heading(_)) => {
                if let Some((level, text)) = heading.take() {
                    let text = text.trim().to_string();
                    if !text.is_empty() {
                        if title.is_none() && level == HeadingLevel::H1 {
                            title = Some(text.clone());
                        }
                        if outline.len() < 8 {
                            outline.push(text);
                        }
                    }
                }
            }
            Event::Start(Tag::Link { dest_url, .. }) => {
                links.push((dest_url.to_string(), line));
            }
            Event::Text(t) => {
                if let Some((_, ref mut buf)) = heading {
                    buf.push_str(&t);
                }
            }
            Event::Code(code) => {
                if let Some((_, ref mut buf)) = heading {
                    buf.push_str(&code);
                }
                if let Some(sym) = mention_symbol(&code) {
                    if seen_mentions.insert(sym.clone()) {
                        mentions.push((sym, line));
                    }
                }
            }
            _ => {}
        }
    }

    ParsedDoc {
        rel_path,
        title,
        outline,
        end_line,
        links,
        mentions,
    }
}

/// Normalize an inline-code span into a candidate symbol, or `None` if it is
/// not plausibly a code identifier (too short, contains whitespace, etc.).
fn mention_symbol(code: &CowStr) -> Option<String> {
    let raw = code.trim();
    // Trailing `()` is common for function mentions: `foo()` -> `foo`.
    let raw = raw.strip_suffix("()").unwrap_or(raw);
    if raw.len() < MIN_MENTION_LEN {
        return None;
    }
    if raw.chars().any(|c| c.is_whitespace()) {
        return None;
    }
    // Require at least one identifier-ish character and no shell/path noise.
    let ok = raw
        .chars()
        .all(|c| c.is_alphanumeric() || matches!(c, '_' | '.' | ':' | '<' | '>'));
    if !ok {
        return None;
    }
    if !raw.chars().next().is_some_and(|c| c.is_alphabetic() || c == '_') {
        return None;
    }
    Some(raw.to_string())
}

/// Resolve a markdown link destination against the linking doc's directory.
/// Returns the target doc's rel path when it points at another indexed doc.
fn resolve_doc_link(
    from_rel: &str,
    dest: &str,
    doc_paths: &std::collections::HashSet<String>,
) -> Option<String> {
    let dest = dest.split('#').next().unwrap_or(dest).trim();
    if dest.is_empty() {
        return None;
    }
    let lower = dest.to_ascii_lowercase();
    if lower.starts_with("http://")
        || lower.starts_with("https://")
        || lower.starts_with("mailto:")
        || lower.starts_with("tel:")
        || dest.starts_with('/')
    {
        return None;
    }

    let base_dir = Path::new(from_rel).parent().unwrap_or_else(|| Path::new(""));
    let joined = base_dir.join(dest);
    let normalized = normalize_rel(&joined);

    if doc_paths.contains(&normalized) {
        return Some(normalized);
    }
    // Allow extension-less links to a sibling doc (e.g. `[x](other)`).
    for ext in [".md", ".mdx"] {
        let with_ext = format!("{normalized}{ext}");
        if doc_paths.contains(&with_ext) {
            return Some(with_ext);
        }
    }
    None
}

/// Collapse `.`/`..` components and normalize separators to `/`.
fn normalize_rel(path: &Path) -> String {
    let mut parts: Vec<String> = Vec::new();
    for comp in path.components() {
        use std::path::Component;
        match comp {
            Component::CurDir => {}
            Component::ParentDir => {
                parts.pop();
            }
            Component::Normal(s) => parts.push(s.to_string_lossy().to_string()),
            Component::RootDir | Component::Prefix(_) => {}
        }
    }
    parts.join("/")
}

fn scan_markdown_files(project_root: &Path, exclude: &[String]) -> Vec<(String, PathBuf)> {
    let exclude_matcher = build_exclude_matcher(project_root, exclude);
    let walker = WalkBuilder::new(project_root)
        .hidden(true)
        .git_ignore(true)
        .git_global(true)
        .git_exclude(true)
        .build();
    let mut files = Vec::new();
    for entry in walker.flatten() {
        if !entry.file_type().is_some_and(|t| t.is_file()) {
            continue;
        }
        let path = entry.path();
        let ext = path
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_ascii_lowercase();
        if ext != "md" && ext != "mdx" {
            continue;
        }
        let rel = path
            .strip_prefix(project_root)
            .unwrap_or(path)
            .to_string_lossy()
            .replace('\\', "/");
        if should_skip_path(&rel, exclude_matcher.as_ref()) {
            continue;
        }
        files.push((rel, path.to_path_buf()));
    }
    files
}

fn build_exclude_matcher(project_root: &Path, patterns: &[String]) -> Option<Gitignore> {
    if patterns.is_empty() {
        return None;
    }
    let mut builder = GitignoreBuilder::new(project_root);
    for pat in patterns {
        let _ = builder.add_line(None, pat);
    }
    builder.build().ok()
}

fn should_skip_path(rel: &str, exclude: Option<&Gitignore>) -> bool {
    let rel_norm = rel.replace('\\', "/");
    for segment in rel_norm.split('/') {
        if SKIP_DIRS.contains(&segment) {
            return true;
        }
    }
    if let Some(ex) = exclude {
        if ex.matched(rel_norm.as_str(), false).is_ignore() {
            return true;
        }
    }
    false
}

fn line_start_table(content: &str) -> Vec<usize> {
    let mut starts = vec![0usize];
    for (i, b) in content.bytes().enumerate() {
        if b == b'\n' {
            starts.push(i + 1);
        }
    }
    starts
}

fn byte_to_line(line_starts: &[usize], offset: usize) -> i32 {
    match line_starts.binary_search(&offset) {
        Ok(idx) => idx as i32 + 1,
        Err(idx) => idx as i32,
    }
}

fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_title_links_and_mentions() {
        let content = "# Auth Guide\n\nSee [setup](./setup.md) and call `login_user()`.\n\n## Details\n";
        let doc = parse_doc("docs/auth.md".to_string(), content);
        assert_eq!(doc.title.as_deref(), Some("Auth Guide"));
        assert!(doc.outline.iter().any(|h| h == "Details"));
        assert!(doc.links.iter().any(|(d, _)| d == "./setup.md"));
        assert!(doc.mentions.iter().any(|(m, _)| m == "login_user"));
    }

    #[test]
    fn resolves_relative_doc_links() {
        let mut docs = std::collections::HashSet::new();
        docs.insert("docs/setup.md".to_string());
        let r = resolve_doc_link("docs/auth.md", "./setup.md", &docs);
        assert_eq!(r.as_deref(), Some("docs/setup.md"));
        let r2 = resolve_doc_link("docs/auth.md", "../README.md", &docs);
        assert_eq!(r2, None);
        let ext_less = resolve_doc_link("docs/auth.md", "setup", &docs);
        assert_eq!(ext_less.as_deref(), Some("docs/setup.md"));
    }

    #[test]
    fn skips_external_links() {
        let docs = std::collections::HashSet::new();
        assert_eq!(resolve_doc_link("a.md", "https://example.com", &docs), None);
        assert_eq!(resolve_doc_link("a.md", "mailto:x@y.z", &docs), None);
    }

    #[test]
    fn ignores_noisy_mentions() {
        assert!(mention_symbol(&CowStr::from("ax")).is_none()); // too short
        assert!(mention_symbol(&CowStr::from("npm run build")).is_none()); // whitespace
        assert_eq!(mention_symbol(&CowStr::from("foo()")).as_deref(), Some("foo"));
        assert_eq!(
            mention_symbol(&CowStr::from("MyStruct")).as_deref(),
            Some("MyStruct")
        );
    }
}
