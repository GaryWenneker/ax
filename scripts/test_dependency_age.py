"""Offline tests for the dependency minimum-age gate."""

import importlib.util
import json
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


if __name__ == "__main__":
    unittest.main()
