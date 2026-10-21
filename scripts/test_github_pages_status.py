"""The newest GitHub Pages deployment is green only when its latest status is success."""

import json
import subprocess
import unittest
from pathlib import Path

SCRIPT = Path(__file__).with_name("github_pages_status.py")


def run_status(statuses: list[dict]) -> subprocess.CompletedProcess[str]:
    return subprocess.run(
        ["python3", str(SCRIPT), "--statuses-json", json.dumps(statuses)],
        check=False,
        capture_output=True,
        text=True,
    )


class GithubPagesStatusTests(unittest.TestCase):
    def test_success_is_green(self) -> None:
        result = run_status([{"state": "success"}])
        self.assertEqual(result.returncode, 0, result.stderr)

    def test_newest_failure_is_not_green(self) -> None:
        result = run_status([{"state": "failure"}, {"state": "success"}])
        self.assertEqual(result.returncode, 1, result.stdout)
        self.assertIn("failure", result.stderr)

    def test_in_progress_is_not_green(self) -> None:
        result = run_status([{"state": "in_progress"}])
        self.assertEqual(result.returncode, 1, result.stdout)

    def test_missing_status_is_not_green(self) -> None:
        result = run_status([])
        self.assertEqual(result.returncode, 1, result.stdout)
        self.assertIn("missing", result.stderr)


if __name__ == "__main__":
    unittest.main()
