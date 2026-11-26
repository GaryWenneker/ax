#!/usr/bin/env python3
"""Manual mutation run for ax-admin and the remote MCP admin extension.

Each mutant is one plausible bug in a security or correctness check. The mutant
is applied, the named test is run, and the mutant counts as killed only when
cargo built the code and a test failed ("test result: FAILED" in the output).
A build error, a missing anchor or a passing test is a failure of this run.

    python3 tools/ax_admin_mutants.py             # run every mutant
    python3 tools/ax_admin_mutants.py --self-test # prove a survivor is reported
    python3 tools/ax_admin_mutants.py --only M07  # one mutant

Files are restored from their original bytes after every mutant, also when the
run is interrupted.
"""
import hashlib
import os
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
ADMIN = ["cargo", "test", "-p", "ax-admin", "--test", "admin"]
ADMIN_LIB = ["cargo", "test", "-p", "ax-admin", "--lib"]
REMOTE = ["cargo", "test", "-p", "ax-mcp", "--test", "remote_admin"]
REMOTE_LIB = ["cargo", "test", "-p", "ax-mcp", "--lib"]

SERVER = "crates/ax-admin/src/server.rs"
MCP = "crates/ax-admin/src/mcp.rs"
STORE = "crates/ax-admin/src/store.rs"
R_SERVER = "crates/ax-mcp/src/remote/server.rs"
R_AUTH = "crates/ax-mcp/src/remote/auth.rs"
R_CONFIG = "crates/ax-mcp/src/remote/config.rs"
USAGE = "crates/ax-usage/src/savings.rs"

# (id, what breaks, file, original text, mutated text, test command, test filter)
MUTANTS = [
    ("M01", "C2: any Host header is accepted", SERVER,
     "if !host.is_some_and(|host| hosts.iter().any(|allowed| allowed == host)) {",
     "if false {", ADMIN, "c2_"),
    ("M02", "C2: any Origin is accepted", SERVER,
     "        if !allowed {\n            return forbidden(\"Request origin is not allowed\");",
     "        if false {\n            return forbidden(\"Request origin is not allowed\");", ADMIN, "c2_"),
    ("M03", "C3: changes need no request token", SERVER,
     "if token.len() != app.csrf.len() || !bool::from(token.as_bytes().ct_eq(app.csrf.as_bytes())) {",
     "if false {", ADMIN, "c3_"),
    ("M04", "C1: listener bound to every interface", SERVER,
     "tokio::net::TcpListener::bind((\"127.0.0.1\", port))",
     "tokio::net::TcpListener::bind((\"0.0.0.0\", port))", ADMIN, "c1_"),
    ("M05", "A9: the key is written into the connection name", SERVER,
     "        id: uuid::Uuid::new_v4().to_string(),\n        name,\n        url: url.to_string(),\n        auth: AuthKind::ApiKey,",
     "        id: uuid::Uuid::new_v4().to_string(),\n        name: format!(\"{name} {key}\"),\n        url: url.to_string(),\n        auth: AuthKind::ApiKey,",
     ADMIN, "a9_"),
    ("M06", "A7: removing a connection keeps its key", SERVER,
     "            blocking(move || secrets.delete(&key_id)).await?;",
     "            let _ = (secrets, key_id);", ADMIN, "a7_"),
    ("M07", "A5: redirects are followed with the key", MCP,
     ".redirect(reqwest::redirect::Policy::none())",
     ".redirect(reqwest::redirect::Policy::limited(5))", ADMIN, "a5_"),
    ("M08", "B4: unlisted tools are sent to the server", MCP,
     "if !self.offers(name).await? {", "if false && !self.offers(name).await? {", ADMIN, "b4_"),
    ("M09", "B2: an expired session is not reopened", MCP,
     "Err(McpError::UnknownProject) if !retried_session => {",
     "Err(McpError::UnknownProject) if false && !retried_session => {", ADMIN, "b2_"),
    ("M10", "B3: an API key is retried like an OAuth token", MCP,
     "Err(McpError::Rejected) if !retried_token && self.tokens.refreshable() => {",
     "Err(McpError::Rejected) if !retried_token => {", ADMIN, "b3_"),
    ("M11", "B6: a streamed answer has no size limit", MCP,
     "if chunk.len() > self.limits.max_bytes.saturating_sub(bytes.len()) {",
     "if false {", ADMIN, "b6_"),
    ("M12", "E10: admin calls get the short timeout", MCP,
     "let timeout = self.limits.call_timeout(name);",
     "let timeout = self.limits.call;", ADMIN, "b6_"),
    ("M13", "A9: configuration file readable by others", STORE,
     "std::fs::Permissions::from_mode(0o600)", "std::fs::Permissions::from_mode(0o644)",
     ADMIN_LIB, "configuration_file_is_private"),
    ("M14", "E1: admin tools callable without the grant", R_SERVER,
     "    admin && ADMIN_TOOLS.contains(&name)\n", "    ADMIN_TOOLS.contains(&name)\n", REMOTE, "e1_"),
    ("M15", "E1: admin tools listed without the grant", R_SERVER,
     "            if admin {\n                // The local catalog hides heavy tools",
     "            if true {\n                // The local catalog hides heavy tools", REMOTE, "e1_"),
    # The admin scope is a second defence in front of these two, so they are
    # checked on the grant rule itself rather than through the HTTP tests.
    ("M16", "E1: a key with the admin scope needs no admin grant", R_AUTH,
     "                None => self.key_admin,", "                None => true,", REMOTE_LIB, "admin_grant"),
    ("M24", "E1: the admin scope is not required", R_AUTH,
     "            && self.scopes.contains(ADMIN_SCOPE)\n", "", REMOTE_LIB, "admin_grant"),
    ("M25", "E1: any write subject is an admin subject", R_AUTH,
     "                Some(sub) => project.admin_subjects.contains(sub),", "                Some(sub) => project.write_subjects.contains(sub),", REMOTE_LIB, "admin_grant"),
    ("M17", "E2: a key works on every project", R_AUTH,
     "                None => self.key_projects.contains(&project.id),", "                None => true,", REMOTE, "e2_"),
    ("M18", "E3: admin without write access is accepted", R_CONFIG,
     "if key.admin && !key.write {", "if false {", REMOTE, "e3_"),
    ("M19", "E3: admin subjects need no write access", R_CONFIG,
     "if p.admin_subjects.iter().any(|s| !p.write_subjects.contains(s)) {", "if false {", REMOTE, "e3_"),
    ("M20", "E5: a call may name another project path", R_SERVER,
     "        if args.get(\"projectPath\").is_some()\n            || args.get(\"project_path\").is_some()",
     "        if args.get(\"__never\").is_some()\n            || args.get(\"__never2\").is_some()", REMOTE, "e5_"),
    ("M21", "E7: the call log shows every project", USAGE,
     "WHERE project = ?1 AND (?2 IS NULL OR id < ?2)", "WHERE ?1 = ?1 AND (?2 IS NULL OR id < ?2)", REMOTE, "e7_"),
    ("M22", "E7: the call log page size is unbounded", USAGE,
     ".clamp(1, MCP_CALL_LOG_MAX_LIMIT)", ".clamp(1, 1_000_000)", REMOTE, "e7_"),
    ("M26", "a saved key is sent to a changed address without asking for it again", SERVER,
     "if url_changed && new_key.is_none() {", "if false {", ADMIN, "a_saved_key_is_never_sent"),
    ("M27", "a call for another connection than the active one is run", SERVER,
     "if connection.id != input.connection {", "if false {", ADMIN, "a_call_for_a_connection"),
    ("M28", "every policy action is available remotely", R_SERVER,
     ".is_some_and(|action| action.as_str() != Some(\"index\"))", ".is_some_and(|_| false)", REMOTE, "policy_reload"),
    ("M29", "ax:admin is advertised to every client", R_SERVER,
     "if !config.oauth.built_in && config.projects.iter().any(|p| !p.admin_subjects.is_empty()) {",
     "if true {", REMOTE, "the_admin_scope"),
    ("M30", "an index run is dropped when its request times out", R_SERVER,
     "let outcome = if let Some((lock, slot)) = mutation {",
     "let outcome = if let Some((lock, slot)) = mutation.filter(|_| false) {", REMOTE, "an_index_run"),
    ("M31", "index-changing calls have no slot limit", R_SERVER,
     "self.admin_slots = Arc::new(Semaphore::new(mutation_slots));",
     "self.admin_slots = Arc::new(Semaphore::new(mutation_slots + 4));", REMOTE, "index_changing_calls"),
    ("M32", "the old key is not restored when a save fails", SERVER,
     "                    Some(key) => secrets.set(&id, &key),", "                    Some(_) => Ok(()),",
     ADMIN, "a_failed_save"),
    ("M33", "memory changes although the file was not written", STORE,
     "        written?;\n        self.data = data;", "        self.data = data;\n        written?;",
     ADMIN_LIB, "a_failed_save_changes_nothing"),
    ("M34", "tokens of a sign-in cancelled too late are kept", SERVER,
     "        abandon(app.clone(), id, login.url);\n    }\n    ok(json!({\"cancelled\": true}))",
     "        let _ = login;\n    }\n    ok(json!({\"cancelled\": true}))", ADMIN, "a10_tokens"),
    ("M35", "a failed second sign-in signs the saved connection out", SERVER,
     "            Ok(true) => {}\n", "            Ok(true) => {\n                let _ = self.logins.logout(url).await;\n            }\n",
     ADMIN, "a10_tokens"),
    ("M36", "sign-in status saves the connection without the request token", SERVER,
     "        Some(LoginState::Connected) => ok(json!({\"state\": \"ready\"})),",
     "        Some(LoginState::Connected) => complete_login(State(app.clone()), Path(id.clone())).await,",
     ADMIN, "a10_browser"),
    ("M37", "the timeout after which an index run continues is not marked", R_SERVER,
     "\"running\":true", "\"running\":false", REMOTE, "an_index_run"),
    ("M38", "a finished sign-in that no page saved keeps its tokens", SERVER,
     "                    if app.pending.lock().await.remove(&id).is_some() {\n                        app.drop_unused_sign_in(&url).await;\n                    }",
     "                    app.pending.lock().await.remove(&id);",
     ADMIN, "a10_a_sign_in_the_page_never_saved"),
    ("M39", "a late-cancelled sign-in removes the tokens of a newer attempt", SERVER,
     "if self.pending.lock().await.values().any(|p| p.url == url) {", "if false {", ADMIN, "a10_tokens"),
    ("M40", "calls queue for minutes behind a running index", R_SERVER,
     "const PROJECT_WAIT: Duration = Duration::from_secs(2);",
     "const PROJECT_WAIT: Duration = Duration::from_secs(30);", REMOTE, "calls_do_not_queue"),
    ("M41", "a proxy's timeout page is taken for a still-running index", MCP,
     "504 if json.as_ref().is_some_and(|v| v[\"running\"] == true) => {", "504 => {", ADMIN, "the_servers_timeouts"),
    ("M23", "E2: admin tools lack the admin scope in discovery", R_SERVER,
     "vec![CONTEXT_SCOPE, WRITE_SCOPE, ADMIN_SCOPE]\n                } else if requires_write {",
     "vec![CONTEXT_SCOPE]\n                } else if requires_write {", REMOTE, "e2_"),
]

# Not a bug: a comment. The run must report it as a survivor.
EQUIVALENT = ("S00", "self-test: a comment change (must survive)", STORE,
              "/// `AX_ADMIN_HOME`, or `~/.ax-admin`.", "/// `AX_ADMIN_HOME`, or `~/.ax-admin` .",
              ADMIN_LIB, "configuration_file_is_private")


def digest(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def run_mutant(mutant) -> str:
    ident, what, rel, old, new, command, test_filter = mutant
    path = ROOT / rel
    original = path.read_bytes()
    text = original.decode("utf-8")
    if text.count(old) != 1:
        return f"BROKEN (anchor found {text.count(old)} times)"
    before = digest(path)
    try:
        path.write_bytes(text.replace(old, new).encode("utf-8"))
        env = dict(os.environ, AX_SKIP_WEB_BUILD="1")
        done = subprocess.run(command + [test_filter], cwd=ROOT, env=env,
                              stdout=subprocess.PIPE, stderr=subprocess.STDOUT, text=True, timeout=1800)
    finally:
        path.write_bytes(original)
    if digest(path) != before:
        return "BROKEN (file not restored)"
    out = done.stdout
    ran = [line for line in out.splitlines() if line.startswith("test result:")]
    if "error: could not compile" in out or "error[E" in out:
        return "BROKEN (does not compile)"
    if not ran:
        return "BROKEN (no test ran)"
    if not any(" 0 passed; 0 failed" not in line for line in ran):
        return "BROKEN (filter matched no test)"
    if done.returncode != 0 and any("FAILED" in line for line in ran):
        failed = [line.split()[1] for line in out.splitlines() if line.startswith("test ") and line.endswith("FAILED")]
        return "killed by " + ", ".join(sorted(set(failed)))
    return "SURVIVED"


def main() -> int:
    args = sys.argv[1:]
    if "--self-test" in args:
        result = run_mutant(EQUIVALENT)
        print(f"{EQUIVALENT[0]} {EQUIVALENT[1]}: {result}")
        if result != "SURVIVED":
            print("self-test FAILED: an equivalent mutant was not reported as a survivor")
            return 1
        print("self-test ok: a surviving mutant is reported")
        return 0
    selected = MUTANTS
    if "--only" in args:
        wanted = args[args.index("--only") + 1].split(",")
        selected = [m for m in MUTANTS if m[0] in wanted]
        if len(selected) != len(wanted):
            print(f"unknown mutant in {wanted}")
            return 1
    killed = 0
    for mutant in selected:
        result = run_mutant(mutant)
        print(f"{mutant[0]} {mutant[1]}: {result}", flush=True)
        killed += result.startswith("killed")
    print(f"{killed}/{len(selected)} mutants killed")
    return 0 if killed == len(selected) else 1


if __name__ == "__main__":
    sys.exit(main())
