"""Temporary negative controls; restores sources even when a check fails."""
from pathlib import Path
import os
import subprocess
ROOT = Path(__file__).resolve().parents[3]
UI = ROOT / 'crates/ax-web/web-ui'

def run(args: list[str], *, env: dict[str, str] | None = None) -> subprocess.CompletedProcess[str]:
    return subprocess.run(args, cwd=ROOT, text=True, stdout=subprocess.PIPE, stderr=subprocess.STDOUT, env=env)

def main() -> None:
    cases = [
        ('focus glow removed', UI/'src/controls.css', '--control-focus-glow:0 0 8px 2px color-mix(in srgb,var(--accent-text) 32%,transparent)', '--control-focus-glow:none', 'soft glow', 'fields and keyboard buttons get a soft glow'),
        ('container breakpoints disabled', UI/'src/controls.css', 'container-type:inline-size;container-name:ax-list;', 'container-type:normal;container-name:ax-list;', 'less space', 'Rules Skills and Memory use less space'),
        ('high-contrast focus removed', UI/'src/controls.css', 'outline:1px solid Highlight !important', 'outline:0 !important', 'high-contrast', 'system high-contrast focus remains visible'),
    ]
    for name, path, old, new, grep, marker in cases:
        original = path.read_text()
        if original.count(old) != 1:
            raise RuntimeError(f'{name}: expected one mutation target')
        try:
            path.write_text(original.replace(old,new))
            if grep:
                build = run(['npm','run','build','--prefix',str(UI)])
                if build.returncode: raise RuntimeError(build.stdout)
                result = run(['node','docs/specs/focus-row-density/ui-check.mjs'],env={**os.environ,'AX_DENSITY_GREP':grep})
            else:
                result = run(['node','--experimental-strip-types','--test',str(UI/'src/lib/navigation-retirement.test.ts')])
            if result.returncode != 1 or marker not in result.stdout:
                raise RuntimeError(f'{name}: missing expected assertion failure\n{result.stdout}')
            print(f'KILLED: {name}', flush=True)
        finally:
            path.write_text(original)
    print('3/3 manual mutants killed; sources restored. Run verify.sh to rebuild and verify final state.',flush=True)


if __name__ == "__main__":
    main()
