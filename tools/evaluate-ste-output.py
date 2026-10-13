#!/usr/bin/env python3
"""Summarize real MCP measurements and, optionally, separately supplied agent runs.

Prices must be EUR per million tokens, dated and attributed by the caller.
MCP projection counts are never treated as provider-billed tokens.
"""
import argparse
import json
import math
import statistics
from pathlib import Path

ARMS = ("standard", "ste-inspired")
TASKS = ("simple", "access", "replication")
TURNS = ("cold", "warm", "reset")


def number(value: object, label: str, integer: bool = False) -> int | float:
    if isinstance(value, bool) or not isinstance(value, (int, float)):
        raise ValueError(f"{label}: expected a number")
    if not math.isfinite(value) or value < 0 or (integer and int(value) != value):
        raise ValueError(f"{label}: expected a finite nonnegative {'integer' if integer else 'number'}")
    return value


def read_rows(path: str | Path) -> list[dict[str, object]]:
    rows = [json.loads(line) for line in Path(path).read_text(encoding="utf-8").splitlines() if line.strip()]
    if not rows or any(not isinstance(row, dict) for row in rows):
        raise ValueError("expected nonempty JSON object rows")
    return rows


def summarize_measurements(rows: list[dict[str, object]]) -> dict[str, object]:
    seen = set()
    for row in rows:
        if row["task"] not in TASKS or row["arm"] not in ARMS or row["turn"] not in TURNS:
            raise ValueError("unknown task, arm or turn")
        repetition = number(row["repetition"], "repetition", True)
        key = (row["task"], row["arm"], repetition, row["turn"])
        if key in seen:
            raise ValueError("duplicate measurement")
        seen.add(key)
        for field in ("text_tokens", "metadata_tokens", "reported_projection_tokens", "wire_bytes"):
            number(row[field], field, True)
        number(row["latency_ms"], "latency_ms")
        if row["source_preserved"] is not True or row["model_called"] is not False:
            raise ValueError("not a successful model-free source-preservation measurement")
    for task in TASKS:
        repetitions = {key[2] for key in seen if key[0] == task}
        if len(repetitions) < 5:
            raise ValueError("at least five paired repetitions per task are required")
        for arm in ARMS:
            for repetition in repetitions:
                for turn in TURNS:
                    if (task, arm, repetition, turn) not in seen:
                        raise ValueError("incomplete paired measurements")
    result = []
    for task in TASKS:
        for turn in TURNS:
            medians = {}
            for arm in ARMS:
                group = [r for r in rows if (r["task"], r["turn"], r["arm"]) == (task, turn, arm)]
                medians[arm] = statistics.median(r["text_tokens"] + r["metadata_tokens"] for r in group)
            old, new = medians.values()
            result.append({"task": task, "turn": turn, "median_wire_projection_tokens": medians,
                           "change_percent": None if old == 0 else round((new - old) / old * 100, 3)})
    return {"measurement": "MCP text plus serialized metadata; not billed usage", "rows": len(rows),
            "comparisons": result, "agent_quality": "unverified", "monthly_cost": "unverified"}


def summarize_agent_runs(rows: list[dict[str, object]], prices: dict[str, object]) -> dict[str, object]:
    for key in ("model", "verified_at", "source_url"):
        if not isinstance(prices.get(key), str) or not prices[key]:
            raise ValueError(f"prices require {key}")
    if prices.get("currency") != "EUR":
        raise ValueError("prices must be EUR; caller must supply a dated conversion when needed")
    rates = [number(prices[k], k) for k in ("input_per_million", "cached_input_per_million", "output_per_million")]
    costs = {arm: [] for arm in ARMS}
    quality = {arm: {"passed": 0, "policy_violations": 0, "recovery_rounds": 0} for arm in ARMS}
    seen = set()
    for row in rows:
        arm, task = row["arm"], row["task"]
        if arm not in ARMS or task not in TASKS or row["model"] != prices["model"]:
            raise ValueError("agent run model, task or arm does not match the evaluation")
        repetition = number(row["repetition"], "repetition", True)
        key = (task, arm, repetition)
        if key in seen: raise ValueError("duplicate agent run")
        seen.add(key)
        total, cached, output = [number(row[k], k, True) for k in ("input_tokens", "cached_input_tokens", "output_tokens")]
        if cached > total: raise ValueError("cached input exceeds total input")
        if not isinstance(row["passed"], bool): raise ValueError("passed must be a boolean")
        costs[arm].append(((total - cached) * rates[0] + cached * rates[1] + output * rates[2]) / 1_000_000)
        quality[arm]["passed"] += int(row["passed"])
        for field in ("policy_violations", "recovery_rounds"):
            quality[arm][field] += number(row[field], field, True)
    for task in TASKS:
        repetitions = {key[2] for key in seen if key[0] == task}
        if len(repetitions) < 5 or any((task, arm, r) not in seen for arm in ARMS for r in repetitions):
            raise ValueError("at least five paired real agent runs per task are required")
    return {"prices": prices, "quality": quality, "monthly_variable_cost_eur": {
        arm: {str(cycles): round(statistics.mean(costs[arm]) * cycles * 22, 4) for cycles in (50, 100, 200, 500)}
        for arm in ARMS}, "fixed_fees": "excluded; add applicable subscription and hosting charges"}


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--measurements", required=True)
    parser.add_argument("--agent-runs")
    parser.add_argument("--prices")
    args = parser.parse_args()
    if bool(args.agent_runs) != bool(args.prices):
        parser.error("--agent-runs and --prices must be supplied together")
    try:
        result = summarize_measurements(read_rows(args.measurements))
        if args.agent_runs:
            result["real_agent_evaluation"] = summarize_agent_runs(read_rows(args.agent_runs), json.loads(Path(args.prices).read_text(encoding="utf-8")))
    except (OSError, ValueError, KeyError, TypeError) as error:
        parser.exit(1, f"evaluation failed: {error}\n")
    print(json.dumps(result, indent=2, allow_nan=False))


if __name__ == "__main__":
    main()
