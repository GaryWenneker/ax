#!/usr/bin/env python3
"""Contract tests for the GitHub Actions workflows in .github/workflows.

Standard library only: the workflows use a small, regular YAML subset, so the
checks read the trigger list, job blocks and `uses:` lines by indentation.
"""

from __future__ import annotations

import re
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
WORKFLOWS = sorted((ROOT / ".github" / "workflows").glob("*.yml"))


def _top_level_block(text: str, key: str) -> list[str]:
    lines = text.splitlines()
    for i, line in enumerate(lines):
        if re.match(rf"^{re.escape(key)}:", line):
            block = [line]
            for nxt in lines[i + 1 :]:
                if nxt and not nxt.startswith((" ", "#")):
                    break
                block.append(nxt)
            return block
    return []


def triggers(text: str) -> set[str]:
    block = _top_level_block(text, "on")
    if not block:
        return set()
    inline = block[0].split(":", 1)[1].strip()
    if inline:
        return {t.strip() for t in inline.strip("[]").split(",") if t.strip()}
    return {
        m.group(1)
        for line in block[1:]
        if (m := re.match(r"^  ([A-Za-z_]+):", line))
    }


def jobs(text: str) -> dict[str, str]:
    block = _top_level_block(text, "jobs")
    result: dict[str, list[str]] = {}
    current = None
    for line in block[1:]:
        m = re.match(r"^  ([A-Za-z0-9_-]+):\s*$", line)
        if m:
            current = m.group(1)
            result[current] = []
        elif current:
            result[current].append(line)
    return {name: "\n".join(body) for name, body in result.items()}


def job_condition(body: str) -> str:
    lines = body.splitlines()
    for i, line in enumerate(lines):
        if re.match(r"^    if:", line):
            cond = [line.split(":", 1)[1]]
            for nxt in lines[i + 1 :]:
                if not nxt.startswith("      "):
                    break
                cond.append(nxt)
            return " ".join(cond)
    return ""


def job_needs(body: str) -> list[str]:
    for line in body.splitlines():
        m = re.match(r"^    needs:\s*(.+)$", line)
        if m:
            return [n.strip() for n in m.group(1).strip("[]").split(",") if n.strip()]
    return []


class WorkflowTriggers(unittest.TestCase):
    def test_event_conditions_name_a_declared_trigger(self) -> None:
        for path in WORKFLOWS:
            text = path.read_text(encoding="utf-8")
            declared = triggers(text)
            for event in re.findall(r"github\.event_name\s*==\s*'([a-z_]+)'", text):
                self.assertIn(event, declared, f"{path.name}: condition on '{event}' can never be true")
            if "github.event.workflow_run" in text:
                self.assertIn("workflow_run", declared, f"{path.name}: workflow_run fields without the trigger")


class PagesDeployment(unittest.TestCase):
    def test_pages_deploy_only_runs_from_main(self) -> None:
        for path in WORKFLOWS:
            all_jobs = jobs(path.read_text(encoding="utf-8"))

            def gated(name: str, seen: frozenset[str] = frozenset()) -> bool:
                if name in seen or name not in all_jobs:
                    return False
                body = all_jobs[name]
                if "refs/heads/main" in job_condition(body):
                    return True
                return any(gated(n, seen | {name}) for n in job_needs(body))

            for name, body in all_jobs.items():
                if "actions/deploy-pages" in body or re.search(r"name:\s*github-pages", body):
                    self.assertTrue(gated(name), f"{path.name}: job '{name}' deploys Pages from any branch")


class ActionPins(unittest.TestCase):
    USES = re.compile(r"^\s*-?\s*uses:\s*(\S+)(.*)$")

    def test_every_action_is_pinned_to_a_commit(self) -> None:
        for path in WORKFLOWS:
            for number, line in enumerate(path.read_text(encoding="utf-8").splitlines(), 1):
                m = self.USES.match(line)
                if not m or m.group(1).startswith(("./", "docker://")):
                    continue
                ref = m.group(1).rsplit("@", 1)
                where = f"{path.name}:{number} {m.group(1)}"
                self.assertEqual(len(ref), 2, f"{where}: no ref")
                self.assertRegex(ref[1], r"^[0-9a-f]{40}$", f"{where}: pin to a full commit SHA")
                self.assertRegex(m.group(2), r"#\s*\S+", f"{where}: name the version in a trailing comment")

    def test_dependabot_updates_pinned_actions(self) -> None:
        text = (ROOT / ".github" / "dependabot.yml").read_text(encoding="utf-8")
        self.assertIn('package-ecosystem: "github-actions"', text)


def run_blocks(text: str) -> list[tuple[int, str]]:
    """Return (line number, body) for every `run:` script in a workflow."""
    lines = text.splitlines()
    blocks = []
    for i, line in enumerate(lines):
        m = re.match(r"^(\s*)(?:-\s+)?run:\s*(.*)$", line)
        if not m:
            continue
        indent = len(m.group(1))
        body = [m.group(2)]
        for nxt in lines[i + 1 :]:
            if nxt.strip() and len(nxt) - len(nxt.lstrip()) <= indent:
                break
            body.append(nxt)
        blocks.append((i + 1, "\n".join(body)))
    return blocks


class ExpressionInjection(unittest.TestCase):
    # Values a dispatcher, issue author or pull request author controls.
    UNTRUSTED = re.compile(
        r"\$\{\{[^}]*\b(?:github\.event\.)?inputs\.|"
        r"\$\{\{[^}]*github\.event\.(?:issue|comment|review|pull_request|head_commit|commits)[^}]*"
        r"\.(?:title|body|message|name|label|ref)\b|"
        r"\$\{\{[^}]*github\.head_ref"
    )

    def test_untrusted_values_reach_scripts_only_through_env(self) -> None:
        for path in WORKFLOWS:
            for number, body in run_blocks(path.read_text(encoding="utf-8")):
                self.assertIsNone(
                    self.UNTRUSTED.search(body),
                    f"{path.name}:{number}: pass the value through env: and read it as an environment variable",
                )

    def test_detects_an_input_expanded_in_a_script(self) -> None:
        text = "jobs:\n  a:\n    steps:\n      - run: |\n          $tag = \"${{ inputs.tag }}\"\n"
        (_, body), = run_blocks(text)
        self.assertIsNotNone(self.UNTRUSTED.search(body))

    def test_chocolatey_tag_check_is_anchored(self) -> None:
        text = (ROOT / ".github" / "workflows" / "chocolatey.yml").read_text(encoding="utf-8")
        self.assertIn(r"'^v?\d+\.\d+\.\d+$'", text)


class Permissions(unittest.TestCase):
    def test_multi_job_workflows_grant_write_only_per_job(self) -> None:
        for path in WORKFLOWS:
            text = path.read_text(encoding="utf-8")
            if len(jobs(text)) < 2:
                continue
            top = "\n".join(_top_level_block(text, "permissions"))
            self.assertNotRegex(top, r":\s*write", f"{path.name}: move write permissions to the jobs that need them")


if __name__ == "__main__":
    unittest.main()
