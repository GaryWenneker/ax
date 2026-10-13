"""Behavioral checks and negative controls for evaluation, without a provider call."""
import importlib.util
import unittest
from pathlib import Path

SPEC = importlib.util.spec_from_file_location("evaluate_ste_output", Path(__file__).with_name("evaluate-ste-output.py"))
evaluation = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(evaluation)


def measurements():
    return [{"task": task, "arm": arm, "turn": turn, "repetition": r,
             "text_tokens": 100 if arm == "standard" else 110, "metadata_tokens": 20,
             "reported_projection_tokens": 120, "wire_bytes": 500, "latency_ms": 1,
             "source_preserved": True, "model_called": False}
            for task in evaluation.TASKS for arm in evaluation.ARMS for turn in evaluation.TURNS for r in range(5)]


class EvaluationTests(unittest.TestCase):
    def test_projection_is_not_claimed_as_billed_usage(self):
        report = evaluation.summarize_measurements(measurements())
        self.assertEqual(report["rows"], 90)
        self.assertEqual(report["agent_quality"], "unverified")
        self.assertEqual(report["monthly_cost"], "unverified")
        self.assertEqual(report["comparisons"][0]["change_percent"], 8.333)

    def test_missing_pair_and_duplicate_fail_closed(self):
        data = measurements()
        for rows in [data[:-1], data + [data[0]], []]:
            with self.assertRaises(ValueError): evaluation.summarize_measurements(rows)

    def test_failed_source_or_nonfinite_measurement_is_rejected(self):
        for field, bad in [("source_preserved", False), ("model_called", True), ("text_tokens", -1), ("latency_ms", float("nan")), ("wire_bytes", True)]:
            rows = measurements()
            rows[0][field] = bad
            with self.assertRaises(ValueError): evaluation.summarize_measurements(rows)

    def test_real_usage_prices_cached_input_once(self):
        prices = {"model":"fixture", "verified_at":"2026-10-13", "source_url":"https://example.invalid/fixture-prices",
                  "currency":"EUR", "input_per_million":2, "cached_input_per_million":0.5, "output_per_million":4}
        runs = [{"model":"fixture", "task":task, "arm":arm, "repetition":r, "input_tokens":1000,
                 "cached_input_tokens":400, "output_tokens":100, "passed":True, "policy_violations":0, "recovery_rounds":0}
                for task in evaluation.TASKS for arm in evaluation.ARMS for r in range(5)]
        report = evaluation.summarize_agent_runs(runs, prices)
        self.assertEqual(report["quality"]["standard"]["passed"], 15)
        self.assertEqual(report["monthly_variable_cost_eur"]["standard"]["50"], 1.98)
        runs[0]["cached_input_tokens"] = 1001
        with self.assertRaises(ValueError): evaluation.summarize_agent_runs(runs, prices)


if __name__ == "__main__":
    unittest.main()
