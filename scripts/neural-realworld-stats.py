#!/usr/bin/env python3
"""Paired bootstrap CI helpers for real-world Neural benchmark reports."""

from __future__ import annotations

import argparse
import json
import math
import random
import sys
from typing import Sequence


def paired_mean_delta(a: Sequence[float], b: Sequence[float]) -> float:
    if len(a) != len(b) or not a:
        raise ValueError("paired samples required")
    return sum(x - y for x, y in zip(a, b)) / len(a)


def bootstrap_ci(
    a: Sequence[float],
    b: Sequence[float],
    *,
    reps: int = 2000,
    alpha: float = 0.05,
    seed: int = 1,
) -> dict:
    if len(a) != len(b) or len(a) < 2:
        return {"kind": "not_measured", "reason": "need paired n>=2"}
    rng = random.Random(seed)
    n = len(a)
    deltas = [a[i] - b[i] for i in range(n)]
    boots = []
    for _ in range(reps):
        sample = [deltas[rng.randrange(n)] for _ in range(n)]
        boots.append(sum(sample) / n)
    boots.sort()
    lo = boots[int((alpha / 2) * reps)]
    hi = boots[int((1 - alpha / 2) * reps) - 1]
    mean = sum(deltas) / n
    # Cliff's delta approximation via dominance on paired diffs sign
    pos = sum(1 for d in deltas if d > 0)
    neg = sum(1 for d in deltas if d < 0)
    cliffs = (pos - neg) / n if n else 0.0
    return {
        "kind": "measured",
        "n": n,
        "meanDelta": mean,
        "ci95": [lo, hi],
        "cliffsDelta": cliffs,
        "reps": reps,
        "alpha": alpha,
    }


def wilcoxon_signs(a: Sequence[float], b: Sequence[float]) -> dict:
    """Sign-test style paired check (no SciPy). Reports signs only + two-sided binomial p."""
    if len(a) != len(b) or not a:
        return {"kind": "not_measured", "reason": "empty"}
    diffs = [x - y for x, y in zip(a, b) if x != y]
    if not diffs:
        return {"kind": "measured", "n": len(a), "nonzero": 0, "pApprox": 1.0}
    pos = sum(1 for d in diffs if d > 0)
    n = len(diffs)
    # two-sided binomial exact under p=0.5
    p = 0.0
    for k in range(0, n + 1):
        pk = math.comb(n, k) / (2**n)
        # accumulate tails as extreme as observed
        if abs(k / n - 0.5) >= abs(pos / n - 0.5):
            p += pk
    return {"kind": "measured", "n": len(a), "nonzero": n, "positive": pos, "pApprox": min(1.0, p)}


def self_test() -> int:
    a = [0.9, 0.8, 0.7, 0.85, 0.75]
    b = [0.5, 0.55, 0.6, 0.5, 0.58]
    ci = bootstrap_ci(a, b, reps=1000, seed=2)
    assert ci["kind"] == "measured"
    assert ci["meanDelta"] > 0
    assert ci["ci95"][0] < ci["meanDelta"] < ci["ci95"][1]
    # negative control: empty
    try:
        paired_mean_delta([], [])
        print("negative control failed", file=sys.stderr)
        return 2
    except ValueError:
        pass
    empty = bootstrap_ci([1.0], [0.0])
    assert empty["kind"] == "not_measured"
    print(json.dumps({"selfTest": "ok", "example": ci}, indent=2))
    return 0


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--self-test", action="store_true")
    parser.add_argument("--a", help="JSON array file of arm A metrics")
    parser.add_argument("--b", help="JSON array file of arm B metrics")
    opts = parser.parse_args()
    if opts.self_test:
        return self_test()
    if not opts.a or not opts.b:
        print("Usage: --self-test OR --a file --b file", file=sys.stderr)
        return 2
    a = json.loads(open(opts.a, encoding="utf-8").read())
    b = json.loads(open(opts.b, encoding="utf-8").read())
    print(json.dumps({"bootstrap": bootstrap_ci(a, b), "signs": wilcoxon_signs(a, b)}, indent=2))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
