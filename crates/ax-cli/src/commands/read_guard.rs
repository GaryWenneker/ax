//! Hidden `ax read-guard --ide <dialect>` — pre-tool hook for agent IDEs.
//!
//! Denies the first whole-file read of an indexed source file, or the first
//! search for a symbol the graph knows, per conversation, and tells the agent
//! which graph call answers it. An identical retry is allowed so edits never
//! deadlock. Every error path allows: a hook must never break the agent.
//!
//! Spec: `docs/specs/read-guard-hook.md`.

use std::path::{Component, Path, PathBuf};
use std::str::FromStr;
use std::time::Duration;

use serde_json::{json, Value};
use sqlx::sqlite::SqliteConnectOptions;
use sqlx::{Connection, Row, SqliteConnection};

const TTL_SECS: i64 = 4 * 60 * 60;
const STATE_CAP: usize = 2_000;
const MAX_LISTED: usize = 8;
const MIN_SYMBOL_LEN: usize = 3;
const EVALUATE_BUDGET: Duration = Duration::from_millis(1_500);
const NON_SYMBOL_KINDS: &str = "('file','doc','table')";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Dialect {
    Cursor,
    Claude,
    Gemini,
    Windsurf,
    Codex,
}

impl Dialect {
    pub fn parse(raw: &str) -> Option<Self> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "cursor" => Some(Self::Cursor),
            "claude" | "vscode" | "copilot" => Some(Self::Claude),
            "gemini" => Some(Self::Gemini),
            "windsurf" => Some(Self::Windsurf),
            "codex" => Some(Self::Codex),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Probe {
    Pass,
    Read { path: PathBuf },
    Search { pattern: String, path: Option<PathBuf> },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Denial {
    pub full: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Rendered {
    pub stdout: Option<String>,
    pub stderr: Option<String>,
    pub exit_code: i32,
}

// ---- classify -----------------------------------------------------------

fn str_field<'a>(obj: &'a Value, keys: &[&str]) -> Option<&'a str> {
    keys.iter()
        .filter_map(|k| obj.get(*k).and_then(Value::as_str))
        .find(|s| !s.trim().is_empty())
}

fn is_partial_read(tool_input: &Value) -> bool {
    ["offset", "limit", "startLine", "endLine", "start_line", "end_line"]
        .iter()
        .any(|k| tool_input.get(*k).is_some_and(|v| !v.is_null()))
}

pub fn classify(input: &Value) -> Probe {
    let empty = Value::Null;
    if let Some(action) = input.get("agent_action_name").and_then(Value::as_str) {
        let info = input.get("tool_info").unwrap_or(&empty);
        return match action {
            "pre_read_code" => str_field(info, &["file_path"])
                .map(|p| Probe::Read { path: p.into() })
                .unwrap_or(Probe::Pass),
            "pre_run_command" => str_field(info, &["command_line"])
                .map(classify_shell)
                .unwrap_or(Probe::Pass),
            _ => Probe::Pass,
        };
    }
    let tool = input.get("tool_name").and_then(Value::as_str).unwrap_or("");
    let ti = input.get("tool_input").unwrap_or(&empty);
    match tool {
        "Read" | "read_file" | "readFile" | "copilot_readFile" | "view" => {
            if is_partial_read(ti) {
                return Probe::Pass;
            }
            str_field(ti, &["path", "file_path", "absolute_path", "filePath", "target_file"])
                .map(|p| Probe::Read { path: p.into() })
                .unwrap_or(Probe::Pass)
        }
        "Grep" | "grep" | "grep_search" | "search_file_content" => match str_field(ti, &["pattern", "query"]) {
            Some(pattern) => Probe::Search {
                pattern: pattern.to_string(),
                path: str_field(ti, &["path", "dir_path"]).map(PathBuf::from),
            },
            None => Probe::Pass,
        },
        "Bash" | "Shell" | "shell" | "run_shell_command" | "run_in_terminal" => str_field(ti, &["command"])
            .map(classify_shell)
            .unwrap_or(Probe::Pass),
        _ => Probe::Pass,
    }
}

// ---- shell --------------------------------------------------------------

#[derive(Debug, PartialEq, Eq)]
enum Tok {
    Word(String),
    Op,
}

/// Words of the first simple command, plus whether anything follows it.
/// `None` when the command uses syntax the guard does not parse.
fn first_simple_command(cmd: &str) -> Option<(Vec<String>, bool)> {
    if cmd.contains('`') || cmd.contains("$(") {
        return None;
    }
    let mut toks = Vec::new();
    let mut cur = String::new();
    let mut quoted = false;
    let mut chars = cmd.chars().peekable();
    let flush = |cur: &mut String, quoted: &mut bool, toks: &mut Vec<Tok>| {
        if !cur.is_empty() || *quoted {
            toks.push(Tok::Word(std::mem::take(cur)));
        }
        *quoted = false;
    };
    while let Some(c) = chars.next() {
        match c {
            '\'' => {
                quoted = true;
                loop {
                    match chars.next() {
                        Some('\'') => break,
                        Some(ch) => cur.push(ch),
                        None => return None,
                    }
                }
            }
            '"' => {
                quoted = true;
                loop {
                    match chars.next() {
                        Some('"') => break,
                        Some('\\') => cur.push(chars.next()?),
                        Some(ch) => cur.push(ch),
                        None => return None,
                    }
                }
            }
            '\\' => cur.push(chars.next()?),
            c if c.is_whitespace() && c != '\n' => flush(&mut cur, &mut quoted, &mut toks),
            '|' | '&' | ';' | '<' | '>' | '(' | ')' | '\n' => {
                if matches!(c, '<' | '>') && !quoted && !cur.is_empty() && cur.chars().all(|d| d.is_ascii_digit()) {
                    cur.clear();
                }
                flush(&mut cur, &mut quoted, &mut toks);
                toks.push(Tok::Op);
                break;
            }
            c => cur.push(c),
        }
    }
    let followed = matches!(toks.last(), Some(Tok::Op));
    if !followed {
        flush(&mut cur, &mut quoted, &mut toks);
    }
    let words: Vec<String> = toks
        .into_iter()
        .filter_map(|t| match t {
            Tok::Word(w) => Some(w),
            Tok::Op => None,
        })
        .collect();
    Some((words, followed))
}

fn program_name(word: &str) -> &str {
    word.rsplit('/').next().unwrap_or(word)
}

/// Short flags whose value is the next word, per search tool.
fn takes_value(tool: &str, flag: &str) -> bool {
    let common = ["-e", "-f", "-A", "-B", "-C", "-m"];
    let long = [
        "--regexp", "--file", "--after-context", "--before-context", "--context", "--max-count",
        "--glob", "--iglob", "--type", "--type-not", "--include", "--exclude", "--exclude-dir",
        "--ignore-dir", "--max-depth", "--threads", "--max-columns", "--encoding",
    ];
    if common.contains(&flag) || long.contains(&flag) {
        return true;
    }
    match tool {
        "rg" => ["-g", "-t", "-T", "-j", "-M", "-E", "-d"].contains(&flag),
        "grep" | "egrep" | "fgrep" => ["-d", "-D"].contains(&flag),
        "ag" | "ack" => ["-G", "-g"].contains(&flag),
        _ => false,
    }
}

fn parse_search(tool: &str, args: &[String]) -> Probe {
    let mut explicit: Vec<String> = Vec::new();
    let mut positional: Vec<String> = Vec::new();
    let mut i = 0;
    let mut flags_done = false;
    while i < args.len() {
        let a = &args[i];
        if !flags_done && a == "--" {
            flags_done = true;
        } else if !flags_done && a.starts_with('-') && a.len() > 1 {
            let is_pattern_flag = a == "-e" || a == "--regexp";
            if takes_value(tool, a) {
                i += 1;
                if is_pattern_flag {
                    match args.get(i) {
                        Some(v) => explicit.push(v.clone()),
                        None => return Probe::Pass,
                    }
                }
            } else if let Some(v) = a.strip_prefix("--regexp=") {
                explicit.push(v.to_string());
            }
        } else {
            positional.push(a.clone());
        }
        i += 1;
    }
    let (pattern, path) = match explicit.len() {
        0 => {
            let mut it = positional.into_iter();
            match it.next() {
                Some(p) => (p, it.next()),
                None => return Probe::Pass,
            }
        }
        1 => (explicit.remove(0), positional.into_iter().next()),
        _ => return Probe::Pass,
    };
    Probe::Search { pattern, path: path.map(PathBuf::from) }
}

pub fn classify_shell(command: &str) -> Probe {
    let Some((words, followed)) = first_simple_command(command.trim()) else {
        return Probe::Pass;
    };
    let Some(first) = words.first() else {
        return Probe::Pass;
    };
    if first.contains('=') {
        return Probe::Pass;
    }
    let prog = program_name(first);
    match prog {
        "cat" | "bat" | "batcat" | "less" | "more" => {
            if followed {
                return Probe::Pass;
            }
            let files: Vec<&String> = words[1..].iter().filter(|w| !w.starts_with('-')).collect();
            match files.as_slice() {
                [one] => Probe::Read { path: PathBuf::from(one.as_str()) },
                _ => Probe::Pass,
            }
        }
        "rg" | "grep" | "egrep" | "fgrep" | "ag" | "ack" => parse_search(prog, &words[1..]),
        "git" if words.get(1).map(String::as_str) == Some("grep") => parse_search("grep", &words[2..]),
        _ => Probe::Pass,
    }
}

// ---- symbol_name ----------------------------------------------------------

fn is_identifier(s: &str) -> bool {
    let mut chars = s.chars();
    matches!(chars.next(), Some(c) if c.is_ascii_alphabetic() || c == '_')
        && chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
}

pub fn symbol_name(pattern: &str) -> Option<String> {
    let mut p = pattern.trim();
    for (open, close) in [(r"\b", r"\b"), (r"\<", r"\>")] {
        if let Some(inner) = p.strip_prefix(open).and_then(|s| s.strip_suffix(close)) {
            p = inner;
        }
    }
    let segments: Vec<&str> = if p.contains("::") {
        p.split("::").collect()
    } else {
        p.split('.').collect()
    };
    if !segments.iter().all(|s| is_identifier(s)) {
        return None;
    }
    let last = segments.last()?;
    (last.len() >= MIN_SYMBOL_LEN).then(|| last.to_string())
}

// ---- conversation / switches ----------------------------------------------

pub fn conversation_key(input: &Value) -> String {
    str_field(input, &["conversation_id", "session_id", "trajectory_id", "turn_id"])
        .map(str::to_string)
        .unwrap_or_else(|| "anon".to_string())
}

pub fn guard_enabled(env: Option<&str>) -> bool {
    !matches!(
        env.map(|v| v.trim().to_ascii_lowercase()).as_deref(),
        Some("off" | "0" | "false" | "no")
    )
}

// ---- render ---------------------------------------------------------------

pub fn render(dialect: Dialect, denial: Option<&Denial>) -> Rendered {
    let out = |v: Value| Rendered { stdout: Some(v.to_string()), stderr: None, exit_code: 0 };
    let silent = Rendered { stdout: None, stderr: None, exit_code: 0 };
    match (dialect, denial) {
        (Dialect::Cursor, None) => out(json!({ "permission": "allow" })),
        // Cursor hands the agent `user_message`; `agent_message` alone never reaches it.
        (Dialect::Cursor, Some(d)) => out(json!({
            "permission": "deny",
            "user_message": d.full,
            "agent_message": d.full,
        })),
        (Dialect::Claude | Dialect::Codex, Some(d)) => out(json!({
            "hookSpecificOutput": {
                "hookEventName": "PreToolUse",
                "permissionDecision": "deny",
                "permissionDecisionReason": d.full,
            }
        })),
        (Dialect::Gemini, Some(d)) => out(json!({ "decision": "deny", "reason": d.full })),
        (Dialect::Windsurf, Some(d)) => Rendered { stdout: None, stderr: Some(d.full.clone()), exit_code: 2 },
        (_, None) => silent,
    }
}

// ---- state ----------------------------------------------------------------

#[derive(Debug, Default)]
pub struct GuardState {
    entries: Vec<(String, String, i64)>,
}

impl GuardState {
    pub fn load(path: &Path) -> Self {
        let Ok(raw) = std::fs::read_to_string(path) else {
            return Self::default();
        };
        let entries = serde_json::from_str::<Value>(&raw)
            .ok()
            .and_then(|v| v.get("entries").and_then(Value::as_array).cloned())
            .unwrap_or_default()
            .into_iter()
            .filter_map(|e| {
                let a = e.as_array()?;
                Some((a.first()?.as_str()?.to_string(), a.get(1)?.as_str()?.to_string(), a.get(2)?.as_i64()?))
            })
            .collect();
        Self { entries }
    }

    pub fn save(&self, path: &Path) -> Result<(), String> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        let body = json!({ "entries": self.entries.iter().map(|(k, t, ts)| json!([k, t, ts])).collect::<Vec<_>>() });
        let tmp = path.with_extension(format!("tmp{}", std::process::id()));
        std::fs::write(&tmp, body.to_string()).map_err(|e| e.to_string())?;
        std::fs::rename(&tmp, path).map_err(|e| {
            let _ = std::fs::remove_file(&tmp);
            e.to_string()
        })
    }

    pub fn seen(&self, key: &str, target: &str, now: i64) -> bool {
        self.entries.iter().any(|(k, t, ts)| k == key && t == target && now - ts < TTL_SECS)
    }

    pub fn record(&mut self, key: &str, target: &str, now: i64) {
        self.entries.retain(|(k, t, ts)| now - ts < TTL_SECS && !(k == key && t == target));
        self.entries.push((key.to_string(), target.to_string(), now));
        if self.entries.len() > STATE_CAP {
            let excess = self.entries.len() - STATE_CAP;
            self.entries.drain(..excess);
        }
    }
}

// ---- paths / database -------------------------------------------------------

fn input_cwd(input: &Value) -> Option<PathBuf> {
    let from_roots = input
        .get("workspace_roots")
        .and_then(Value::as_array)
        .and_then(|r| r.first())
        .and_then(Value::as_str);
    str_field(input, &["cwd"])
        .or_else(|| input.get("tool_info").and_then(|i| str_field(i, &["cwd"])))
        .or(from_roots)
        .map(PathBuf::from)
        .or_else(|| std::env::current_dir().ok())
}

fn normalize(path: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for c in path.components() {
        match c {
            Component::CurDir => {}
            Component::ParentDir => {
                out.pop();
            }
            other => out.push(other),
        }
    }
    out
}

fn absolute(path: &Path, cwd: Option<&Path>) -> PathBuf {
    if path.is_absolute() {
        normalize(path)
    } else {
        normalize(&cwd.map(|c| c.join(path)).unwrap_or_else(|| path.to_path_buf()))
    }
}

fn project_root(start: &Path) -> Option<PathBuf> {
    let home = dirs::home_dir();
    start
        .ancestors()
        .filter(|a| Some(*a) != home.as_deref())
        .find(|a| a.join(".ax").join("ax.db").is_file())
        .map(Path::to_path_buf)
}

fn relative_key(root: &Path, path: &Path) -> Option<String> {
    let rel = path.strip_prefix(root).ok()?;
    Some(rel.to_string_lossy().replace('\\', "/"))
}

async fn open_db(root: &Path) -> Option<SqliteConnection> {
    let db = root.join(".ax").join("ax.db");
    let opts = SqliteConnectOptions::from_str(&format!("sqlite://{}", db.display()))
        .ok()?
        .read_only(true)
        .busy_timeout(Duration::from_millis(250));
    SqliteConnection::connect_with(&opts).await.ok()
}

struct Symbol {
    label: String,
    kind: String,
    file: String,
    start: i64,
    end: i64,
}

async fn file_symbols(conn: &mut SqliteConnection, rel: &str) -> Option<Vec<Symbol>> {
    sqlx::query("SELECT 1 FROM files WHERE path = ?")
        .bind(rel)
        .fetch_optional(&mut *conn)
        .await
        .ok()??;
    let sql = format!(
        "SELECT name, kind, start_line, end_line FROM nodes WHERE file_path = ? AND kind NOT IN {NON_SYMBOL_KINDS} ORDER BY start_line LIMIT {}",
        MAX_LISTED + 1
    );
    let rows = sqlx::query(&sql).bind(rel).fetch_all(&mut *conn).await.ok()?;
    Some(
        rows.iter()
            .map(|r| Symbol {
                label: r.try_get("name").unwrap_or_default(),
                kind: r.try_get("kind").unwrap_or_default(),
                file: rel.to_string(),
                start: r.try_get("start_line").unwrap_or(0),
                end: r.try_get("end_line").unwrap_or(0),
            })
            .collect(),
    )
}

async fn named_symbols(conn: &mut SqliteConnection, name: &str, scope: &str) -> Option<Vec<Symbol>> {
    let sql = format!(
        "SELECT qualified_name, kind, file_path, start_line, end_line FROM nodes \
         WHERE name = ? AND kind NOT IN {NON_SYMBOL_KINDS} \
         AND (? = '' OR file_path = ? OR substr(file_path, 1, length(?)) = ?) \
         ORDER BY file_path, start_line LIMIT {}",
        MAX_LISTED + 1
    );
    let dir_prefix = format!("{scope}/");
    let rows = sqlx::query(&sql)
        .bind(name)
        .bind(scope)
        .bind(scope)
        .bind(&dir_prefix)
        .bind(&dir_prefix)
        .fetch_all(&mut *conn)
        .await
        .ok()?;
    Some(
        rows.iter()
            .map(|r| Symbol {
                label: r.try_get("qualified_name").unwrap_or_default(),
                kind: r.try_get("kind").unwrap_or_default(),
                file: r.try_get("file_path").unwrap_or_default(),
                start: r.try_get("start_line").unwrap_or(0),
                end: r.try_get("end_line").unwrap_or(0),
            })
            .collect(),
    )
}

fn listing(symbols: &[Symbol], line: impl Fn(&Symbol) -> String) -> String {
    let mut out: Vec<String> = symbols.iter().take(MAX_LISTED).map(|s| format!("  - {}", line(s))).collect();
    if symbols.len() > MAX_LISTED {
        out.push("  - … more in the graph".to_string());
    }
    out.join("\n")
}

// ---- evaluate ---------------------------------------------------------------

async fn read_denial(path: &Path) -> Option<(String, Denial)> {
    let root = project_root(path.parent()?)?;
    let rel = relative_key(&root, path)?;
    let mut conn = open_db(&root).await?;
    let symbols = file_symbols(&mut conn, &rel).await?;
    let _ = conn.close().await;
    if symbols.is_empty() {
        return None;
    }
    let list = listing(&symbols, |s| format!("ax_node(\"{}\") — {} {}, lines {}-{}", s.label, s.kind, s.file, s.start, s.end));
    let full = format!(
        "ax read-guard: {rel} is indexed in the ax graph. Read the symbols through the graph instead of the whole file:\n\
         {list}\n\
         ax_node returns a symbol's full numbered source plus its callers and callees; ax_explore answers \
         \"how does X work\" across files. A partial Read (offset/limit) is not guarded where the IDE sends \
         the range to the hook; Cursor does not, so there a partial Read is denied once as well. \
         If you need the raw text (for example right before an edit), repeat this same read and it will be allowed."
    );
    Some((format!("read:{}", path.display()), Denial { full }))
}

async fn search_denial(pattern: &str, scope: &Path) -> Option<(String, Denial)> {
    let name = symbol_name(pattern)?;
    let root = project_root(scope)?;
    let prefix = relative_key(&root, scope)?;
    let mut conn = open_db(&root).await?;
    let symbols = named_symbols(&mut conn, &name, &prefix).await?;
    let _ = conn.close().await;
    if symbols.is_empty() {
        return None;
    }
    let list = listing(&symbols, |s| format!("{} — {} at {}:{}-{}", s.label, s.kind, s.file, s.start, s.end));
    let full = format!(
        "ax read-guard: `{name}` is a symbol in the ax graph:\n\
         {list}\n\
         Use ax_node(\"{name}\") for its source, ax_callers / ax_callees for usages, or ax_impact for blast radius — \
         instead of a text search. Searching for text that is not a symbol name is not guarded. \
         If you really need the raw text matches, repeat this same search and it will be allowed."
    );
    Some((format!("search:{}:{}", scope.display(), pattern), Denial { full }))
}

pub async fn evaluate(input: &Value, state_path: &Path, now: i64) -> Option<Denial> {
    let probe = classify(input);
    if probe == Probe::Pass {
        return None;
    }
    let cwd = input_cwd(input);
    let (target, denial) = match probe {
        Probe::Pass => return None,
        Probe::Read { path } => read_denial(&absolute(&path, cwd.as_deref())).await?,
        Probe::Search { pattern, path } => {
            let scope = match path {
                Some(p) => absolute(&p, cwd.as_deref()),
                None => normalize(cwd.as_deref()?),
            };
            search_denial(&pattern, &scope).await?
        }
    };
    let key = conversation_key(input);
    let mut state = GuardState::load(state_path);
    if state.seen(&key, &target, now) {
        return None;
    }
    state.record(&key, &target, now);
    state.save(state_path).ok()?;
    Some(denial)
}

fn state_path() -> Option<PathBuf> {
    match std::env::var("AX_READ_GUARD_STATE") {
        Ok(p) if !p.trim().is_empty() => Some(PathBuf::from(p)),
        _ => dirs::home_dir().map(|h| h.join(".ax").join("read-guard.json")),
    }
}

async fn decide(dialect: Dialect) -> Rendered {
    if !guard_enabled(std::env::var("AX_READ_GUARD").ok().as_deref()) {
        return render(dialect, None);
    }
    let mut raw = String::new();
    if std::io::Read::read_to_string(&mut std::io::stdin(), &mut raw).is_err() {
        return render(dialect, None);
    }
    let (Ok(input), Some(state)) = (serde_json::from_str::<Value>(&raw), state_path()) else {
        return render(dialect, None);
    };
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    let denial = tokio::time::timeout(EVALUATE_BUDGET, evaluate(&input, &state, now))
        .await
        .ok()
        .flatten();
    render(dialect, denial.as_ref())
}

pub async fn run(ide: &str) -> Result<(), String> {
    let Some(dialect) = Dialect::parse(ide) else {
        return Ok(());
    };
    let rendered = decide(dialect).await;
    if let Some(out) = rendered.stdout {
        println!("{out}");
    }
    if let Some(err) = rendered.stderr {
        eprintln!("{err}");
    }
    if rendered.exit_code != 0 {
        std::process::exit(rendered.exit_code);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    const TTL: i64 = 4 * 60 * 60;

    // ---- classify -------------------------------------------------------

    #[test]
    fn cursor_whole_read_is_a_read_probe() {
        let input = json!({ "tool_name": "Read", "tool_input": { "path": "src/lib.rs" } });
        assert_eq!(classify(&input), Probe::Read { path: "src/lib.rs".into() });
    }

    #[test]
    fn claude_read_with_offset_or_limit_passes() {
        for input in [
            json!({ "tool_name": "Read", "tool_input": { "file_path": "/r/src/lib.rs", "offset": 10 } }),
            json!({ "tool_name": "Read", "tool_input": { "file_path": "/r/src/lib.rs", "limit": 40 } }),
            json!({ "tool_name": "read_file", "tool_input": { "filePath": "/r/a.rs", "startLine": 1, "endLine": 80 } }),
        ] {
            assert_eq!(classify(&input), Probe::Pass, "{input}");
        }
    }

    #[test]
    fn null_offset_is_still_a_whole_read() {
        let input = json!({ "tool_name": "Read", "tool_input": { "file_path": "/r/a.rs", "offset": null, "limit": null } });
        assert_eq!(classify(&input), Probe::Read { path: "/r/a.rs".into() });
    }

    #[test]
    fn unknown_tools_pass() {
        for input in [
            json!({ "tool_name": "Write", "tool_input": { "path": "a.rs" } }),
            json!({ "tool_name": "MCP: ax_explore", "tool_input": {} }),
            json!({ "tool_name": "editFiles", "tool_input": { "files": ["a.rs"] } }),
            json!({}),
            Value::Null,
        ] {
            assert_eq!(classify(&input), Probe::Pass, "{input}");
        }
    }

    #[test]
    fn other_ide_read_shapes_are_recognised() {
        let windsurf = json!({ "agent_action_name": "pre_read_code", "tool_info": { "file_path": "/r/a.rs" } });
        assert_eq!(classify(&windsurf), Probe::Read { path: "/r/a.rs".into() });
        let gemini = json!({ "tool_name": "read_file", "tool_input": { "absolute_path": "/r/a.rs" } });
        assert_eq!(classify(&gemini), Probe::Read { path: "/r/a.rs".into() });
        let copilot = json!({ "tool_name": "copilot_readFile", "tool_input": { "filePath": "/r/a.rs" } });
        assert_eq!(classify(&copilot), Probe::Read { path: "/r/a.rs".into() });
        for tool in ["readFile", "view"] {
            let input = json!({ "tool_name": tool, "tool_input": { "path": "/r/a.rs" } });
            assert_eq!(classify(&input), Probe::Read { path: "/r/a.rs".into() }, "{tool}");
        }
    }

    #[test]
    fn grep_tools_are_search_probes() {
        let cursor = json!({ "tool_name": "Grep", "tool_input": { "pattern": "resolve_stacks", "path": "crates" } });
        assert_eq!(
            classify(&cursor),
            Probe::Search { pattern: "resolve_stacks".into(), path: Some("crates".into()) }
        );
        let vscode = json!({ "tool_name": "grep_search", "tool_input": { "query": "resolve_stacks" } });
        assert_eq!(classify(&vscode), Probe::Search { pattern: "resolve_stacks".into(), path: None });
        let gemini = json!({ "tool_name": "search_file_content", "tool_input": { "pattern": "x_y_z", "dir_path": "src" } });
        assert_eq!(classify(&gemini), Probe::Search { pattern: "x_y_z".into(), path: Some("src".into()) });
    }

    #[test]
    fn shell_tools_route_through_the_shell_parser() {
        let codex = json!({ "tool_name": "Bash", "tool_input": { "command": "rg -n resolve_stacks crates/" } });
        assert_eq!(
            classify(&codex),
            Probe::Search { pattern: "resolve_stacks".into(), path: Some("crates/".into()) }
        );
        let windsurf = json!({ "agent_action_name": "pre_run_command", "tool_info": { "command_line": "cat src/lib.rs", "cwd": "/r" } });
        assert_eq!(classify(&windsurf), Probe::Read { path: "src/lib.rs".into() });
        let gemini = json!({ "tool_name": "run_shell_command", "tool_input": { "command": "grep -rw foo_bar ." } });
        assert_eq!(classify(&gemini), Probe::Search { pattern: "foo_bar".into(), path: Some(".".into()) });
    }

    #[test]
    fn shell_parser_only_inspects_the_first_simple_command() {
        assert_eq!(classify_shell("cat src/lib.rs"), Probe::Read { path: "src/lib.rs".into() });
        assert_eq!(classify_shell("bat -p 'src/my file.rs'"), Probe::Read { path: "src/my file.rs".into() });
        assert_eq!(classify_shell("cat src/lib.rs | head -20"), Probe::Pass, "piped cat is a partial read");
        assert_eq!(classify_shell("cd crates && cat lib.rs"), Probe::Pass);
        assert_eq!(classify_shell("head -50 src/lib.rs"), Probe::Pass);
        assert_eq!(classify_shell("sed -n 1,40p src/lib.rs"), Probe::Pass);
        assert_eq!(classify_shell("cat a.rs b.rs"), Probe::Pass, "multi-file cat is not one file read");
        assert_eq!(
            classify_shell("rg -e foo_bar -g '*.rs' -A 3"),
            Probe::Search { pattern: "foo_bar".into(), path: None }
        );
        assert_eq!(
            classify_shell("/usr/bin/grep -rn \"fn main\" ."),
            Probe::Search { pattern: "fn main".into(), path: Some(".".into()) }
        );
        assert_eq!(
            classify_shell("git grep -n resolve_stacks"),
            Probe::Search { pattern: "resolve_stacks".into(), path: None }
        );
        assert_eq!(
            classify_shell("rg foo_bar | head"),
            Probe::Search { pattern: "foo_bar".into(), path: None },
            "a piped search is still a search"
        );
        assert_eq!(classify_shell("FOO=1 cargo test"), Probe::Pass);
        assert_eq!(classify_shell(""), Probe::Pass);
    }

    // ---- symbol_name ----------------------------------------------------

    #[test]
    fn symbol_name_accepts_bare_and_qualified_identifiers() {
        assert_eq!(symbol_name("choose_stacks_for_init").as_deref(), Some("choose_stacks_for_init"));
        assert_eq!(symbol_name(r"\bfoo_bar\b").as_deref(), Some("foo_bar"));
        assert_eq!(symbol_name(r"\<FooBar\>").as_deref(), Some("FooBar"));
        assert_eq!(symbol_name("ax_policy::resolve_stacks").as_deref(), Some("resolve_stacks"));
        assert_eq!(symbol_name("Engine.lockAx").as_deref(), Some("lockAx"));
    }

    #[test]
    fn symbol_name_rejects_text_and_regex() {
        for p in ["fn main", "foo.*bar", "ab", "", "\"quoted\"", "error: failed", "[a-z]+", "1abc"] {
            assert_eq!(symbol_name(p), None, "{p:?}");
        }
    }

    // ---- conversation_key / enabled -------------------------------------

    #[test]
    fn conversation_key_prefers_conversation_then_session_then_anon() {
        assert_eq!(conversation_key(&json!({ "conversation_id": "c1", "session_id": "s1" })), "c1");
        assert_eq!(conversation_key(&json!({ "session_id": "s1" })), "s1");
        assert_eq!(conversation_key(&json!({ "trajectory_id": "t1" })), "t1");
        assert_eq!(conversation_key(&json!({ "turn_id": "u1" })), "u1");
        assert_eq!(conversation_key(&json!({ "conversation_id": "" })), "anon");
        assert_eq!(conversation_key(&Value::Null), "anon");
    }

    #[test]
    fn guard_can_be_switched_off() {
        assert!(guard_enabled(None));
        assert!(guard_enabled(Some("on")));
        for off in ["off", "OFF", "0", "false"] {
            assert!(!guard_enabled(Some(off)), "{off}");
        }
    }

    #[test]
    fn dialect_parse() {
        assert_eq!(Dialect::parse("cursor"), Some(Dialect::Cursor));
        assert_eq!(Dialect::parse("Claude"), Some(Dialect::Claude));
        assert_eq!(Dialect::parse("vscode"), Some(Dialect::Claude));
        assert_eq!(Dialect::parse("gemini"), Some(Dialect::Gemini));
        assert_eq!(Dialect::parse("windsurf"), Some(Dialect::Windsurf));
        assert_eq!(Dialect::parse("codex"), Some(Dialect::Codex));
        assert_eq!(Dialect::parse("zed"), None);
    }

    // ---- render ---------------------------------------------------------

    fn denial() -> Denial {
        Denial { full: "full msg".into() }
    }

    fn parse(s: &Option<String>) -> Value {
        serde_json::from_str(s.as_deref().expect("stdout")).expect("json")
    }

    #[test]
    fn render_cursor() {
        let allow = render(Dialect::Cursor, None);
        assert_eq!(parse(&allow.stdout), json!({ "permission": "allow" }));
        assert_eq!(allow.exit_code, 0);
        let deny = render(Dialect::Cursor, Some(&denial()));
        assert_eq!(
            parse(&deny.stdout),
            json!({ "permission": "deny", "user_message": "full msg", "agent_message": "full msg" }),
            "Cursor hands the agent user_message, so it carries the full guidance"
        );
        assert_eq!(deny.exit_code, 0);
    }

    #[test]
    fn render_claude_and_codex() {
        for d in [Dialect::Claude, Dialect::Codex] {
            let allow = render(d, None);
            assert_eq!(allow.stdout, None);
            assert_eq!(allow.exit_code, 0);
            let deny = render(d, Some(&denial()));
            assert_eq!(
                parse(&deny.stdout),
                json!({ "hookSpecificOutput": {
                    "hookEventName": "PreToolUse",
                    "permissionDecision": "deny",
                    "permissionDecisionReason": "full msg"
                } })
            );
            assert_eq!(deny.exit_code, 0);
        }
    }

    #[test]
    fn render_gemini() {
        assert_eq!(render(Dialect::Gemini, None).stdout, None);
        let deny = render(Dialect::Gemini, Some(&denial()));
        assert_eq!(parse(&deny.stdout), json!({ "decision": "deny", "reason": "full msg" }));
        assert_eq!(deny.exit_code, 0);
    }

    #[test]
    fn render_windsurf_blocks_with_exit_2() {
        let allow = render(Dialect::Windsurf, None);
        assert_eq!((allow.stdout, allow.stderr, allow.exit_code), (None, None, 0));
        let deny = render(Dialect::Windsurf, Some(&denial()));
        assert_eq!(deny.stdout, None);
        assert_eq!(deny.stderr.as_deref(), Some("full msg"));
        assert_eq!(deny.exit_code, 2);
    }

    // ---- state ----------------------------------------------------------

    fn temp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("ax-read-guard-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn state_remembers_until_ttl_and_roundtrips() {
        let dir = temp_dir("state");
        let path = dir.join("read-guard.json");
        let mut state = GuardState::load(&path);
        assert!(!state.seen("c", "t", 100));
        state.record("c", "t", 100);
        assert!(state.seen("c", "t", 100 + TTL - 1));
        assert!(!state.seen("c", "t", 100 + TTL + 1), "expired entries do not count");
        assert!(!state.seen("other", "t", 101), "keyed by conversation");
        state.save(&path).unwrap();
        let again = GuardState::load(&path);
        assert!(again.seen("c", "t", 101));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn state_is_capped_and_drops_oldest() {
        let mut state = GuardState::default();
        for i in 0..2_005 {
            state.record("c", &format!("t{i}"), 1_000 + i);
        }
        assert_eq!(state.entries.len(), 2_000);
        assert!(!state.seen("c", "t0", 3_100));
        assert!(state.seen("c", "t2004", 3_100));
    }

    #[test]
    fn corrupt_state_loads_empty() {
        let dir = temp_dir("corrupt");
        let path = dir.join("read-guard.json");
        std::fs::write(&path, "{not json").unwrap();
        assert!(GuardState::load(&path).entries.is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }

    // ---- evaluate (fixture ax.db) ---------------------------------------

    async fn fixture(name: &str) -> PathBuf {
        let root = temp_dir(name);
        std::fs::create_dir_all(root.join(".ax")).unwrap();
        std::fs::create_dir_all(root.join("src")).unwrap();
        std::fs::create_dir_all(root.join("logs")).unwrap();
        std::fs::write(root.join("src/lib.rs"), "fn alpha_one() {}\n").unwrap();
        std::fs::write(root.join("README.md"), "# readme\n").unwrap();
        let db = root.join(".ax/ax.db");
        let url = format!("sqlite://{}?mode=rwc", db.display());
        let pool = sqlx::SqlitePool::connect(&url).await.unwrap();
        for sql in [
            "CREATE TABLE nodes (id TEXT PRIMARY KEY, kind TEXT, name TEXT, qualified_name TEXT, file_path TEXT, start_line INTEGER, end_line INTEGER)",
            "CREATE TABLE files (path TEXT PRIMARY KEY)",
            "INSERT INTO files VALUES ('src/lib.rs')",
            "INSERT INTO nodes VALUES ('1','file','lib.rs','src/lib.rs','src/lib.rs',1,30)",
            "INSERT INTO nodes VALUES ('2','function','alpha_one','src/lib.rs::alpha_one','src/lib.rs',1,12)",
            "INSERT INTO nodes VALUES ('3','struct','BetaTwo','src/lib.rs::BetaTwo','src/lib.rs',14,30)",
            "INSERT INTO nodes VALUES ('4','doc','README.md','README.md','README.md',1,1)",
            "INSERT INTO files VALUES ('docs/guide.md')",
            "INSERT INTO nodes VALUES ('5','doc','guide.md','docs/guide.md','docs/guide.md',1,40)",
            "INSERT INTO nodes VALUES ('6','table','usage_rows','src/lib.rs::usage_rows','src/lib.rs',20,22)",
        ] {
            sqlx::query(sql).execute(&pool).await.unwrap();
        }
        pool.close().await;
        root
    }

    fn read_input(root: &Path, rel: &str, conv: &str) -> Value {
        json!({
            "conversation_id": conv,
            "cwd": root.display().to_string(),
            "tool_name": "Read",
            "tool_input": { "path": root.join(rel).display().to_string() }
        })
    }

    fn search_input(root: &Path, pattern: &str, path: Option<&str>, conv: &str) -> Value {
        let mut ti = json!({ "pattern": pattern });
        if let Some(p) = path {
            ti["path"] = json!(p);
        }
        json!({ "conversation_id": conv, "cwd": root.display().to_string(), "tool_name": "Grep", "tool_input": ti })
    }

    #[tokio::test]
    async fn first_whole_read_of_indexed_source_is_denied_then_allowed() {
        let root = fixture("read").await;
        let state = root.join("state.json");
        let input = read_input(&root, "src/lib.rs", "conv-a");
        let denial = evaluate(&input, &state, 1_000).await.expect("first read denied");
        assert!(denial.full.contains("src/lib.rs"));
        assert!(denial.full.contains("alpha_one"), "{}", denial.full);
        assert!(denial.full.contains("BetaTwo"));
        assert!(denial.full.contains("ax_node"));
        assert!(denial.full.contains("repeat this same read"));
        assert!(
            !denial.full.contains("A partial Read (offset/limit) is not guarded."),
            "Cursor strips offset/limit from the hook payload, so the message must not promise it"
        );
        assert!(denial.full.contains("Cursor"), "{}", denial.full);
        assert!(!denial.full.contains("README"), "file/doc nodes are not symbols");
        assert!(!denial.full.contains("usage_rows"), "table nodes are not symbols");
        assert!(!denial.full.contains("\"lib.rs\""), "the file node itself is not listed");
        assert_eq!(evaluate(&input, &state, 1_001).await, None, "identical retry allowed");
        let other = read_input(&root, "src/lib.rs", "conv-b");
        assert!(evaluate(&other, &state, 1_002).await.is_some(), "new conversation is guarded again");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[tokio::test]
    async fn relative_read_resolves_against_cwd() {
        let root = fixture("relative").await;
        let state = root.join("state.json");
        let input = json!({
            "session_id": "s", "cwd": root.display().to_string(),
            "tool_name": "Read", "tool_input": { "file_path": "src/lib.rs" }
        });
        assert!(evaluate(&input, &state, 1).await.is_some());
        let _ = std::fs::remove_dir_all(&root);
    }

    #[tokio::test]
    async fn non_source_and_partial_reads_pass() {
        let root = fixture("nonsource").await;
        let state = root.join("state.json");
        assert_eq!(evaluate(&read_input(&root, "README.md", "c"), &state, 1).await, None);
        assert_eq!(evaluate(&read_input(&root, "logs/run.log", "c"), &state, 1).await, None);
        assert_eq!(
            evaluate(&read_input(&root, "docs/guide.md", "c"), &state, 1).await,
            None,
            "an indexed file with only a doc node is not source"
        );
        let partial = json!({
            "conversation_id": "c", "tool_name": "Read",
            "tool_input": { "path": root.join("src/lib.rs").display().to_string(), "offset": 5 }
        });
        assert_eq!(evaluate(&partial, &state, 1).await, None);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[tokio::test]
    async fn symbol_search_is_denied_once_with_graph_hits() {
        let root = fixture("search").await;
        let state = root.join("state.json");
        let input = search_input(&root, "alpha_one", None, "c");
        let denial = evaluate(&input, &state, 1).await.expect("symbol search denied");
        assert!(denial.full.contains("src/lib.rs::alpha_one"), "{}", denial.full);
        assert!(denial.full.contains("src/lib.rs:1"));
        assert!(denial.full.contains("ax_callers"));
        assert!(denial.full.contains("repeat this same search"));
        assert_eq!(evaluate(&input, &state, 2).await, None, "identical retry allowed");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[tokio::test]
    async fn non_symbol_or_unknown_or_out_of_index_searches_pass() {
        let root = fixture("search-pass").await;
        let state = root.join("state.json");
        assert_eq!(evaluate(&search_input(&root, "fn alpha_one", None, "c"), &state, 1).await, None);
        assert_eq!(evaluate(&search_input(&root, "no_such_symbol", None, "c"), &state, 1).await, None);
        let logs = root.join("logs").display().to_string();
        assert_eq!(evaluate(&search_input(&root, "alpha_one", Some(&logs), "c"), &state, 1).await, None);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[tokio::test]
    async fn outside_an_ax_project_everything_passes() {
        let dir = temp_dir("outside");
        std::fs::write(dir.join("a.rs"), "fn x() {}\n").unwrap();
        let state = dir.join("state.json");
        let input = json!({ "tool_name": "Read", "tool_input": { "path": dir.join("a.rs").display().to_string() } });
        assert_eq!(evaluate(&input, &state, 1).await, None);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn unwritable_state_fails_open() {
        let root = fixture("unwritable").await;
        let state = root.join("src"); // a directory: writing the state file fails
        assert_eq!(evaluate(&read_input(&root, "src/lib.rs", "c"), &state, 1).await, None);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[tokio::test]
    async fn broken_database_fails_open() {
        let root = temp_dir("broken-db");
        std::fs::create_dir_all(root.join(".ax")).unwrap();
        std::fs::create_dir_all(root.join("src")).unwrap();
        std::fs::write(root.join(".ax/ax.db"), "this is not sqlite").unwrap();
        std::fs::write(root.join("src/lib.rs"), "fn a() {}\n").unwrap();
        let state = root.join("state.json");
        assert_eq!(evaluate(&read_input(&root, "src/lib.rs", "c"), &state, 1).await, None);
        assert_eq!(evaluate(&search_input(&root, "alpha_one", None, "c"), &state, 1).await, None);
        let _ = std::fs::remove_dir_all(&root);
    }
}
