#!/usr/bin/env python3
"""Prove removal tests reject three plausible partial reversals; restore source exactly."""
import hashlib
import json
import subprocess
from pathlib import Path

MUTANTS = [
    ("selector-left-in-schema", "tools.rs", '"contextMode": {', '"textStyle": {"type": "string"}, "contextMode": {', "preflight_schema_has_no_writing_style_selector"),
    ("guidance-left-in-output", "smart_output.rs", 'Apply matched rules and skills before editing.\\n\\n{body}', 'Write explanations in clear technical English.\\nApply matched rules and skills before editing.\\n\\n{body}', "raw_preflight_keeps_standard_output_with_obsolete_arguments"),
    ("embedded-style-validation-left", "invoke.rs", 'let args = crate::request_context::public_args(args);', 'let args = crate::request_context::public_args(args); if name == "ax_preflight" && args.get("textStyle").is_some() { return Err("obsolete style validation".into()); }', "embedded_preflight_ignores_obsolete_style_validation"),
]


def main() -> None:
    root = Path(__file__).resolve().parents[1]
    results = []
    for name, file, old, new, test in MUTANTS:
        path = root / "crates/ax-mcp/src" / file
        original = path.read_bytes()
        source = original.decode("utf-8")
        if source.count(old) != 1:
            raise ValueError(f"mutation target is not unique: {name}")
        changed = source.replace(old, new).encode("utf-8")
        try:
            path.write_bytes(changed)
            proc = subprocess.run(["cargo", "test", "-p", "ax-mcp", "--locked", "--test", "preflight_standard_contract", test, "--", "--exact"], cwd=root, text=True, capture_output=True, timeout=600)
            output = proc.stdout + proc.stderr
            killed = proc.returncode == 101 and f"test {test} ... FAILED" in output and "test result: FAILED" in output
            if not killed:
                raise RuntimeError(f"mutant was not killed by its behavioral test: {name}\n{output}")
            results.append({"name": name, "test": test, "source_sha256": hashlib.sha256(changed).hexdigest(), "exit_code": proc.returncode, "killed": True})
        finally:
            path.write_bytes(original)
            if path.read_bytes() != original:
                raise RuntimeError(f"source restoration failed: {file}")
    print(json.dumps(results, indent=2))


if __name__ == "__main__":
    main()
