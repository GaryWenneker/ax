#!/usr/bin/env python3
"""Choose CI work from changed paths, keeping web UI assets out of Rust builds."""
import os
from pathlib import PurePosixPath
import subprocess


def classify(paths):
    backend = any(
        (p.startswith('crates/') and not p.startswith('crates/ax-web/web-ui/'))
        or p.endswith('.rs')
        or PurePosixPath(p).name in {'Cargo.toml', 'Cargo.lock', 'rust-toolchain', 'rust-toolchain.toml'}
        or p.startswith('.cargo/')
        or p.startswith('database/')
        or p == 'scripts/check-home-dir.sh'
        for p in paths
    )
    frontend = backend or any(
        p.startswith('crates/ax-web/web-ui/')
        or p in {'.github/workflows/ci.yml', 'scripts/ci-changes.py', 'scripts/test_ci_changes.py'}
        for p in paths
    )
    return {'backend': backend, 'frontend': frontend}


def main():
    base = os.environ.get('CI_BASE_SHA', '')
    head = os.environ.get('CI_HEAD_SHA', 'HEAD')
    if not base or set(base) == {'0'}:
        flags = {'backend': True, 'frontend': True}
    else:
        paths = subprocess.check_output(['git', 'diff', '--name-only', base, head], text=True).splitlines()
        flags = classify(paths)
    output = '\n'.join(f'{key}={str(value).lower()}' for key, value in flags.items()) + '\n'
    print(output, end='')
    if os.environ.get('GITHUB_OUTPUT'):
        with open(os.environ['GITHUB_OUTPUT'], 'a', encoding='utf-8') as stream:
            stream.write(output)


if __name__ == '__main__':
    main()
