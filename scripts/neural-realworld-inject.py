#!/usr/bin/env python3
"""Apply/revert RED injects for real-world benchmark tasks in a target tree."""
from __future__ import annotations
import argparse, json, shutil, sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
MANIFEST = ROOT / "docs/specs/ax-neural/injects/manifest.json"

def load():
    return json.loads(MANIFEST.read_text(encoding="utf-8"))

def apply_one(tree: Path, task_id: str, reverse: bool = False) -> None:
    man = load()
    spec = man["injects"].get(task_id)
    if not spec:
        raise SystemExit(f"no inject for {task_id}")
    path = tree / spec["file"]
    text = path.read_text(encoding="utf-8")
    frm, to = (spec["fixed"], spec["broken"]) if not reverse else (spec["broken"], spec["fixed"])
    if frm not in text:
        # already in desired state?
        if to in text:
            print(json.dumps({"taskId": task_id, "noop": True, "reverse": reverse}))
            return
        raise SystemExit(f"inject anchor missing in {path}")
    path.write_text(text.replace(frm, to, 1), encoding="utf-8")
    print(json.dumps({"taskId": task_id, "applied": True, "reverse": reverse, "file": spec["file"]}))

def main() -> int:
    p = argparse.ArgumentParser()
    p.add_argument("action", choices=["break", "fix", "list"])
    p.add_argument("--task", required=False)
    p.add_argument("--tree", default=str(ROOT))
    opts = p.parse_args()
    man = load()
    if opts.action == "list":
        print(json.dumps(sorted(man["injects"]), indent=2))
        return 0
    if not opts.task:
        raise SystemExit("--task required")
    apply_one(Path(opts.tree), opts.task, reverse=(opts.action == "fix"))
    return 0

if __name__ == "__main__":
    raise SystemExit(main())
