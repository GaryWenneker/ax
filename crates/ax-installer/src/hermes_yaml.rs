//! Edits Hermes Agent's `config.yaml`: sets or removes only `mcp_servers.ax`
//! and the `mcp-ax` item of `platform_toolsets.cli`.
//!
//! The file is parsed first; a file that does not parse is refused and left
//! alone. Edits are made on the text so comments and the layout of every other
//! entry stay as written. The edited text is parsed again and must equal the
//! intended value; when the layout is one the text editor does not handle
//! (flow style, for example), the intended value is written as plain YAML.

use serde_yaml::{Mapping, Value};

const SERVERS: &str = "mcp_servers";
const TOOLSETS: &str = "platform_toolsets";
const CLI: &str = "cli";
const AX: &str = "ax";
const TOOLSET: &str = "mcp-ax";

/// The `ax:` server entry, with every line indented by `indent` spaces.
pub(crate) fn ax_entry(bin: &str, path: &str, indent: usize) -> String {
    let scalar = |s: &str| {
        serde_yaml::to_string(&Value::String(s.to_string()))
            .map(|t| t.trim_end().to_string())
            .unwrap_or_else(|_| format!("{s:?}"))
    };
    let (bin, path) = (scalar(bin), scalar(path));
    let pad = " ".repeat(indent);
    [
        "ax:".to_string(),
        format!("  command: {bin}"),
        "  args:".to_string(),
        "    - serve".to_string(),
        "    - --mcp".to_string(),
        "    - --path".to_string(),
        format!("    - {path}"),
        format!("  cwd: {path}"),
        "  timeout: 120".to_string(),
        "  connect_timeout: 60".to_string(),
        "  enabled: true".to_string(),
    ]
    .iter()
    .map(|l| format!("{pad}{l}\n"))
    .collect()
}

fn parse_root(content: &str) -> Result<Mapping, String> {
    if content.trim().is_empty() {
        return Ok(Mapping::new());
    }
    match serde_yaml::from_str::<Value>(content) {
        Ok(Value::Mapping(map)) => Ok(map),
        Ok(Value::Null) => Ok(Mapping::new()),
        Ok(_) => Err("does not hold a YAML mapping".into()),
        Err(e) => Err(format!("is not valid YAML ({e})")),
    }
}

fn key(name: &str) -> Value {
    Value::String(name.to_string())
}

/// `root[name]` as a mapping, created when missing or null.
fn child_mapping<'a>(root: &'a mut Mapping, name: &str) -> Result<&'a mut Mapping, String> {
    let slot = root.entry(key(name)).or_insert(Value::Null);
    if slot.is_null() {
        *slot = Value::Mapping(Mapping::new());
    }
    slot.as_mapping_mut()
        .ok_or_else(|| format!("\"{name}\" is not a mapping"))
}

/// The value `content` should have after installing the ax entry.
fn intended_install(mut root: Mapping, entry: Value) -> Result<Mapping, String> {
    child_mapping(&mut root, SERVERS)?.insert(key(AX), entry);
    let toolsets = child_mapping(&mut root, TOOLSETS)?;
    let cli = toolsets.entry(key(CLI)).or_insert(Value::Null);
    if cli.is_null() {
        *cli = Value::Sequence(Vec::new());
    }
    let list = cli
        .as_sequence_mut()
        .ok_or_else(|| format!("\"{TOOLSETS}.{CLI}\" is not a list"))?;
    if !list.iter().any(|v| v.as_str() == Some(TOOLSET)) {
        list.push(key(TOOLSET));
    }
    Ok(root)
}

/// The value `content` should have after removing the ax entries. Containers
/// left empty by the removal are dropped too, since install created them.
fn intended_remove(mut root: Mapping) -> Mapping {
    let servers_now_empty = match root.get_mut(SERVERS).and_then(Value::as_mapping_mut) {
        Some(servers) => servers.remove(AX).is_some() && servers.is_empty(),
        None => false,
    };
    if servers_now_empty {
        root.remove(SERVERS);
    }
    let mut toolsets_now_empty = false;
    if let Some(toolsets) = root.get_mut(TOOLSETS).and_then(Value::as_mapping_mut) {
        let mut cli_now_empty = false;
        if let Some(list) = toolsets.get_mut(CLI).and_then(Value::as_sequence_mut) {
            let before = list.len();
            list.retain(|v| v.as_str() != Some(TOOLSET));
            cli_now_empty = list.len() < before && list.is_empty();
        }
        if cli_now_empty {
            toolsets.remove(CLI);
            toolsets_now_empty = toolsets.is_empty();
        }
    }
    if toolsets_now_empty {
        root.remove(TOOLSETS);
    }
    root
}

/// Text of `content` with `mcp_servers.ax` set and `mcp-ax` in
/// `platform_toolsets.cli`. Errors when `content` is not a YAML mapping or
/// its `mcp_servers` / `platform_toolsets.cli` have an unexpected type.
pub(crate) fn upsert(content: &str, bin: &str, path: &str) -> Result<String, String> {
    let original = parse_root(content)?;
    let entry_text = ax_entry(bin, path, 0);
    let entry = parse_root(&entry_text)?
        .remove(AX)
        .ok_or("internal: ax entry did not parse")?;
    let intended = intended_install(original.clone(), entry)?;
    if intended == original {
        return Ok(content.to_string());
    }
    let mut lines = Lines::new(content);
    lines.remove_child(SERVERS, AX);
    lines.insert_child(SERVERS, &entry_text);
    if !lines.list_contains(TOOLSETS, CLI, TOOLSET) {
        lines.push_list_item(TOOLSETS, CLI, TOOLSET);
    }
    finish(lines.text(), &intended)
}

/// Text of `content` without the ax entries, or `None` when it has none.
pub(crate) fn remove(content: &str) -> Result<Option<String>, String> {
    let original = parse_root(content)?;
    let intended = intended_remove(original.clone());
    if intended == original {
        return Ok(None);
    }
    let mut lines = Lines::new(content);
    lines.remove_child(SERVERS, AX);
    lines.remove_list_item(TOOLSETS, CLI, TOOLSET);
    lines.drop_if_empty_child(TOOLSETS, CLI);
    lines.drop_if_empty(TOOLSETS);
    lines.drop_if_empty(SERVERS);
    finish(lines.text(), &intended).map(Some)
}

fn finish(edited: String, intended: &Mapping) -> Result<String, String> {
    if parse_root(&edited).ok().as_ref() == Some(intended) {
        return Ok(edited);
    }
    if intended.is_empty() {
        return Ok(String::new());
    }
    serde_yaml::to_string(intended).map_err(|e| e.to_string())
}

/// Line-level view of a block-style YAML document.
struct Lines {
    lines: Vec<String>,
}

fn indent_of(line: &str) -> usize {
    line.len() - line.trim_start_matches(' ').len()
}

fn is_content(line: &str) -> bool {
    let t = line.trim();
    !t.is_empty() && !t.starts_with('#')
}

/// The text after `name:` when `line` is that key at exactly `indent`
/// (trailing comment removed), else `None`.
fn key_rest<'a>(line: &'a str, indent: usize, name: &str) -> Option<&'a str> {
    if indent_of(line) != indent {
        return None;
    }
    let rest = line[indent..].strip_prefix(name)?.strip_prefix(':')?;
    if !(rest.is_empty() || rest.starts_with([' ', '\t'])) {
        return None;
    }
    let rest = rest.trim();
    Some(if rest.starts_with('#') { "" } else { rest })
}

impl Lines {
    fn new(content: &str) -> Self {
        Self {
            lines: content.lines().map(str::to_string).collect(),
        }
    }

    fn text(&self) -> String {
        let mut lines = self.lines.clone();
        while lines.last().is_some_and(|l| l.trim().is_empty()) {
            lines.pop();
        }
        if lines.is_empty() {
            return String::new();
        }
        lines.join("\n") + "\n"
    }

    /// End (exclusive) of the block owned by the key on line `at`: up to the
    /// next content line indented at most `indent`. Trailing blank and
    /// comment lines are not part of the block.
    fn block_end(&self, at: usize, indent: usize, allow_same_indent_items: bool) -> usize {
        let mut last = at;
        for (i, line) in self.lines.iter().enumerate().skip(at + 1) {
            if !is_content(line) {
                continue;
            }
            let ind = indent_of(line);
            let same_indent_item =
                allow_same_indent_items && ind == indent && line.trim_start().starts_with('-');
            if ind <= indent && !same_indent_item {
                break;
            }
            last = i;
        }
        last + 1
    }

    /// Line of the top-level key `name` whose value is a nested block.
    fn top(&self, name: &str) -> Option<usize> {
        self.lines
            .iter()
            .position(|l| key_rest(l, 0, name).is_some_and(str::is_empty))
    }

    /// Indent used by the children of the block starting at line `at`.
    fn child_indent(&self, at: usize, end: usize) -> Option<usize> {
        self.lines[at + 1..end]
            .iter()
            .find(|l| is_content(l))
            .map(|l| indent_of(l))
    }

    /// `(line, indent)` of `child` directly under top-level `parent`.
    fn child(&self, parent: &str, child: &str) -> Option<(usize, usize)> {
        let at = self.top(parent)?;
        let end = self.block_end(at, 0, false);
        let indent = self.child_indent(at, end)?;
        (at + 1..end)
            .find(|&i| key_rest(&self.lines[i], indent, child).is_some())
            .map(|i| (i, indent))
    }

    fn remove_child(&mut self, parent: &str, child: &str) {
        if let Some((at, indent)) = self.child(parent, child) {
            let end = self.block_end(at, indent, false);
            self.lines.drain(at..end);
        }
    }

    /// Adds `entry` (written at indent 0) as the last child of top-level
    /// `parent`, creating `parent` at the end of the file when missing.
    fn insert_child(&mut self, parent: &str, entry: &str) {
        let Some(at) = self.top(parent) else {
            self.append(&format!("{parent}:\n{}", indent_text(entry, 2)));
            return;
        };
        let end = self.block_end(at, 0, false);
        let indent = self.child_indent(at, end).unwrap_or(2);
        let new: Vec<String> = indent_text(entry, indent)
            .lines()
            .map(str::to_string)
            .collect();
        self.lines.splice(end..end, new);
    }

    fn append(&mut self, block: &str) {
        while self.lines.last().is_some_and(|l| l.trim().is_empty()) {
            self.lines.pop();
        }
        if !self.lines.is_empty() {
            self.lines.push(String::new());
        }
        self.lines.extend(block.lines().map(str::to_string));
    }

    /// Lines of the items of list `parent.child`, with the list's range.
    fn list_items(&self, parent: &str, child: &str) -> Option<(usize, usize, usize, Vec<usize>)> {
        let (at, indent) = self.child(parent, child)?;
        let end = self.block_end(at, indent, true);
        let items: Vec<usize> = (at + 1..end)
            .filter(|&i| {
                let l = &self.lines[i];
                is_content(l) && l.trim_start().starts_with('-')
            })
            .collect();
        Some((at, end, indent, items))
    }

    fn list_contains(&self, parent: &str, child: &str, item: &str) -> bool {
        self.list_items(parent, child)
            .is_some_and(|(_, _, _, items)| {
                items
                    .iter()
                    .any(|&i| item_value(&self.lines[i]) == Some(item))
            })
    }

    fn push_list_item(&mut self, parent: &str, child: &str, item: &str) {
        match self.list_items(parent, child) {
            Some((at, end, indent, items)) => {
                let item_indent = items
                    .first()
                    .map(|&i| indent_of(&self.lines[i]))
                    .unwrap_or(indent + 2);
                let line = format!("{}- {item}", " ".repeat(item_indent));
                let after = items.last().map(|&i| i + 1).unwrap_or(at + 1);
                debug_assert!(after <= end.max(at + 1));
                self.lines.insert(after, line);
            }
            None => self.insert_child(parent, &format!("{child}:\n  - {item}\n")),
        }
    }

    fn remove_list_item(&mut self, parent: &str, child: &str, item: &str) {
        if let Some((_, _, _, items)) = self.list_items(parent, child) {
            for &i in items.iter().rev() {
                if item_value(&self.lines[i]) == Some(item) {
                    self.lines.remove(i);
                }
            }
        }
    }

    fn drop_if_empty_child(&mut self, parent: &str, child: &str) {
        if let Some((at, indent)) = self.child(parent, child) {
            if self.block_end(at, indent, true) == at + 1
                && key_rest(&self.lines[at], indent, child) == Some("")
            {
                self.lines.remove(at);
            }
        }
    }

    fn drop_if_empty(&mut self, name: &str) {
        if let Some(at) = self.top(name) {
            if self.block_end(at, 0, false) == at + 1 {
                self.lines.remove(at);
            }
        }
    }
}

fn item_value(line: &str) -> Option<&str> {
    let rest = line.trim_start().strip_prefix('-')?;
    let rest = rest.split(" #").next().unwrap_or(rest).trim();
    Some(rest.trim_matches(|c| c == '"' || c == '\''))
}

fn indent_text(text: &str, indent: usize) -> String {
    let pad = " ".repeat(indent);
    text.lines().map(|l| format!("{pad}{l}\n")).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn yaml(text: &str) -> Value {
        serde_yaml::from_str(text).unwrap_or_else(|e| panic!("{e}\n{text}"))
    }

    #[test]
    fn comments_and_layout_outside_the_ax_entries_are_kept() {
        let original = "# top comment\nmcp_servers:\n    github:   # four-space indent\n        command: npx\nplatform_toolsets:\n  cli: # mine\n  - hermes-cli\n";
        let out = upsert(original, "ax", ".").unwrap();
        assert!(out.starts_with("# top comment\nmcp_servers:\n    github:   # four-space indent\n        command: npx\n    ax:\n"), "{out}");
        assert!(
            out.contains("  cli: # mine\n  - hermes-cli\n  - mcp-ax\n"),
            "{out}"
        );
        let back = remove(&out).unwrap().unwrap();
        assert_eq!(back, original);
    }

    #[test]
    fn flow_style_falls_back_to_plain_yaml_with_the_same_value() {
        let original =
            "mcp_servers: {github: {command: npx}}\nplatform_toolsets: {cli: [hermes-cli]}\n";
        let out = upsert(original, "ax", ".").unwrap();
        let v = yaml(&out);
        assert_eq!(v["mcp_servers"]["github"]["command"], "npx");
        assert_eq!(v["mcp_servers"]["ax"]["command"], "ax");
        assert_eq!(v["platform_toolsets"]["cli"][1], "mcp-ax");
        assert_eq!(yaml(&remove(&out).unwrap().unwrap()), yaml(original));
    }

    #[test]
    fn a_binary_path_that_needs_quoting_round_trips() {
        let bin = r"C:\Program Files\ax: tools\#1\ax.exe";
        let v = yaml(&upsert("", bin, ".").unwrap());
        assert_eq!(v["mcp_servers"]["ax"]["command"], bin);
    }

    #[test]
    fn a_non_list_cli_toolset_is_refused() {
        assert!(upsert("platform_toolsets:\n  cli: all\n", "ax", ".").is_err());
        assert!(upsert("mcp_servers: [a]\n", "ax", ".").is_err());
    }
}
