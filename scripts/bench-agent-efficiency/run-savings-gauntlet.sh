#!/usr/bin/env bash
# WITH vs WITHOUT token gauntlet. See README.md in this directory.
set -euo pipefail
cd "$(dirname "$0")/../.."
exec python3 scripts/bench-agent-efficiency/gauntlet.py
