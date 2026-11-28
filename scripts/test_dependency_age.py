"""Offline tests for the dependency minimum-age gate."""

import importlib.util
import json
import os
import re
import shutil
import subprocess
import sys
import tempfile
import unittest
from datetime import datetime, timedelta, timezone
from pathlib import Path
from unittest.mock import patch

SPEC = importlib.util.spec_from_file_location(
    "check_dependency_age", Path(__file__).with_name("check_dependency_age.py")
)
policy = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(policy)


class DependencyAgeTests(unittest.TestCase):
    def test_npm_nested_scoped_package_and_link(self):
        lock = {"packages": {
            "": {"name": "site"},
            "node_modules/@scope/pkg": {"version": "1.2.3"},
            "node_modules/other/node_modules/inner": {"version": "2.0.0"},
            "node_modules/local": {"link": True},
        }}
        self.assertEqual(
            policy.npm_packages(json.dumps(lock).encode()),
            {("@scope/pkg", "1.2.3"), ("inner", "2.0.0")},
        )

    def test_cargo_excludes_workspace_paths(self):
        lock = b"""[[package]]
name = "internal"
version = "1.0.0"
[[package]]
name = "external"
version = "2.1.0"
source = "registry+https://github.com/rust-lang/crates.io-index"
"""
        self.assertEqual(policy.cargo_packages(lock), {("external", "2.1.0")})

    def test_only_new_versions_are_checked(self):
        old = {("existing", "1.0.0"), ("old", "0.2.0")}
        new = {("existing", "1.0.0"), ("old", "0.3.0")}
        self.assertEqual(policy.added_versions(old, new), [("old", "0.3.0")])

    def test_seven_day_boundary_and_immature_release(self):
        now = datetime(2026, 10, 13, tzinfo=timezone.utc)
        def fake_published(_ecosystem, _name, version, _cache):
            return now - timedelta(days=7 if version == "old" else 6)
        with patch.object(policy, "published_at", side_effect=fake_published):
            failures = policy.evaluate(
                "npm", [("p", "old"), ("p", "recent")], now, {}
            )
        self.assertEqual(len(failures), 1)
        self.assertIn("recent", failures[0])

    def test_unverifiable_release_is_blocked(self):
        now = datetime.now(timezone.utc)
        failures = policy.evaluate("npm", [("!unsupported-source:pkg", "1.0.0")], now, {})
        self.assertEqual(len(failures), 1)
        self.assertIn("cannot be verified", failures[0])

    def test_latest_stable_php_skips_older_majors_and_release_candidates(self):
        payload = {
            "8": {"version": "8.5.11", "tags": ["security"]},
            "7": {"version": "7.4.33", "museum": False},
            "9": {"version": "9.0.0", "museum": True},
            "rc": {"version": "8.6.0", "tags": ["RC"]},
            "3": {"version": "3.0.x (latest)", "museum": True},
        }
        self.assertEqual(policy.latest_stable_php(payload), "8.5.11")

    def test_php_feed_without_a_stable_version_fails_closed(self):
        with self.assertRaises(policy.PolicyError):
            policy.latest_stable_php({"8": {"version": "8.6.0", "tags": ["RC"]}})

    def test_php_latest_published_today_is_required_not_delayed(self):
        now = datetime(2026, 10, 21, tzinfo=timezone.utc)

        def boom(*_args):
            raise AssertionError("PHP must not use the 7-day publication check")

        with patch.object(policy, "published_at", side_effect=boom):
            failures = policy.evaluate(
                "php", [("runtime", "8.5.12")], now, {"php-latest": "8.5.12"}
            )
        self.assertEqual(failures, [])

    def test_php_older_than_latest_is_blocked_even_after_seven_days(self):
        now = datetime(2026, 10, 21, tzinfo=timezone.utc)
        failures = policy.evaluate(
            "php", [("runtime", "8.5.11")], now, {"php-latest": "8.5.12"}
        )
        self.assertEqual(len(failures), 1)
        self.assertIn("8.5.12", failures[0])
        self.assertIn("7-day", failures[0])

    def test_php_minor_channel_of_latest_passes_and_older_pins_fail(self):
        now = datetime(2026, 10, 21, tzinfo=timezone.utc)
        failures = policy.evaluate(
            "php",
            [
                ("runtime", "8.5"),
                ("runtime", "8.5.11-cli-alpine"),
                ("runtime", "8.4"),
                ("runtime", "8.5.10-fpm-alpine"),
                ("runtime", "latest"),
            ],
            now,
            {"php-latest": "8.5.11"},
        )
        self.assertEqual(len(failures), 3)
        joined = "\n".join(failures)
        self.assertIn("8.4", joined)
        self.assertIn("8.5.10", joined)
        self.assertIn("latest", joined)

    def test_declared_pins_ignore_the_resolved_latest_output(self):
        text = """
        php-version: ${{ steps.php_runtime.outputs.version }}
        php-version: '8.4'
        FROM php:8.4.3-fpm-alpine3.21
        """
        self.assertEqual(
            policy.declared_php_runtimes(text),
            ["8.4", "8.4.3-fpm-alpine3.21"],
        )

    def test_php_scan_reads_workflow_and_dockerfile(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            workflow = root / ".github" / "workflows"
            workflow.mkdir(parents=True)
            (workflow / "ci.yml").write_text("php-version: '8.4'\n", encoding="utf-8")
            (root / "Dockerfile").write_text(
                "FROM php:8.5.11-cli-alpine\n", encoding="utf-8"
            )
            self.assertEqual(
                policy.php_runtime_pins(root),
                ["8.4", "8.5.11-cli-alpine"],
            )

    def test_cli_rejects_php_that_is_not_latest(self):
        script = Path(__file__).with_name("check_dependency_age.py")
        result = subprocess.run(
            [
                sys.executable,
                str(script),
                "--php-runtime",
                "8.4.26",
                "--php-latest",
                "8.5.11",
            ],
            capture_output=True,
            text=True,
            check=False,
        )
        self.assertEqual(result.returncode, 1)
        self.assertIn("8.5.11", result.stderr + result.stdout)

    def test_cli_accepts_the_latest_php_patch_and_its_minor_channel(self):
        script = Path(__file__).with_name("check_dependency_age.py")
        for version in ("8.5.11", "8.5"):
            result = subprocess.run(
                [
                    sys.executable,
                    str(script),
                    "--php-runtime",
                    version,
                    "--php-latest",
                    "8.5.11",
                ],
                capture_output=True,
                text=True,
                check=False,
            )
            self.assertEqual(result.returncode, 0, result.stderr + result.stdout)

    def test_php_runtime_guard_rejects_an_old_image_and_accepts_the_latest(self):
        guard = (
            Path(__file__).resolve().parents[1]
            / ".agents/skills/security-audit/scripts/php-runtime-guard.sh"
        )
        self.assertTrue(guard.is_file(), guard)
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            bindir = root / "bin"
            bindir.mkdir()
            (bindir / "python3").symlink_to(sys.executable)
            bash = shutil.which("bash")
            env = dict(os.environ)
            env["PHP_LATEST_OVERRIDE"] = "8.5.11"
            env["PATH"] = str(bindir)
            dockerfile = root / "Dockerfile"
            dockerfile.write_text("FROM php:8.2.34-cli\n", encoding="utf-8")
            rejected = subprocess.run(
                [bash, "-c", f'source "{guard}" && php_runtime_guard "{root}"'],
                capture_output=True,
                text=True,
                check=False,
                env=env,
            )
            self.assertEqual(rejected.returncode, 1, rejected.stdout + rejected.stderr)
            self.assertIn("8.5.11", rejected.stdout + rejected.stderr)
            dockerfile.write_text("FROM php:8.5.11-cli\n", encoding="utf-8")
            accepted = subprocess.run(
                [bash, "-c", f'source "{guard}" && php_runtime_guard "{root}"'],
                capture_output=True,
                text=True,
                check=False,
                env=env,
            )
            self.assertEqual(accepted.returncode, 0, accepted.stdout + accepted.stderr)

    def test_php_audit_scripts_call_the_runtime_guard(self):
        root = (
            Path(__file__).resolve().parents[1] / ".agents/skills/security-audit/scripts"
        )
        for relative in ("security-audit.sh", "scanners/php.sh"):
            text = (root / relative).read_text(encoding="utf-8")
            self.assertIn("php-runtime-guard.sh", text)
            self.assertIn("php_runtime_guard", text)


ROOT = Path(__file__).resolve().parents[1]


def tracked_npm_lockfiles() -> set[str]:
    out = subprocess.run(
        ["git", "ls-files", "-z", "--", "package-lock.json", "*/package-lock.json"],
        cwd=ROOT, capture_output=True, check=True,
    ).stdout.decode()
    return {p for p in out.split("\0") if p and "node_modules/" not in p}


def matrix_paths(workflow: str) -> set[str]:
    text = (ROOT / ".github" / "workflows" / workflow).read_text(encoding="utf-8")
    return set(re.findall(r"(?m)^\s+path:\s*(\S+)\s*$", text))


class LockfileCoverageTests(unittest.TestCase):
    """Every tracked npm lockfile is under every dependency policy."""

    def setUp(self):
        self.lockfiles = tracked_npm_lockfiles()
        self.dirs = {str(Path(p).parent) for p in self.lockfiles}
        self.assertTrue(self.lockfiles, "git ls-files found no npm lockfiles")

    def test_age_and_advisory_checks_cover_every_tracked_lockfile(self):
        self.assertEqual(set(policy.NPM_LOCKFILES), self.lockfiles)

    def test_monitor_and_autofix_matrices_cover_every_lockfile(self):
        for workflow in ("dependency-monitor.yml", "dependency-autofix.yml"):
            self.assertEqual(matrix_paths(workflow), self.dirs, workflow)

    def test_autofix_project_choices_cover_every_lockfile(self):
        sys.path.insert(0, str(Path(__file__).parent))
        import lcm_autofix
        self.assertEqual(set(lcm_autofix.PROJECTS), self.dirs)

    def test_dependabot_covers_every_lockfile(self):
        text = (ROOT / ".github" / "dependabot.yml").read_text(encoding="utf-8")
        npm_dirs = set(re.findall(
            r'package-ecosystem:\s*"npm"\s*\n\s*directory:\s*"/([^"]+)"', text
        ))
        self.assertEqual(npm_dirs, self.dirs)


if __name__ == "__main__":
    unittest.main()
