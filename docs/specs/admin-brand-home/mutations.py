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
        ('retired route restored', UI/'src/lib/routes.ts', "PATH_PAGE['/'] = 'stats';", "PATH_PAGE['/'] = 'stats';\nPATH_PAGE['/ship'] = 'stats';", None, 'Quality Gates page and navigation are retired'),
        ('wrong logo font', UI/'src/index.css', 'font-family:"Archivo Variable",system-ui,sans-serif;', 'font-family:"Oxanium Variable",system-ui,sans-serif;', 'header matches', 'expect(style.font)'),
        ('fixed-width tags restored', UI/'src/controls.css', ':is(.page-level-badge,.page-scope-badge,.page-origin-badge,.mcp-kind-badge,.policy-pack-badge)', ':is(.page-level-badge,.page-scope-badge,.page-item-badge,.page-origin-badge,.mcp-kind-badge,.policy-pack-badge)', 'tags fit', 'expect(widths[0])'),
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
                result = run(['node','docs/specs/admin-brand-home/ui-check.mjs'],env={**os.environ,'AX_BRAND_GREP':grep})
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
