#!/bin/sh
# Container entry for one-shot jobs (Azure Container Apps jobs, CI): clone a repository,
# index it with ax, then run a command against the index.
#
#   AX_REPO_URL     git URL to clone (required; may carry a token for private repos)
#   AX_REPO_REF     branch or tag to check out (optional; default branch otherwise)
#   AX_JOB_COMMAND  shell command to run in the indexed checkout (default: ax status)
#   AX_WORKDIR      checkout directory (default: /tmp/project)
set -eu

: "${AX_REPO_URL:?set AX_REPO_URL to the git URL of the repository to index}"
dir="${AX_WORKDIR:-/tmp/project}"

if [ -n "${AX_REPO_REF:-}" ]; then
  git clone --quiet --depth 1 --branch "$AX_REPO_REF" "$AX_REPO_URL" "$dir"
else
  git clone --quiet --depth 1 "$AX_REPO_URL" "$dir"
fi
cd "$dir"
echo "ax-index-repo: indexing $(git rev-parse --short HEAD) in $dir" >&2
ax init . >&2
exec sh -c "${AX_JOB_COMMAND:-ax status}"
