"""Offline regression tests for safe automated npm fixes."""

import unittest
from unittest.mock import patch

import lcm_autofix as policy


def report(critical=0, high=0, moderate=0, low=0, info=0):
    counts = dict(critical=critical, high=high, moderate=moderate, low=low, info=info)
    return {"metadata": {"vulnerabilities": dict(counts, total=sum(counts.values()))}}


class AutoFixPolicyTests(unittest.TestCase):
    def test_safe_decrease_is_eligible(self):
        self.assertTrue(policy.check_candidate(
            "site", report(high=4), report(high=2), {"site/package-lock.json"}
        ))

    def test_mixed_severity_increase_is_not_safe(self):
        self.assertFalse(policy.check_candidate(
            "site", report(high=3), report(high=2, critical=1),
            {"site/package-lock.json"},
        ))

    def test_no_actual_improvement_is_skipped(self):
        self.assertFalse(policy.check_candidate(
            "telemetry-worker", report(high=2), report(high=2),
            {"telemetry-worker/package-lock.json"},
        ))

    def test_missing_metadata_fails_closed(self):
        with self.assertRaises(policy.RejectedCandidate):
            policy.check_candidate("site", {}, {}, {"site/package-lock.json"})

    def test_registry_error_fails_closed(self):
        with self.assertRaises(policy.RejectedCandidate):
            policy.severity_counts({"error": {"code": "E503"}})

    def test_invalid_counts_fail_closed(self):
        with self.assertRaises(policy.RejectedCandidate):
            policy.severity_counts(report(high=1) | {"metadata": {"vulnerabilities": {"total": 1, "high": True}}})

    def test_unexpected_manifest_change_rejected(self):
        with self.assertRaises(policy.RejectedCandidate):
            policy.check_candidate(
                "site", report(high=2), report(high=1),
                {"site/package-lock.json", "site/package.json"},
            )

    def test_no_change_does_not_propose_pr(self):
        self.assertFalse(policy.check_candidate("site", report(high=3), report(high=2), set()))

    def test_higher_risk_regression_cannot_pass(self):
        with self.assertRaises(AssertionError):
            self.assertTrue(policy.is_improvement(
                policy.severity_counts(report(critical=0, high=3)),
                policy.severity_counts(report(critical=1, high=2)),
            ))

    def test_diff_failure_is_not_ignored(self):
        with patch.object(policy.subprocess, "run") as fake_run:
            fake_run.return_value.returncode = 128
            fake_run.return_value.stderr = "git failed"
            with self.assertRaises(policy.RejectedCandidate):
                policy.tracked_changes()


if __name__ == "__main__":
    unittest.main()
