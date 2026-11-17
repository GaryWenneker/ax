#!/usr/bin/env python3
"""Gates for the Neural "must not" rules in docs/specs/ax-neural/SPEC.md.

Each gate first runs on a known-bad sample and must flag it (its negative control), then runs on
the real input and must find nothing. A crash, an unreadable input, a control that is not
flagged, or a real finding exits 1.

What these gates read is text: a dependency tree, a source file, a constant, git status. Each one
catches the spellings named here, not every way to break its rule. The tests cover behavior.
"""
import pathlib
import re
import subprocess
import sys

ROOT = pathlib.Path(__file__).resolve().parent.parent

ML_RUNTIME = re.compile(r"^(?:candle|burn|ort|tract|onnx|onnxruntime)(?:[-_][\w-]+)? v\S+", re.M)
REASONING = re.compile(r"^ax-reasoning v\S+", re.M)
NEURAL_CALL = re.compile(r"ax_neural|neural_tool|neural::")
EDGE_WRITE = re.compile(
    r"\b(?:insert(?:\s+or\s+\w+)?\s+into|replace\s+into|update|delete\s+from)\s+edges\b", re.I
)
CREDENTIAL = re.compile(
    r"AKIA[0-9A-Z]{16}|gh[pousr]_[A-Za-z0-9]{36}|sk-[A-Za-z0-9_-]{20,}|xox[abprs]-[A-Za-z0-9-]{10,}"
    r"|-----BEGIN [A-Z ]*PRIVATE KEY-----"
)
NEURAL_FILES = (
    "crates/ax-neural/**/*",
    "crates/ax-mcp/src/neural_tool.rs",
    "crates/ax-mcp/tests/neural_preflight.rs",
    "crates/ax-cli/src/commands/neural.rs",
    "crates/ax-web/src/neural.rs",
    "crates/ax-db/tests/migration_v46.rs",
    "crates/ax-web/web-ui/src/components/neuralMetrics*.ts",
    "docs/specs/ax-neural/*",
    "scripts/neural-*",
)
WEIGHT = re.compile(r"^pub const (W_\w+): f64 = ([0-9.]+);$", re.M)
# The plan: "FTS 0.45, vector 0.25, ACT-R 0.15, graph 0.10, usage 0.05 ... Do not retune it."
LOCKED = {"W_FTS": 0.45, "W_VECTOR": 0.25, "W_ACT_R": 0.15, "W_GRAPH": 0.10, "W_USAGE": 0.05}
PACKAGE = re.compile(r"^(\S+) v(\S+)(.*)$")


def run(*args: str) -> str:
    return subprocess.run(args, cwd=ROOT, capture_output=True, text=True, check=True).stdout


def tree(package: str, *extra: str) -> str:
    out = run("cargo", "tree", "-p", package, "-e", "normal,build", "--prefix", "none", *extra)
    if not re.search(rf"^{re.escape(package)} v", out, re.M):
        raise RuntimeError(f"cargo tree for {package} has no root line")
    return out


def matches(pattern: re.Pattern[str]):
    return lambda text: [m.group(0) for m in pattern.finditer(text)]


def weight_drift(text: str) -> list[str]:
    found = {name: float(value) for name, value in WEIGHT.findall(text)}
    if set(found) != set(LOCKED):
        return [f"weights found: {sorted(found)}, expected {sorted(LOCKED)}"]
    return [f"{name} = {found[name]}, locked {value}" for name, value in LOCKED.items() if found[name] != value]


def head_lock() -> set[tuple[str, str]]:
    text = run("git", "show", "HEAD:Cargo.lock")
    locked = set(re.findall(r'name = "([^"]+)"\nversion = "([^"]+)"', text))
    if not locked:
        raise RuntimeError("no packages read from HEAD:Cargo.lock")
    return locked


def new_packages(locked: set[tuple[str, str]]):
    def find(text: str) -> list[str]:
        found = []
        for line in text.splitlines():
            match = PACKAGE.match(line.strip())
            if not match or match.group(3).startswith(" (/"):
                continue
            if (match.group(1), match.group(2)) not in locked:
                found.append(f"{match.group(1)} v{match.group(2)}")
        return sorted(set(found))

    return find


def lines(text: str) -> list[str]:
    return [line for line in text.splitlines() if line.strip()]


TEST_MODULE = "\n#[cfg(test)]\nmod tests {\n"


def production_part(path: pathlib.Path) -> str:
    """The file up to its test module. Anything after `#[cfg(test)] mod tests` counts as test code."""
    text = path.read_text(encoding="utf-8")
    marks = text.count("#[cfg(test)]")
    if marks == 0:
        return text
    if marks != 1 or text.count(TEST_MODULE) != 1:
        raise RuntimeError(f"{path}: expected one `#[cfg(test)] mod tests` and no other test code")
    return text.split(TEST_MODULE)[0]


def neural_sources() -> str:
    files = sorted((ROOT / "crates/ax-neural/src").glob("*.rs"))
    if not files:
        raise RuntimeError("no ax-neural sources found")
    return "\n".join(production_part(path) for path in files)


def neural_files() -> str:
    paths = sorted({path for pattern in NEURAL_FILES for path in ROOT.glob(pattern) if path.is_file()})
    if len(paths) < 20:
        raise RuntimeError(f"only {len(paths)} Neural files found")
    return "\n".join(path.read_text(encoding="utf-8") for path in paths)


def main() -> int:
    locked = head_lock()
    gates = [
        (
            "the default ax-cli build links no ML runtime",
            matches(ML_RUNTIME),
            "ort v2.0.0-rc.12\ncandle-core v0.8.0\n",
            lambda: tree("ax-cli"),
        ),
        (
            # Default features only. ML crates may appear later behind `neural-runtime`
            # after ADR 002 admits a challenger; they must not enter the default tree.
            "the default ax-neural build links no ML runtime",
            matches(ML_RUNTIME),
            "tract-onnx v0.21.0\n",
            lambda: tree("ax-neural"),
        ),
        (
            "ax-neural does not depend on ax-reasoning",
            matches(REASONING),
            "ax-reasoning v9.0.3 (/repo/crates/ax-reasoning)\n",
            lambda: tree("ax-neural", "--all-features"),
        ),
        (
            "ax-neural adds no external crate missing from HEAD's Cargo.lock",
            new_packages(locked),
            "fake-crate v9.9.9\n",
            # ax-memory and ax-db carry other uncommitted work; their crates are not Neural's.
            lambda: tree("ax-neural", "--all-features", "--prune", "ax-memory", "--prune", "ax-db"),
        ),
        (
            "memory recall (ax-context retrieval.rs) does not call Neural",
            matches(NEURAL_CALL),
            "use ax_neural::rank::keep_order;\n",
            lambda: (ROOT / "crates/ax-context/src/retrieval.rs").read_text(encoding="utf-8"),
        ),
        (
            "ax-neural production code writes no rows to the graph edges table",
            matches(EDGE_WRITE),
            '"INSERT OR IGNORE INTO edges (source, target, kind) VALUES (?, ?, ?)"\n',
            neural_sources,
        ),
        (
            "the fusion weights are the locked values",
            weight_drift,
            "pub const W_FTS: f64 = 0.50;\npub const W_VECTOR: f64 = 0.25;\n"
            "pub const W_ACT_R: f64 = 0.15;\npub const W_GRAPH: f64 = 0.10;\npub const W_USAGE: f64 = 0.05;\n",
            lambda: (ROOT / "crates/ax-memory/src/fusion.rs").read_text(encoding="utf-8"),
        ),
        (
            "the Neural files hold no credential-shaped token",
            matches(CREDENTIAL),
            # Built at run time so this file never holds a token-shaped literal itself.
            "AKIA" + "Q" * 16 + "\n",
            neural_files,
        ),
        (
            "no GitHub workflow file changed",
            lines,
            " M .github/workflows/release.yml\n",
            lambda: run("git", "status", "--porcelain", "--", ".github"),
        ),
    ]
    failed = False
    for name, find, bad_sample, load in gates:
        if not find(bad_sample):
            print(f"FAIL {name}: the negative control was not flagged")
            failed = True
            continue
        findings = find(load())
        if findings:
            print(f"FAIL {name}: {findings}")
            failed = True
        else:
            print(f"ok   {name} (control flagged)")
    return 1 if failed else 0


if __name__ == "__main__":
    sys.exit(main())
