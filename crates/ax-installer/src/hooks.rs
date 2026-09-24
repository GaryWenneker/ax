//! `ax read-guard` pre-tool hook entries for agent IDEs that can block a tool call.
//!
//! Each IDE has its own hooks file shape. Entries are recognised by the
//! `read-guard` subcommand inside their `command`, so re-installing replaces the
//! entry (for example after the binary moved) and uninstall removes only ours.
//! A hooks file that is not valid JSON is never rewritten.

use std::fs;
use std::path::Path;

use serde_json::{json, Map, Value};

use crate::report::FileAction;

const MARKER: &str = "read-guard";
const TURN_MARKER: &str = "turn-hook";
const GEMINI_NAME: &str = "ax-read-guard";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GuardIde {
    Cursor,
    Claude,
    Gemini,
    Windsurf,
    Codex,
}

impl GuardIde {
    fn dialect(self) -> &'static str {
        match self {
            Self::Cursor => "cursor",
            Self::Claude => "claude",
            Self::Gemini => "gemini",
            Self::Windsurf => "windsurf",
            Self::Codex => "codex",
        }
    }
}

fn quote_bin(bin: &str) -> String {
    if bin.contains(char::is_whitespace) {
        format!("\"{bin}\"")
    } else {
        bin.to_string()
    }
}

pub fn guard_command(bin: &str, ide: GuardIde) -> String {
    format!("{} {MARKER} --ide {}", quote_bin(bin), ide.dialect())
}

fn command_contains(entry: &Value, marker: &str) -> bool {
    entry
        .get("command")
        .and_then(Value::as_str)
        .is_some_and(|c| c.contains(marker))
}

fn is_guard(entry: &Value) -> bool {
    command_contains(entry, MARKER)
}

fn is_turn_hook(entry: &Value) -> bool {
    command_contains(entry, TURN_MARKER)
}

fn group_has_guard(group: &Value) -> bool {
    group
        .get("hooks")
        .and_then(Value::as_array)
        .is_some_and(|hooks| hooks.iter().any(is_guard))
}

fn event_array<'a>(config: &'a mut Value, event: &str) -> Result<&'a mut Vec<Value>, String> {
    if !config.is_object() {
        return Err("hooks file is not a JSON object".into());
    }
    let hooks = config
        .as_object_mut()
        .ok_or("hooks file is not a JSON object")?
        .entry("hooks")
        .or_insert_with(|| Value::Object(Map::new()));
    let hooks = hooks.as_object_mut().ok_or("`hooks` is not an object")?;
    hooks
        .entry(event.to_string())
        .or_insert_with(|| json!([]))
        .as_array_mut()
        .ok_or_else(|| format!("`hooks.{event}` is not an array"))
}

/// Put `entry` where the first matching element was (or append), dropping other matches.
fn replace_or_push(items: &mut Vec<Value>, entry: Value, matches: impl Fn(&Value) -> bool) {
    match items.iter().position(&matches) {
        Some(first) => {
            items[first] = entry;
            let mut idx = 0;
            items.retain(|v| {
                let keep = idx == first || !matches(v);
                idx += 1;
                keep
            });
        }
        None => items.push(entry),
    }
}

pub fn upsert(config: &mut Value, ide: GuardIde, bin: &str) -> Result<(), String> {
    let cmd = guard_command(bin, ide);
    match ide {
        GuardIde::Cursor => {
            if config.get("version").is_none() && config.is_object() {
                config["version"] = json!(1);
            }
            let entry = json!({ "command": cmd, "matcher": "Read|Grep|Shell", "timeout": 5 });
            replace_or_push(event_array(config, "preToolUse")?, entry, is_guard);
        }
        GuardIde::Claude => nested(config, "PreToolUse", "Read|Grep|Bash", json!({ "type": "command", "command": cmd, "timeout": 5 }))?,
        GuardIde::Codex => nested(config, "PreToolUse", "^Bash$", json!({ "type": "command", "command": cmd, "timeout": 5 }))?,
        GuardIde::Gemini => nested(
            config,
            "BeforeTool",
            "read_file|search_file_content|grep|run_shell_command",
            json!({ "name": GEMINI_NAME, "type": "command", "command": cmd, "timeout": 5000 }),
        )?,
        GuardIde::Windsurf => {
            for event in ["pre_read_code", "pre_run_command"] {
                let entry = json!({ "command": cmd, "show_output": true });
                replace_or_push(event_array(config, event)?, entry, is_guard);
            }
        }
    }
    Ok(())
}

fn nested(config: &mut Value, event: &str, matcher: &str, hook: Value) -> Result<(), String> {
    let group = json!({ "matcher": matcher, "hooks": [hook] });
    replace_or_push(event_array(config, event)?, group, group_has_guard);
    Ok(())
}

/// Remove every read-guard entry, flat or nested. Returns whether anything changed.
pub fn remove(config: &mut Value) -> bool {
    remove_marked(config, is_guard)
}

fn remove_marked(config: &mut Value, is_ours: fn(&Value) -> bool) -> bool {
    let Some(hooks) = config.get_mut("hooks").and_then(Value::as_object_mut) else {
        return false;
    };
    let mut changed = false;
    let mut emptied = Vec::new();
    for (event, items) in hooks.iter_mut() {
        let Some(items) = items.as_array_mut() else { continue };
        let before = items.len();
        let mut inner_changed = false;
        items.retain_mut(|item| {
            if is_ours(item) {
                return false;
            }
            if let Some(inner) = item.get_mut("hooks").and_then(Value::as_array_mut) {
                let n = inner.len();
                inner.retain(|h| !is_ours(h));
                if inner.len() != n {
                    inner_changed = true;
                    return !inner.is_empty();
                }
            }
            true
        });
        let event_changed = inner_changed || items.len() != before;
        changed |= event_changed;
        if event_changed && items.is_empty() {
            emptied.push(event.clone());
        }
    }
    for event in emptied {
        hooks.remove(&event);
    }
    changed
}

/// Add the Cursor `beforeSubmitPrompt` / `afterAgentResponse` / `stop` entries for
/// `ax turn-hook start|response|end`.
pub fn upsert_cursor_turn_hooks(config: &mut Value, bin: &str) -> Result<(), String> {
    if config.get("version").is_none() && config.is_object() {
        config["version"] = json!(1);
    }
    let bin = quote_bin(bin);
    for (event, phase) in [
        ("beforeSubmitPrompt", "start"),
        ("afterAgentResponse", "response"),
        ("stop", "end"),
    ] {
        let entry = json!({ "command": format!("{bin} {TURN_MARKER} {phase}"), "timeout": 10 });
        replace_or_push(event_array(config, event)?, entry, is_turn_hook);
    }
    Ok(())
}

/// Remove every `ax turn-hook` entry. Returns whether anything changed.
pub fn remove_turn_hooks(config: &mut Value) -> bool {
    remove_marked(config, is_turn_hook)
}

pub fn install_cursor_turn_hooks_file(path: &Path, bin: &str) -> Result<FileAction, String> {
    let existed = path.exists();
    let before = read_strict(path)?;
    let mut after = before.clone();
    upsert_cursor_turn_hooks(&mut after, bin)?;
    write(path, &before, &after, existed)
}

/// `Ok(None)` when there was nothing of ours to remove.
pub fn uninstall_turn_hooks_file(path: &Path) -> Result<Option<FileAction>, String> {
    uninstall_marked_file(path, remove_turn_hooks)
}

/// Missing file → `{}`; unreadable or invalid JSON → error (the file is left alone).
fn read_strict(path: &Path) -> Result<Value, String> {
    match fs::read_to_string(path) {
        Ok(raw) if raw.trim().is_empty() => Ok(json!({})),
        Ok(raw) => serde_json::from_str(&raw).map_err(|e| format!("{} is not valid JSON ({e}); left unchanged", path.display())),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(json!({})),
        Err(e) => Err(format!("cannot read {}: {e}", path.display())),
    }
}

fn write(path: &Path, before: &Value, after: &Value, existed: bool) -> Result<FileAction, String> {
    if existed && before == after {
        return Ok(FileAction::Unchanged);
    }
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let body = serde_json::to_string_pretty(after).map_err(|e| e.to_string())?;
    fs::write(path, body).map_err(|e| e.to_string())?;
    Ok(if existed { FileAction::Updated } else { FileAction::Created })
}

pub fn install_file(path: &Path, ide: GuardIde, bin: &str) -> Result<FileAction, String> {
    let existed = path.exists();
    let before = read_strict(path)?;
    let mut after = before.clone();
    upsert(&mut after, ide, bin)?;
    write(path, &before, &after, existed)
}

/// `Ok(None)` when there was nothing of ours to remove.
pub fn uninstall_file(path: &Path) -> Result<Option<FileAction>, String> {
    uninstall_marked_file(path, remove)
}

fn uninstall_marked_file(
    path: &Path,
    remove_ours: fn(&mut Value) -> bool,
) -> Result<Option<FileAction>, String> {
    if !path.exists() {
        return Ok(None);
    }
    let before = read_strict(path)?;
    let mut after = before.clone();
    if !remove_ours(&mut after) {
        return Ok(None);
    }
    write(path, &before, &after, true).map(Some)
}

#[cfg(test)]
mod tests {
    use super::*;

    const BIN: &str = "/opt/ax/bin/ax";

    fn temp_file(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("ax-guard-hooks-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir.join("hooks.json")
    }

    #[test]
    fn command_quotes_a_bin_path_with_spaces() {
        assert_eq!(guard_command(BIN, GuardIde::Cursor), "/opt/ax/bin/ax read-guard --ide cursor");
        assert_eq!(
            guard_command("/Users/me/Application Support/ax", GuardIde::Gemini),
            "\"/Users/me/Application Support/ax\" read-guard --ide gemini"
        );
    }

    #[test]
    fn cursor_entry_shape() {
        let mut c = json!({});
        upsert(&mut c, GuardIde::Cursor, BIN).unwrap();
        assert_eq!(
            c,
            json!({ "version": 1, "hooks": { "preToolUse": [
                { "command": "/opt/ax/bin/ax read-guard --ide cursor", "matcher": "Read|Grep|Shell", "timeout": 5 }
            ] } })
        );
    }

    #[test]
    fn nested_entry_shapes() {
        let mut claude = json!({});
        upsert(&mut claude, GuardIde::Claude, BIN).unwrap();
        assert_eq!(
            claude["hooks"]["PreToolUse"],
            json!([{ "matcher": "Read|Grep|Bash", "hooks": [
                { "type": "command", "command": "/opt/ax/bin/ax read-guard --ide claude", "timeout": 5 }
            ] }])
        );
        let mut codex = json!({});
        upsert(&mut codex, GuardIde::Codex, BIN).unwrap();
        assert_eq!(codex["hooks"]["PreToolUse"][0]["matcher"], "^Bash$");
        assert_eq!(codex["hooks"]["PreToolUse"][0]["hooks"][0]["command"], "/opt/ax/bin/ax read-guard --ide codex");
        let mut gemini = json!({});
        upsert(&mut gemini, GuardIde::Gemini, BIN).unwrap();
        assert_eq!(
            gemini["hooks"]["BeforeTool"],
            json!([{ "matcher": "read_file|search_file_content|grep|run_shell_command", "hooks": [
                { "name": "ax-read-guard", "type": "command", "command": "/opt/ax/bin/ax read-guard --ide gemini", "timeout": 5000 }
            ] }])
        );
    }

    #[test]
    fn windsurf_guards_reads_and_commands() {
        let mut c = json!({});
        upsert(&mut c, GuardIde::Windsurf, BIN).unwrap();
        let entry = json!([{ "command": "/opt/ax/bin/ax read-guard --ide windsurf", "show_output": true }]);
        assert_eq!(c["hooks"]["pre_read_code"], entry);
        assert_eq!(c["hooks"]["pre_run_command"], entry);
    }

    #[test]
    fn upsert_is_idempotent_and_replaces_a_moved_binary_in_place() {
        let other = json!({ "command": "./audit.sh", "matcher": "Write" });
        let mut c = json!({ "version": 1, "hooks": { "preToolUse": [
            { "command": "/old/ax read-guard --ide cursor", "matcher": "Read" },
            other.clone(),
            { "command": "/older/ax read-guard --ide cursor" }
        ] } });
        upsert(&mut c, GuardIde::Cursor, BIN).unwrap();
        let once = c.clone();
        upsert(&mut c, GuardIde::Cursor, BIN).unwrap();
        assert_eq!(c, once, "second install changes nothing");
        let items = c["hooks"]["preToolUse"].as_array().unwrap();
        assert_eq!(items.len(), 2, "duplicates collapse to one entry");
        assert_eq!(items[0]["command"], "/opt/ax/bin/ax read-guard --ide cursor");
        assert_eq!(items[1], other, "unrelated hooks keep their place");
    }

    #[test]
    fn upsert_preserves_unrelated_settings_and_groups() {
        let prompt = json!({ "hooks": [{ "type": "command", "command": "/opt/ax/bin/ax prompt-hook" }] });
        let mut c = json!({ "model": "opus", "hooks": { "UserPromptSubmit": [prompt.clone()], "PreToolUse": [
            { "matcher": "Write", "hooks": [{ "type": "command", "command": "lint" }] }
        ] } });
        upsert(&mut c, GuardIde::Claude, BIN).unwrap();
        assert_eq!(c["model"], "opus");
        assert_eq!(c["hooks"]["UserPromptSubmit"], json!([prompt]));
        assert_eq!(c["hooks"]["PreToolUse"].as_array().unwrap().len(), 2);
        assert_eq!(c["hooks"]["PreToolUse"][0]["matcher"], "Write");
    }

    #[test]
    fn upsert_rejects_hooks_of_the_wrong_type() {
        let mut c = json!({ "hooks": [] });
        assert!(upsert(&mut c, GuardIde::Claude, BIN).is_err());
        let mut c = json!({ "hooks": { "PreToolUse": {} } });
        assert!(upsert(&mut c, GuardIde::Claude, BIN).is_err());
        let mut c = json!([1, 2]);
        assert!(upsert(&mut c, GuardIde::Cursor, BIN).is_err());
    }

    #[test]
    fn remove_takes_only_read_guard_entries() {
        let mut c = json!({ "hooks": {
            "preToolUse": [{ "command": "/opt/ax/bin/ax read-guard --ide cursor" }, { "command": "./audit.sh" }],
            "PreToolUse": [
                { "matcher": "Read|Grep|Bash", "hooks": [{ "type": "command", "command": "ax read-guard --ide claude" }] },
                { "matcher": "*", "hooks": [
                    { "type": "command", "command": "ax read-guard --ide claude" },
                    { "type": "command", "command": "keep-me" }
                ] }
            ],
            "UserPromptSubmit": [{ "hooks": [{ "type": "command", "command": "ax prompt-hook" }] }],
            "pre_read_code": [{ "command": "ax read-guard --ide windsurf" }]
        } });
        assert!(remove(&mut c));
        assert_eq!(c["hooks"]["preToolUse"], json!([{ "command": "./audit.sh" }]));
        assert_eq!(
            c["hooks"]["PreToolUse"],
            json!([{ "matcher": "*", "hooks": [{ "type": "command", "command": "keep-me" }] }])
        );
        assert_eq!(c["hooks"]["UserPromptSubmit"][0]["hooks"][0]["command"], "ax prompt-hook");
        assert!(c["hooks"].get("pre_read_code").is_none(), "an event emptied by removal is dropped");
        assert!(!remove(&mut c), "second remove is a no-op");
    }

    #[test]
    fn remove_keeps_pre_existing_empty_events() {
        let mut c = json!({ "hooks": { "Stop": [], "preToolUse": [{ "command": "./audit.sh" }] } });
        assert!(!remove(&mut c));
        assert_eq!(c["hooks"]["Stop"], json!([]));
        let mut c = json!({ "hooks": {
            "PreToolUse": [{ "hooks": [{ "type": "command", "command": "ax read-guard --ide claude" }] }],
            "Stop": []
        } });
        assert!(remove(&mut c));
        assert!(c["hooks"].get("PreToolUse").is_none());
        assert_eq!(c["hooks"]["Stop"], json!([]), "an untouched empty event survives an earlier removal");
    }

    #[test]
    fn install_and_uninstall_files() {
        let path = temp_file("roundtrip");
        assert_eq!(install_file(&path, GuardIde::Cursor, BIN).unwrap(), FileAction::Created);
        assert_eq!(install_file(&path, GuardIde::Cursor, BIN).unwrap(), FileAction::Unchanged);
        assert_eq!(install_file(&path, GuardIde::Cursor, "/new/ax").unwrap(), FileAction::Updated);
        let on_disk: Value = serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(on_disk["hooks"]["preToolUse"][0]["command"], "/new/ax read-guard --ide cursor");
        assert_eq!(uninstall_file(&path).unwrap(), Some(FileAction::Updated));
        assert_eq!(uninstall_file(&path).unwrap(), None);
        let _ = fs::remove_dir_all(path.parent().unwrap());
    }

    #[test]
    fn invalid_json_is_never_rewritten() {
        let path = temp_file("invalid");
        fs::write(&path, "{ \"hooks\": { oops").unwrap();
        let err = install_file(&path, GuardIde::Claude, BIN).unwrap_err();
        assert!(err.contains("not valid JSON"), "{err}");
        assert!(uninstall_file(&path).is_err());
        assert_eq!(fs::read_to_string(&path).unwrap(), "{ \"hooks\": { oops");
        let _ = fs::remove_dir_all(path.parent().unwrap());
    }

    #[test]
    fn cursor_turn_hook_entry_shape() {
        let mut c = json!({});
        upsert_cursor_turn_hooks(&mut c, BIN).unwrap();
        assert_eq!(
            c,
            json!({ "version": 1, "hooks": {
                "beforeSubmitPrompt": [{ "command": "/opt/ax/bin/ax turn-hook start", "timeout": 10 }],
                "afterAgentResponse": [{ "command": "/opt/ax/bin/ax turn-hook response", "timeout": 10 }],
                "stop": [{ "command": "/opt/ax/bin/ax turn-hook end", "timeout": 10 }]
            } })
        );
        let mut spaced = json!({});
        upsert_cursor_turn_hooks(&mut spaced, "/Users/me/Application Support/ax").unwrap();
        assert_eq!(
            spaced["hooks"]["stop"][0]["command"],
            "\"/Users/me/Application Support/ax\" turn-hook end"
        );
    }

    #[test]
    fn cursor_turn_hooks_are_idempotent_and_keep_user_entries() {
        let session = json!({ "command": "./ax-session-model.sh" });
        let mut c = json!({ "version": 1, "hooks": {
            "beforeSubmitPrompt": [session.clone(), { "command": "/old/ax turn-hook start" }],
            "preToolUse": [{ "command": "/opt/ax/bin/ax read-guard --ide cursor" }]
        } });
        upsert_cursor_turn_hooks(&mut c, BIN).unwrap();
        let once = c.clone();
        upsert_cursor_turn_hooks(&mut c, BIN).unwrap();
        assert_eq!(c, once, "second install changes nothing");
        assert_eq!(
            c["hooks"]["beforeSubmitPrompt"],
            json!([session, { "command": "/opt/ax/bin/ax turn-hook start", "timeout": 10 }])
        );
        assert_eq!(c["hooks"]["stop"].as_array().unwrap().len(), 1);
        assert_eq!(c["hooks"]["preToolUse"].as_array().unwrap().len(), 1);
    }

    #[test]
    fn turn_hook_and_read_guard_removal_leave_each_other_alone() {
        let mut c = json!({});
        upsert(&mut c, GuardIde::Cursor, BIN).unwrap();
        upsert_cursor_turn_hooks(&mut c, BIN).unwrap();
        c["hooks"]["stop"]
            .as_array_mut()
            .unwrap()
            .push(json!({ "command": "./notify.sh" }));

        let mut guard_removed = c.clone();
        assert!(remove(&mut guard_removed));
        assert_eq!(
            guard_removed["hooks"]["beforeSubmitPrompt"],
            c["hooks"]["beforeSubmitPrompt"]
        );
        assert_eq!(guard_removed["hooks"]["stop"], c["hooks"]["stop"]);

        assert!(remove_turn_hooks(&mut c));
        assert!(c["hooks"].get("beforeSubmitPrompt").is_none());
        assert!(c["hooks"].get("afterAgentResponse").is_none());
        assert_eq!(c["hooks"]["stop"], json!([{ "command": "./notify.sh" }]));
        assert_eq!(c["hooks"]["preToolUse"].as_array().unwrap().len(), 1);
        assert!(!remove_turn_hooks(&mut c), "second remove is a no-op");
    }

    #[test]
    fn turn_hook_files_install_and_uninstall() {
        let path = temp_file("turn-roundtrip");
        assert_eq!(
            install_cursor_turn_hooks_file(&path, BIN).unwrap(),
            FileAction::Created
        );
        assert_eq!(
            install_cursor_turn_hooks_file(&path, BIN).unwrap(),
            FileAction::Unchanged
        );
        assert_eq!(
            uninstall_turn_hooks_file(&path).unwrap(),
            Some(FileAction::Updated)
        );
        assert_eq!(uninstall_turn_hooks_file(&path).unwrap(), None);
        fs::write(&path, "{ oops").unwrap();
        assert!(install_cursor_turn_hooks_file(&path, BIN).is_err());
        assert_eq!(fs::read_to_string(&path).unwrap(), "{ oops");
        let _ = fs::remove_dir_all(path.parent().unwrap());
    }

    #[test]
    fn uninstall_of_a_missing_file_is_a_no_op() {
        let path = temp_file("missing");
        assert_eq!(uninstall_file(&path).unwrap(), None);
        assert!(!path.exists());
        let _ = fs::remove_dir_all(path.parent().unwrap());
    }
}
