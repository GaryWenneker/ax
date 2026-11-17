#!/usr/bin/env bash
# Labeled delivery/abstention/size sweep against the live project.
# Uses `ax preflight --json` (and optional `ax recall --json`) — never opens
# `.ax/ax.db` with a raw sqlite handle.
# Usage: ./scripts/memory-delivery-live.sh [probes.json]
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
PROBES="${1:-$ROOT/benchmarks/memory/live-probes.json}"
OUT="${AX_MEMORY_DELIVERY_OUT:-$ROOT/docs/specs/bench-out/live-delivery.json}"
mkdir -p "$(dirname "$OUT")"
python3 - "$PROBES" "$OUT" "$ROOT" <<'PY'
import json, math, subprocess, sys
from pathlib import Path

probes_path, out_path, root = Path(sys.argv[1]), Path(sys.argv[2]), Path(sys.argv[3])
probes = json.loads(probes_path.read_text())


def approx_tokens(text: str) -> int:
    # Conservative: ~4 chars per token for English/Dutch prose + markup.
    return max(0, (len(text) + 3) // 4)


def titles_block(memories: list) -> str:
    if not memories:
        return ""
    close = "</ax_memories>\n"
    body = (
        '<ax_memories note="Project memories matched for this prompt. '
        'Titles only; call ax_recall with the id or title for a body.">\n'
    )
    for m in memories:
        mid = m.get("id") or ""
        kind = m.get("kind") or ""
        title = (m.get("title") or "").replace("\n", " ")
        entry = f"- {mid} [{kind}] {title}\n"
        if len(body) + len(entry) + len(close) > 6000:
            break
        body += entry
    body += close
    return body


def run_preflight(prompt: str) -> dict:
    raw = subprocess.check_output(
        ["ax", "preflight", "--json", "--prompt", prompt],
        cwd=str(root),
        text=True,
    )
    return json.loads(raw)


def run_recall(query: str, limit: int, depth: str | None) -> dict:
    cmd = ["ax", "recall", query, "--json", "--limit", str(limit)]
    if depth:
        cmd.extend(["--depth", depth])
    raw = subprocess.check_output(cmd, cwd=str(root), text=True)
    return json.loads(raw)


rows = []
for probe in probes["probes"]:
    channel = probe.get("channel") or "recall"
    expect = probe.get("expect")
    family = probe.get("family") or (
        "abstention" if expect == "empty" else "delivery"
    )
    limit = int(probe.get("limit", 5))
    memories = []
    ids = []
    block = ""
    raw_chars = 0
    if channel == "preflight":
        data = run_preflight(probe["query"])
        memories = data.get("memories") or []
        ids = [m.get("id") for m in memories if m.get("id")]
        block = titles_block(memories)
        raw_chars = len(json.dumps(data.get("memories") or []))
    else:
        data = run_recall(probe["query"], limit, probe.get("depth"))
        matches = data.get("matches") or []
        ids = []
        memories = []
        for m in matches:
            mem = m.get("memory") if isinstance(m.get("memory"), dict) else m
            mid = m.get("id") or (mem or {}).get("id")
            if mid:
                ids.append(mid)
            if isinstance(mem, dict):
                memories.append(mem)
        block = titles_block(memories[:3])
        raw_chars = len(json.dumps(data))

    top3 = ids[:3]
    if expect == "empty":
        ok = len(ids) == 0
        precision_at_3 = 1.0 if ok else 0.0
        delivered = False
    elif expect == "nonempty":
        ok = len(ids) > 0
        precision_at_3 = 1.0 if ok else 0.0
        delivered = ok
    elif isinstance(expect, list):
        want = set(expect)
        delivered = any(i in want for i in ids)
        ok = delivered
        if not top3:
            precision_at_3 = 0.0
        else:
            precision_at_3 = sum(1 for i in top3 if i in want) / len(top3)
    else:
        ok = False
        delivered = False
        precision_at_3 = 0.0

    rows.append(
        {
            "id": probe["id"],
            "query": probe["query"],
            "expect": expect,
            "family": family,
            "channel": channel,
            "ids": ids,
            "ok": ok,
            "delivered": delivered if family == "delivery" else None,
            "precision_at_3": round(precision_at_3, 4),
            "block_chars": len(block),
            "block_tokens_approx": approx_tokens(block),
            "raw_chars": raw_chars,
        }
    )
    print(
        f"{probe['id']}: ok={ok} n={len(ids)} tokens={approx_tokens(block)}",
        flush=True,
    )


def pct(n, d):
    return round(100.0 * n / d, 2) if d else None


def percentile(vals, p):
    if not vals:
        return None
    xs = sorted(vals)
    if len(xs) == 1:
        return xs[0]
    k = (len(xs) - 1) * (p / 100.0)
    f = math.floor(k)
    c = math.ceil(k)
    if f == c:
        return xs[int(k)]
    return xs[f] + (xs[c] - xs[f]) * (k - f)


delivery = [r for r in rows if r["family"] == "delivery"]
abstention = [r for r in rows if r["family"] == "abstention"]
del_ok = sum(1 for r in delivery if r["ok"])
abs_ok = sum(1 for r in abstention if r["ok"])
# Size gate: title-block tokens on prompts that delivered a memory block
size_vals = [r["block_tokens_approx"] for r in rows if r["block_chars"] > 0]
size_all = [r["block_tokens_approx"] for r in rows]
p95_tokens = percentile(size_vals, 95) if size_vals else percentile(size_all, 95)
prec_vals = [r["precision_at_3"] for r in delivery]
mean_p3 = round(sum(prec_vals) / len(prec_vals), 4) if prec_vals else None

delivery_rate = pct(del_ok, len(delivery))
abstention_rate = pct(abs_ok, len(abstention))
gates = {
    "delivery_ge_95": (delivery_rate or 0) >= 95.0,
    "abstention_ge_90": (abstention_rate or 0) >= 90.0,
    "block_tokens_p95_le_300": (p95_tokens or 0) <= 300,
    "precision_at_3_ge_0_8": (mean_p3 or 0) >= 0.8,
}

summary = {
    "probes": rows,
    "passed": sum(1 for r in rows if r["ok"]),
    "total": len(rows),
    "metrics": {
        "delivery_ok": del_ok,
        "delivery_total": len(delivery),
        "delivery_pct": delivery_rate,
        "abstention_ok": abs_ok,
        "abstention_total": len(abstention),
        "abstention_pct": abstention_rate,
        "block_tokens_p95": p95_tokens,
        "precision_at_3_mean": mean_p3,
        "gates": gates,
        "all_gates_pass": all(gates.values()),
    },
}
out_path.write_text(json.dumps(summary, indent=2) + "\n")
m = summary["metrics"]
print(
    f"wrote {out_path} ({summary['passed']}/{summary['total']}) "
    f"delivery={m['delivery_pct']}% abstention={m['abstention_pct']}% "
    f"p95_tokens={m['block_tokens_p95']} p@3={m['precision_at_3_mean']} "
    f"gates={m['all_gates_pass']}"
)
if not m["all_gates_pass"]:
    sys.exit(1)
PY
