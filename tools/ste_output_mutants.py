#!/usr/bin/env python3
"""Persisted manual mutations for the pilot; restore source after every run."""
import argparse
import hashlib
import json
import subprocess
from pathlib import Path


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", required=True)
    args = parser.parse_args()
    root = Path(__file__).resolve().parent.parent
    mutants = [
        ("reject-invalid-style", "crates/ax-mcp/src/text_style.rs", 'Some(_) => Err("textStyle must be \\"standard\\" or \\"ste-inspired\\"".into()),', 'Some(_) => Ok(Self::Standard),', "s04_invalid_style_has_no_delivery_effect"),
        ("validate-before-reset", "crates/ax-mcp/src/server.rs", '    crate::text_style::PreflightTextStyle::validate_tool(name, &args)?;', '', "s04_invalid_style_has_no_delivery_effect"),
        ("metadata-is-opt-in", "crates/ax-mcp/src/text_style.rs", 'if self == Self::SteInspired {', 'if self == Self::Standard {', "s01_legacy_default_is_unchanged"),
        ("metadata-does-not-duplicate-bodies", "crates/ax-mcp/src/server.rs", '                metadata["presentation"] = presentation.clone();', '                metadata["presentation"] = presentation.clone();\n                metadata["rules"] = value["rules"].clone();', "s13_metadata_does_not_duplicate_bodies"),
        ("source-exceptions-survive", "crates/ax-mcp/src/smart_output.rs", 'format!("{instruction}\\n\\n{body}")', 'format!("{instruction}\\n\\n{}", body.replace("unless", "if"))', "s05_protected_source_is_exact"),
    ]
    results = []
    for name, relative, old, new, test in mutants:
        path = root / relative
        original = path.read_bytes()
        source = original.decode("utf-8")
        if source.count(old) != 1: raise RuntimeError(f"{name}: mutation target must occur exactly once")
        mutated = source.replace(old, new).encode("utf-8")
        if mutated == original: raise RuntimeError(f"{name}: mutation did not change source")
        try:
            path.write_bytes(mutated)
            if path.read_bytes() != mutated: raise RuntimeError(f"{name}: mutant was not written")
            command = ["cargo", "test", "-p", "ax-mcp", "--locked", "--test", "ste_output_contract", test, "--", "--exact", "--test-threads=1"]
            run = subprocess.run(command, cwd=root, capture_output=True, text=True)
            combined = run.stdout + run.stderr
            killed = run.returncode != 0 and "test result: FAILED" in combined and test in combined
            results.append({"mutant": name, "test": test, "executed_source_sha256": hashlib.sha256(mutated).hexdigest(), "exit_code": run.returncode, "killed": killed})
            if not killed: raise RuntimeError(f"{name}: mutant survived or did not reach a behavioral assertion\n{combined[-3000:]}")
            if name == "source-exceptions-survive":
                property_run = subprocess.run(["cargo", "test", "-p", "ax-mcp", "--locked", "--lib", "text_style::tests::arbitrary_utf8_policy_bodies_survive_the_authored_envelope", "--", "--exact"], cwd=root, capture_output=True, text=True)
                property_kill = property_run.returncode != 0 and "test result: FAILED" in property_run.stdout
                results[-1]["property_suite_killed"] = property_kill
                if not property_kill: raise RuntimeError("source-preservation property did not kill its mutant")
        finally:
            path.write_bytes(original)
            if path.read_bytes() != original: raise RuntimeError(f"{name}: source restore failed")
        print(json.dumps(results[-1]), flush=True)
    Path(args.output).write_text(json.dumps(results, indent=2), encoding="utf-8")


if __name__ == "__main__": main()
