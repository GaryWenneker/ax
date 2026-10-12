"""Behavioral negative controls; restores sources in every outcome."""
from pathlib import Path
import os
import subprocess
ROOT = Path(__file__).resolve().parents[3]
UI = ROOT / 'crates/ax-web/web-ui'

def run(args: list[str], *, env: dict[str, str] | None = None) -> subprocess.CompletedProcess[str]:
    return subprocess.run(args, cwd=ROOT, text=True, encoding="utf-8", timeout=240, stdout=subprocess.PIPE, stderr=subprocess.STDOUT, env=env)

def main() -> None:
    model = UI/'src/lib/columnModel.ts'
    cases = [
        ('sorting removed', model, 'if (sort) matching.sort', 'if (false && sort) matching.sort', None, 'sorting is stable'),
        ('column filters disabled', model, '.includes(query.trim().toLocaleLowerCase())', ".includes('')", None, 'filters intersect'),
        ('width bounds removed', model, 'Math.round(Math.max(minimum, Math.min(1600, value)))', 'Math.round(value)', None, 'persisted widths'),
        ('logging stack alignment broken', UI/'src/controls.css', None, '\n@container ax-list (min-width:1000px){html[data-ax-layout="portal"] body .mcp-trace-calm .page-item-body{align-items:center;gap:12px}}\n', 'logging retains', 'logging retains stacked'),
    ]
    for name, path, old, new, grep, marker in cases:
        original = path.read_text(encoding="utf-8")
        if old is not None and original.count(old) != 1:
            raise RuntimeError(f'{name}: expected one mutation target')
        try:
            path.write_text(original + new if old is None else original.replace(old,new), encoding="utf-8")
            if grep:
                build = run(['npm','run','build','--prefix',str(UI)])
                if build.returncode: raise RuntimeError(build.stdout)
                result = run(['node','docs/specs/list-columns/ui-check.mjs'],env={**os.environ,'AX_DENSITY_GREP':grep,'AX_COLUMN_SCREENSHOTS':'','AX_DENSITY_SCREENSHOTS':''})
                assertion = 'Error: expect' in result.stdout
            else:
                result = run(['node','--experimental-strip-types','--test',str(UI/'src/lib/columnModel.test.ts')])
                assertion = 'AssertionError' in result.stdout
            if result.returncode != 1 or marker not in result.stdout or not assertion:
                raise RuntimeError(f'{name}: expected behavioral assertion did not fail\n{result.stdout}')
            print(f'KILLED: {name}', flush=True)
        finally:
            path.write_text(original, encoding="utf-8")
    print('4/4 negative controls killed; sources restored. verify.sh rebuilds the final state.',flush=True)

if __name__ == '__main__':
    main()
