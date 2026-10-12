"""Three explicit negative controls; always restore the exact original bytes."""
from pathlib import Path
import os
import subprocess

def main() -> None:
    root = Path(__file__).resolve().parents[3]
    cargo = os.environ.get("CARGO", "cargo")
    cases = [
        ("homepage selector", "site/src/scripts/scroll-reveal.js",
         '"[data-ax-reveal]",', '"[data-disabled-reveal]",',
         ["node", "--experimental-strip-types", "--test", "site/tests/scroll-reveal.test.ts"]),
        ("stale attribution", "crates/ax-policy/src/provenance.rs",
         "value.content_hash == crate::revisions::content_hash(incoming)", "!incoming.is_empty()",
         [cargo, "test", "-p", "ax-policy", "--lib", "origin_survives_reopen_and_only_matches_exact_content"]),
        ("credential disclosure", "crates/ax-share/src/sync.rs",
         'url.set_username("").ok()?;', '// Mutant: username remains visible.',
         [cargo, "test", "-p", "ax-share", "--lib", "share_source_never_contains_url_credentials_or_query_tokens"]),
    ]
    for name, relative, original, mutant, command in cases:
        path = root / relative
        saved = path.read_bytes()
        text = saved.decode("utf-8")
        if text.count(original) != 1:
            raise RuntimeError(f"{name}: expected exactly one mutation location")
        try:
            path.write_text(text.replace(original, mutant), encoding="utf-8")
            result = subprocess.run(command, cwd=root, capture_output=True, text=True, timeout=300)
            output = result.stdout + result.stderr
            if result.returncode == 0 or "FAILED" not in output and "fail 1" not in output:
                raise RuntimeError(f"{name}: not killed by a behavioral assertion\n{output}")
            print(f"Killed: {name}", flush=True)
        finally:
            path.write_bytes(saved)
    print("3/3 explicit mutants killed and restored", flush=True)


if __name__ == "__main__":
    main()
