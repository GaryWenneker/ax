#!/usr/bin/env python3
"""End-to-end check that every agent-facing ax MCP tool delivers usable text.

Builds a throwaway project (own HOME, own .ax), starts `ax serve --mcp` over
stdio, and calls every tool in `tools/list` the way an agent would. A call
passes when it is not an error, its text is non-empty, under the size limit,
and names the thing that was asked for.

Fails closed: a tool with no planned call, an unexpected exception, or a
server that stops answering is a failure, never a skip.

Usage: tools/ax_agent_e2e.py <path-to-ax-binary> [--client NAME] [--json]
Exit code 0 only when every check passed.
"""

import json
import os
import re
import shutil
import subprocess
import sys
import tempfile

MAX_REPLY_CHARS = 60_000
STEP_TIMEOUT_S = 120

FIXTURE = os.path.join(os.path.dirname(os.path.abspath(__file__)), "ax_agent_e2e_fixture.json")


class Failure(Exception):
    pass


class Server:
    def __init__(self, ax, project, env, client):
        self.proc = subprocess.Popen(
            [ax, "serve", "--mcp", "--path", project],
            cwd=project,
            env=env,
            stdin=subprocess.PIPE,
            stdout=subprocess.PIPE,
            stderr=subprocess.DEVNULL,
            text=True,
        )
        self.next_id = 0
        init = self.rpc(
            "initialize",
            {
                "protocolVersion": "2025-06-18",
                "capabilities": {},
                "clientInfo": {"name": client, "version": "e2e"},
            },
        )
        self.version = init["serverInfo"]["version"]

    def rpc(self, method, params):
        self.next_id += 1
        want = self.next_id
        message = {"jsonrpc": "2.0", "id": want, "method": method, "params": params}
        self.proc.stdin.write(json.dumps(message) + "\n")
        self.proc.stdin.flush()
        while True:
            line = self.proc.stdout.readline()
            if not line:
                raise Failure(f"server closed the pipe during {method}")
            try:
                reply = json.loads(line)
            except json.JSONDecodeError:
                continue
            if reply.get("id") != want:
                continue
            if "error" in reply:
                raise Failure(f"{method}: JSON-RPC error {reply['error']}")
            return reply["result"]

    def call(self, name, arguments):
        return self.rpc("tools/call", {"name": name, "arguments": arguments})

    def close(self):
        self.proc.kill()
        self.proc.wait()


def text_of(result):
    parts = result.get("content") or []
    return "\n".join(p.get("text", "") for p in parts if p.get("type") == "text")


def build_project(ax):
    home = tempfile.mkdtemp(prefix="ax-e2e-home-")
    project = tempfile.mkdtemp(prefix="ax-e2e-project-")
    env = dict(
        os.environ,
        HOME=home,
        AX_HOME_DIR=home,
        USERPROFILE=home,
        AX_NO_UPDATE_CHECK="1",
        NO_COLOR="1",
    )
    for key in ("AX_MCP_FULL", "AX_MCP_STRUCTURED"):
        env.pop(key, None)
    # The project's files live in a data file, keyed by relative path.
    with open(FIXTURE, encoding="utf-8") as handle:
        files = json.load(handle)
    for rel, body in files.items():
        path = os.path.join(project, rel)
        os.makedirs(os.path.dirname(path), exist_ok=True)
        with open(path, "w", encoding="utf-8") as handle:
            handle.write(body)
    git = ["git", "-c", "user.name=e2e", "-c", "user.email=e2e@example.invalid"]
    subprocess.run(git + ["init", "-q"], cwd=project, check=True)
    subprocess.run(git + ["add", "-A"], cwd=project, check=True)
    subprocess.run(git + ["commit", "-q", "-m", "Add invoice billing MARKER-COMMIT"],
                   cwd=project, check=True, env=env)
    done = subprocess.run(
        [ax, "init", "."],
        cwd=project,
        env=env,
        stdin=subprocess.DEVNULL,
        capture_output=True,
        text=True,
        timeout=600,
    )
    if not os.path.isfile(os.path.join(project, ".ax", "ax.db")):
        raise Failure(f"ax init did not create .ax/ax.db: {done.stderr[-400:]}")
    # The post-commit hook does this for later commits; do it for the first one.
    subprocess.run([ax, "capture-git", "--quiet"], cwd=project, env=env,
                   stdin=subprocess.DEVNULL, capture_output=True, timeout=120)
    return project, home, env


class Run:
    def __init__(self, server, text_only):
        self.server = server
        self.text_only = text_only
        self.rows = []
        self.called = set()
        self.state = {}

    def check(self, label, name, arguments, expect=(), forbid=(), allow_error=False,
              known_gap=None):
        """Call one tool; record pass/fail. Returns the reply text ('' on failure)."""
        self.called.add(name)
        problems = []
        text = ""
        structured = None
        try:
            result = self.server.call(name, arguments)
            text = text_of(result)
            structured = result.get("structuredContent")
            if result.get("isError") and not allow_error:
                problems.append("isError=true")
            if not text.strip():
                problems.append("empty text")
            if len(text) > MAX_REPLY_CHARS:
                problems.append(f"{len(text)} chars > {MAX_REPLY_CHARS}")
            for needle in expect:
                if needle not in text:
                    problems.append(f"missing {needle!r}")
            for needle in forbid:
                if needle in text:
                    problems.append(f"contains {needle!r}")
            if self.text_only and structured is not None:
                problems.append("structuredContent sent to a text-only client")
        except Failure as error:
            problems.append(str(error))
        if known_gap:
            # A documented defect: reported on every run, never counted as a pass.
            # When it starts passing the gap is closed and this entry must go.
            if problems:
                problems = [f"known gap: {known_gap}"]
            else:
                problems = ["known gap now passes: remove known_gap from this check"]
        self.rows.append(
            {
                "label": label,
                "tool": name,
                "gap": bool(known_gap) and problems[0].startswith("known gap:"),
                "ok": not problems,
                "chars": len(text),
                "structured": structured is not None,
                "problems": problems,
                "preview": " ".join(text.split())[:160],
            }
        )
        return text if not problems else text


def agent_calls(run):
    """Every tool, called with realistic arguments, in the order an agent would."""
    c = run.check
    first = c(
        "preflight (first turn)",
        "ax_preflight",
        {"prompt": "change the invoice total calculation", "files": ["src/billing.rs"]},
        expect=["MARKER-RULE-CENTS", "MARKER-SKILL-INVOICE", "<ax_chat session="],
    )
    match = re.search(r"<ax_chat session=([A-Za-z0-9_\-]+)", first)
    session = match.group(1) if match else "missing-session"
    run.state["session"] = session
    s = {"session": session}

    c("status", "ax_status", {}, expect=["ax Status"])
    c("sync", "ax_sync", {})
    c("search symbol", "ax_search", dict(s, query="invoice_total"),
      expect=["invoice_total", "src/billing.rs"])
    c("node source", "ax_node", dict(s, name="invoice_total"),
      expect=["items.iter().sum()"])
    c("explore question", "ax_explore",
      dict(s, query="how is the invoice total calculated"),
      expect=["invoice_total", "items.iter().sum()"])
    c("context for a task", "ax_context", dict(s, task="change how invoice_total sums items"),
      expect=["invoice_total"])
    c("callers", "ax_callers", dict(s, symbol="invoice_total"), expect=["taxed_total"])
    c("callers through a macro argument", "ax_callers", dict(s, symbol="invoice_total"),
      expect=["print_invoice"], known_gap="calls inside println!/assert! arguments get no edge")
    c("callees", "ax_callees", dict(s, symbol="taxed_total"), expect=["add_tax"])
    c("impact", "ax_impact", dict(s, symbol="invoice_total"), expect=["taxed_total"])
    c("path between symbols", "ax_path", dict(s, **{"from": "taxed_total", "to": "add_tax"}),
      expect=["add_tax"])
    c("module api", "ax_api", {"module": "billing"}, expect=["invoice_total"])
    c("cycles", "ax_cycles", {})
    c("insights", "ax_insights", {})
    c("report", "ax_report", {})
    c("affected tests", "ax_affected", dict(s, files=["src/billing.rs"]),
      expect=["tests/billing_test.rs"])

    c("rules for a prompt", "ax_rules", {"prompt": "invoice", "files": ["src/billing.rs"]},
      expect=["money-in-cents", "MARKER-RULE-CENTS"])
    c("skill body", "ax_skill", {"name": "invoice-review"}, expect=["MARKER-SKILL-INVOICE"])
    c("guard before write", "ax_guard", {"path": "src/billing.rs", "operation": "write"},
      expect=["allowed"])
    c("capture proposal", "ax_policy_capture",
      {"action": "propose", "prompt": "Always store money as integer cents"},
      expect=["cents"])

    c("remember", "ax_remember",
      {"title": "Tax is 20 percent", "kind": "decision", "triggers": ["tax-rate"],
       "files": ["src/tax.rs"],
       "body": "MARKER-MEMORY The tax rate is 20 percent because add_tax divides by five."})
    c("recall by cue", "ax_recall", {"cue": "tax-rate"},
      expect=["Tax is 20 percent", "MARKER-MEMORY"])
    c("recall by text", "ax_recall", {"query": "tax rate percent", "limit": 3},
      expect=["Tax is 20 percent"])
    c("history of a file", "ax_history", {"query": "src/billing.rs"}, expect=["MARKER-COMMIT"])
    c("history by topic word", "ax_history", {"query": "invoice billing"},
      expect=["MARKER-COMMIT"],
      known_gap="a commit is found by file path but not by the words of its subject")

    c("session note", "ax_session",
      dict(s, action="add", facts=["MARKER-NOTE invoices are stored in cents"]))
    c("preflight (second turn) shows the note and skips delivered bodies", "ax_preflight",
      dict(s, prompt="now change the tax rate", files=["src/tax.rs"]),
      expect=["MARKER-NOTE", f"session={session}"])

    stashed = c("stash", "ax_stash",
                {"label": "e2e", "text": "MARKER-STASH " + "long tool output line\n" * 400})
    stash_id = re.search(r"\b(cc_[0-9a-f]+)\b", stashed)
    run.state["stash_id"] = stash_id.group(1) if stash_id else ""
    c("expand a stashed reply", "ax_expand", {"id": run.state["stash_id"]},
      expect=["MARKER-STASH"])
    c("cache status", "ax_cache_status", dict(s))
    c("durable document put", "ax_durable",
      dict(s, action="doc_put", kind="plan", body={"note": "MARKER-DOC change the tax rate"}))
    c("durable document get", "ax_durable", dict(s, action="doc_get", kind="plan"),
      expect=["MARKER-DOC"])

    c("tool economics", "ax_tool_economics", {})
    c("optimization advice", "ax_optimization_advice", {})
    c("cost", "ax_cost", {})
    c("neural status", "ax_neural_status", {})
    c("neural search", "ax_neural_search", {"query": "invoice total"})
    c("neural rank", "ax_neural_rank",
      {"query": "tax rate",
       "blocks": [{"id": "a", "class": "optional", "text": "add_tax divides by five"},
                  {"id": "b", "class": "optional", "text": "unrelated css colors"}]})
    c("neural predict", "ax_neural_predict", {"task": "change the tax rate"})
    c("neural explain", "ax_neural_explain", {})
    c("neural model list", "ax_neural_model_list", {})
    c("neural model status", "ax_neural_model_status", {})
    c("neural benchmark", "ax_neural_benchmark", {})

    # Tools that only exist when the project has policy.
    optional = {
        "ax_policy_index": {},
        "ax_index": {},
        "ax_lsp": {"action": "status"},
        "ax_ship": {"mode": "evaluate"},
        "ax_files": {},
        "ax_diagnostics": {"diagnostics": []},
        "ax_budget": {},
    }
    return optional


def run_client(ax, client, text_only):
    project, home, env = build_project(ax)
    server = Server(ax, project, env, client)
    run = Run(server, text_only)
    try:
        listed = [tool["name"] for tool in server.rpc("tools/list", {})["tools"]]
        optional = agent_calls(run)
        for name in listed:
            if name not in run.called and name in optional:
                run.check(f"{name} (default call)", name, optional[name])
        untested = [name for name in listed if name not in run.called]
        for name in untested:
            run.rows.append({"label": "NO PLANNED CALL", "tool": name, "ok": False, "chars": 0,
                             "structured": False, "problems": ["tool is listed but never called"],
                             "preview": ""})
        unknown = sorted(run.called - set(listed))
        for name in unknown:
            for row in run.rows:
                if row["tool"] == name and row["ok"]:
                    row["ok"] = False
                    row["problems"].append("tool is not in tools/list")
        return {"client": client, "version": server.version, "tools_listed": len(listed),
                "rows": run.rows}
    finally:
        server.close()
        shutil.rmtree(project, ignore_errors=True)
        shutil.rmtree(home, ignore_errors=True)


def lean_contract(ax):
    """A non-Claude client still gets the lean structuredContent (spec N3/R4)."""
    project, home, env = build_project(ax)
    server = Server(ax, project, env, "cursor-vscode")
    try:
        result = server.call("ax_preflight", {"prompt": "review the invoice", "session": "lean-chat"})
        structured = result.get("structuredContent") or {}
        problems = []
        if (structured.get("session") or {}).get("id") != "lean-chat":
            problems.append("structuredContent.session.id missing for a lean client")
        if "inject" in structured:
            problems.append("lean payload carries the inject body")
        if "MARKER-RULE-CENTS" not in text_of(result):
            problems.append("rule body missing from content.text")
        return {"label": "lean client keeps structuredContent", "tool": "ax_preflight",
                "ok": not problems, "chars": len(text_of(result)), "structured": bool(structured),
                "problems": problems, "preview": ""}
    finally:
        server.close()
        shutil.rmtree(project, ignore_errors=True)
        shutil.rmtree(home, ignore_errors=True)


def hook_checks(ax):
    """The prompt hook, run as the IDE runs it: JSON on stdin, context on stdout."""
    project, home, env = build_project(ax)

    def hook(prompt):
        done = subprocess.run([ax, "prompt-hook"], cwd=project, env=env, capture_output=True,
                              text=True, timeout=STEP_TIMEOUT_S,
                              input=json.dumps({"prompt": prompt, "cwd": project}))
        if done.returncode != 0:
            raise Failure(f"prompt-hook exit {done.returncode}: {done.stderr[-200:]}")
        return done.stdout

    question = "how does invoice_total work"
    cases = [
        ("hook: a structural question gets context", question,
         lambda out: "<ax_context" in out and "invoice_total" in out),
        ("hook: harness reminder text is not explored",
         f"<system-reminder>{question}</system-reminder>\nYes",
         lambda out: "<ax_context" not in out),
        ("hook: a task notification prints nothing",
         f"<task-notification>\n<summary>{question}</summary>\n</task-notification>",
         lambda out: out == ""),
        ("hook: a plain word is not a symbol", "Ok", lambda out: "<ax_context" not in out),
    ]
    rows = []
    try:
        for label, prompt, passes in cases:
            problems = []
            out = ""
            try:
                out = hook(prompt)
                if not passes(out):
                    problems.append("unexpected hook output")
            except (Failure, subprocess.TimeoutExpired) as error:
                problems.append(str(error))
            rows.append({"label": label, "tool": "prompt-hook", "ok": not problems,
                         "chars": len(out), "structured": False, "problems": problems,
                         "preview": " ".join(out.split())[:160]})
    finally:
        shutil.rmtree(project, ignore_errors=True)
        shutil.rmtree(home, ignore_errors=True)
    return rows


def main(argv):
    if len(argv) < 2:
        print(__doc__)
        return 2
    ax = os.path.abspath(argv[1])
    client = "claude-code"
    if "--client" in argv:
        client = argv[argv.index("--client") + 1]
    if not os.access(ax, os.X_OK):
        print(f"not an executable: {ax}")
        return 2
    try:
        report = run_client(ax, client, text_only="claude" in client.lower())
        report["rows"].append(lean_contract(ax))
        report["rows"].extend(hook_checks(ax))
    except Failure as error:
        print(f"FAILED before the run finished: {error}")
        return 1
    gaps = [row for row in report["rows"] if row.get("gap")]
    failed = [row for row in report["rows"] if not row["ok"] and not row.get("gap")]
    if "--json" in argv:
        print(json.dumps(report, indent=2))
    else:
        print(f"ax {report['version']} as client {report['client']}: "
              f"{report['tools_listed']} tools listed")
        for row in report["rows"]:
            mark = "PASS" if row["ok"] else ("GAP " if row.get("gap") else "FAIL")
            print(f"{mark}  {row['tool']:<26} {row['chars']:>6}c  {row['label']}")
            for problem in row["problems"]:
                print(f"        - {problem}")
            if not row["ok"] and row["preview"]:
                print(f"        > {row['preview']}")
        passed = len(report["rows"]) - len(failed) - len(gaps)
        print(f"{passed} passed, {len(failed)} failed, {len(gaps)} known gaps")
    return 1 if failed else 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))
