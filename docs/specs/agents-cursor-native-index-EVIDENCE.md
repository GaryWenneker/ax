# EVIDENCE — Index and zip restore skip Cursor-native `.agents` files

**SPEC:** `/Users/gary/io/ax/docs/specs/agents-cursor-native-index.md`  
**spec approval:** C1–C4 approved 2026-09-02 (user: "approve"). C5–C6 autonomous (Windows `ax_preflight validation_failed` + `.scripts/wcag` missing from graph).  
**tier:** 2  
**command:** `bash /Users/gary/io/ax/tools/gauntlet-agents-git-share.sh` plus targeted `cargo test -p ax-extraction --lib scan_files_includes_scripts_dot_dir` and `cargo test -p ax-mcp --lib preflight_tool_schema_includes_project_path` / `string_array_accepts_single_string`  
**source:** working tree after C5–C6

## Mapping

| Behavior | Test |
|----------|------|
| C1 | `agents_share::tests::leak_gate_ignores_cursor_native_files` |
| C2 | `leak_gate_detects_disabled_and_private` |
| C3 | `index::tests::import_skips_unparseable_cursor_files` |
| C4 | `index::tests::restore_then_index_with_cursor_native_neighbor` |
| C5 | `index::tests::ensure_policy_ready_skips_cursor_files_when_checking_stale` |
| C6 | `orchestrator::tests::scan_files_includes_scripts_dot_dir` |

## Gauntlet (fresh run after last code edit)

```
cargo test -p ax-policy --lib  →  134 passed; 0 failed
ensure_policy_ready_skips_cursor_files_when_checking_stale → ok
scan_files_includes_scripts_dot_dir → ok
preflight_tool_schema_includes_project_path → ok (prompt not required)
string_array_accepts_single_string → ok
negative-control: leak test went red with defence removed
manual mutation: 5/5 killed
grep: skip rule; no parse_rule_file(...).map_err in stale check; INDEXED_DOT_DIRS
gauntlet-agents-git-share: ok
```

## Skipped

- Full `ax index` on a client project (Windows machine). User must install this binary, **MCP: Restart Servers**, then `ax index` (extraction version 7) so `.scripts/wcag` is walked.
- Independent verification: not performed (Tier 2).

## Known limits

Unparseable files are skipped with `eprintln`. `.scripts` is the only extra hidden directory indexed; other dot-dirs stay skipped.
