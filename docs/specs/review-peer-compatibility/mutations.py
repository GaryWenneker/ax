"""Three real peer-compatibility mutants; restore exact source bytes after each."""
from pathlib import Path
import os
import subprocess

def main() -> None:
    source = Path('crates/ax-cli/src/server_replica.rs')
    original = source.read_bytes()
    cases = [
        ('accept-incompatible-protocol', 'if protocol != "3" {', 'if false {'),
        ('hide-version-mismatch', 'version != env!("CARGO_PKG_VERSION")', 'version == env!("CARGO_PKG_VERSION")'),
        ('drop-persisted-warning', '"compatibility_warning": warning,', '"compatibility_warning": serde_json::Value::Null,'),
    ]
    try:
        for name, old, new in cases:
            text = original.decode('utf-8')
            if old not in text:
                raise RuntimeError(f'Mutant {name} was not inserted')
            source.write_text(text.replace(old, new, 1), encoding='utf-8')
            result = subprocess.run([os.environ.get('CARGO','cargo'),'test','-p','ax-cli','--bin','ax','compatibility_tests'], capture_output=True, text=True, timeout=300)
            if result.returncode == 0 or 'test result: FAILED' not in result.stdout:
                raise RuntimeError(f'Mutant {name} survived or failed before testing: {result.stderr[-2000:]} {result.stdout[-2000:]}')
            print(f'KILLED {name}', flush=True)
            source.write_bytes(original)
    finally:
        source.write_bytes(original)
    assert source.read_bytes() == original

if __name__ == '__main__':
    main()
