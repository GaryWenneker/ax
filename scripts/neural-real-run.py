#!/usr/bin/env python3
"""Real execution of `ax neural` and a CLI preflight in a new temporary project.

usage: neural-real-run.py <ax binary>

Runs the built binary, not a test harness, with an empty HOME and no AX_* variables: init,
feedback (a secret-shaped context id is refused), predict, preflight (the inject has the Predicted
block and contextBudget.neural is present), status (token delta and ranking time measured), export
from a subdirectory (no project path, no execution ids in the file), evaluate, benchmark (temporary
database), share, and the model commands (promote without --explicit is refused; promote, rollback,
model-status, and models agree). Fails closed: a failed check, an unexpected exit code, output
that is not JSON, or a timeout exits 1.
"""
import json
import os
import pathlib
import subprocess
import sys
import tempfile

EXECUTIONS = ("exec-real-run-1", "exec-real-run-2")
NEURAL = {"prediction": {"enabled": True}, "learning": {"feedback_collection": True}}


def check(condition: bool, message: str) -> None:
    if not condition:
        raise AssertionError(message)


class Ax:
    def __init__(self, binary: pathlib.Path, home: pathlib.Path, project: pathlib.Path):
        self.binary = binary
        self.project = project
        self.env = {key: value for key, value in os.environ.items() if not key.startswith("AX_")}
        self.env["HOME"] = str(home)

    def run(self, *args: str, cwd: pathlib.Path | None = None, want: int = 0) -> str:
        result = subprocess.run(
            [str(self.binary), *args],
            cwd=cwd or self.project,
            env=self.env,
            capture_output=True,
            text=True,
            timeout=300,
        )
        output = result.stdout + result.stderr
        check(
            result.returncode == want,
            f"ax {' '.join(args)}: exit {result.returncode}, wanted {want}\n{output[-3000:]}",
        )
        print(f"ok   ax {' '.join(args)} (exit {want})")
        return output if want else result.stdout

    def json(self, *args: str, cwd: pathlib.Path | None = None) -> dict:
        text = self.run(*args, cwd=cwd)
        try:
            return json.loads(text)
        except json.JSONDecodeError as error:
            raise AssertionError(f"ax {' '.join(args)}: not JSON ({error})\n{text[-3000:]}")


def configure(project: pathlib.Path) -> None:
    path = project / "ax.json"
    config = json.loads(path.read_text(encoding="utf-8")) if path.exists() else {}
    config["neural"] = NEURAL
    path.write_text(json.dumps(config, indent=2), encoding="utf-8")


def feedback_and_predict(ax: Ax) -> None:
    for execution in EXECUTIONS:
        stored = ax.json(
            "neural", "feedback", execution, "parser", "success",
            "--context-id", "memory:parser-lines", "--skill-id", "parse-errors",
            "--rule-id", "english-only", "--input-tokens", "300", "--output-tokens", "40",
        )
        check(stored.get("stored") is True, f"feedback not stored: {stored}")
    refused = ax.run(
        "neural", "feedback", "exec-real-run-3", "parser", "success",
        "--context-id", "password=hunter2", want=1,
    )
    check("secret-shaped text is excluded from feedback" in refused, refused[-2000:])
    predicted = ax.json("neural", "predict", "fix the parser")
    items = predicted.get("predictions", [])
    check(predicted.get("label") == "predicted" and items, f"no predictions: {predicted}")
    check(all(item.get("label") == "predicted" for item in items), f"unlabeled: {items}")
    kinds = {item.get("kind") for item in items}
    check({"memory", "rule", "skill"} <= kinds, f"prediction kinds {kinds}")


def preflight_and_status(ax: Ax) -> None:
    reply = ax.json("preflight", "--path", str(ax.project), "--prompt", "fix the parser", "--json")
    text = json.dumps(reply)
    check("## Predicted" in text, "the preflight inject has no Predicted block")
    check('"neural"' in json.dumps(find(reply, "contextBudget")), "contextBudget.neural missing")
    status = ax.json("neural", "status")
    metrics = status.get("metrics", {})
    delta, latency = metrics.get("tokenDelta", {}), metrics.get("rankLatency", {})
    check(delta.get("kind") == "measured" and delta.get("preflights", 0) >= 1, f"tokenDelta {delta}")
    check(latency.get("kind") == "measured", f"rankLatency {latency}")
    check(metrics.get("feedbackCount", {}).get("value") == 2, f"feedbackCount {metrics}")


def find(value, key: str):
    """The first value stored under `key` anywhere in a JSON tree."""
    if isinstance(value, dict):
        if key in value:
            return value[key]
        children = value.values()
    elif isinstance(value, list):
        children = value
    else:
        return None
    for child in children:
        found = find(child, key)
        if found is not None:
            return found
    return None


def export_evaluate_benchmark(ax: Ax) -> None:
    src = ax.project / "src"
    summary = ax.json("neural", "export", "--out", "rows.json", cwd=src)
    check(summary.get("exported") == 2, f"export summary {summary}")
    check(summary.get("projectPathIncluded") is False, f"export summary {summary}")
    rows = (src / "rows.json").read_text(encoding="utf-8")
    for private in (str(ax.project), ax.project.name, *EXECUTIONS, "hunter2"):
        at = rows.find(private)
        check(at < 0, f"the export contains {private!r}: ...{rows[max(0, at - 80):at + 80]}...")
    evaluation = ax.json("neural", "evaluate")
    check(evaluation.get("kind") == "measured", f"evaluate {evaluation}")
    accuracy = ax.json("neural", "status").get("metrics", {}).get("predictionAccuracy", {})
    check(accuracy.get("kind") == "measured", f"predictionAccuracy {accuracy}")
    bench = ax.json("neural", "benchmark")
    check(bench.get("openedLiveDatabase") is False, f"benchmark {bench}")
    check(bench.get("database") == "temporary" and bench.get("winner") == "hash-v1", f"benchmark {bench}")
    share = ax.json("neural", "share")
    check(share.get("sent") is False and share.get("rawFeedback") is False, f"share {share}")


def models(ax: Ax) -> None:
    flags = ("--explicit", "--beats-baseline", "--regressions-clear")
    ax.json("neural", "register", "m", "1", "aaa", "--metrics", '{"precision":0.8}')
    ax.json("neural", "register", "m", "2", "bbb")
    refused = ax.run("neural", "promote", "m", "1", want=1)
    check("explicit" in refused, refused[-2000:])
    ax.json("neural", "promote", "m", "1", *flags)
    ax.json("neural", "promote", "m", "2", *flags)
    check(ax.json("neural", "model-status", "m").get("promotedChecksum") == "bbb", "promote m 2")
    check(ax.json("neural", "rollback", "m").get("rolledBackTo") == "1", "rollback m")
    check(ax.json("neural", "model-status", "m").get("promotedChecksum") == "aaa", "after rollback")
    listed = ax.json("neural", "models").get("models", [])
    check([model.get("version") for model in listed] == ["1", "2"], f"models {listed}")


def main() -> int:
    if len(sys.argv) != 2:
        print(__doc__)
        return 2
    binary = pathlib.Path(sys.argv[1]).resolve()
    check(binary.is_file(), f"no binary at {binary}")
    with tempfile.TemporaryDirectory() as tmp:
        root = pathlib.Path(tmp).resolve()
        # A name that no export key contains, so finding it in the export means it leaked.
        home, project = root / "home", root / "parser-sample-7f3c"
        home.mkdir()
        (project / "src").mkdir(parents=True)
        (project / "src" / "lib.rs").write_text("pub fn parse() {}\n", encoding="utf-8")
        ax = Ax(binary, home, project)
        ax.run("init")
        check((project / ".ax" / "ax.db").is_file(), "init made no .ax/ax.db")
        configure(project)
        feedback_and_predict(ax)
        preflight_and_status(ax)
        export_evaluate_benchmark(ax)
        models(ax)
    print("real run: every step checked")
    return 0


if __name__ == "__main__":
    try:
        sys.exit(main())
    except (AssertionError, OSError, subprocess.SubprocessError) as error:
        print(f"FAIL real run: {error}")
        sys.exit(1)
