#!/usr/bin/env bash
# Copy the project database to a temp file and dry-run the hierarchy classifier.
# The copy is the only database the dry-run opens. The live .ax/ax.db is read
# with mode=ro and is never the cargo test fixture.
set -euo pipefail

root=$(cd "$(dirname "$0")/.." && pwd)
source="$root/.ax/ax.db"
if [[ ! -f "$source" ]]; then
  echo "missing project database: $source" >&2
  exit 1
fi

work=$(mktemp -d "${TMPDIR:-/tmp}/ax-hierarchy-cutover.XXXXXX")
trap 'rm -rf "$work"' EXIT
dest="$work/fixture.db"

before_inode=$(stat -f '%i' "$source")
before_size=$(stat -f '%z' "$source")
before_mtime=$(stat -f '%m' "$source")
python3 - "$source" "$dest" << 'PY'
import sqlite3
import sys

source = sqlite3.connect(f"file:{sys.argv[1]}?mode=ro", uri=True)
copy = sqlite3.connect(sys.argv[2])
with copy:
    source.backup(copy)
source.close()
copy.close()
PY
after_inode=$(stat -f '%i' "$source")
after_size=$(stat -f '%z' "$source")
after_mtime=$(stat -f '%m' "$source")
if [[ "$before_inode" != "$after_inode" || "$before_size" != "$after_size" ]]; then
  echo "live database inode or size changed during the read-only backup" >&2
  echo "before: inode=$before_inode size=$before_size" >&2
  echo "after:  inode=$after_inode size=$after_size" >&2
  exit 1
fi

case "$dest" in
  *.ax/ax.db)
    echo "refusing to dry-run the live database" >&2
    exit 1
    ;;
esac

echo "fixture=$dest"
echo "live_inode=$before_inode live_size=$before_size mtime_before=$before_mtime mtime_after=$after_mtime"
AX_HIERARCHY_FIXTURE="$dest" cargo test -p ax-usage --lib copied_project_database_dry_run_keeps_hashes -- --ignored --nocapture
