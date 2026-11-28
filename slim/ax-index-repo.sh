#!/bin/sh
# Container entry for one-shot jobs (Azure Container Apps jobs, CI): clone a repository,
# index it with ax, then run a command against the index.
#
#   AX_REPO_URL     git URL to fetch (required; may carry a token for private repos, which is
#                   used for the fetch only and not stored in the checkout)
#   AX_REPO_REF     branch or tag to check out (optional; default branch otherwise). An
#                   existing checkout in AX_WORKDIR is updated to this ref.
#   AX_JOB_COMMAND  shell command to run in the indexed checkout (default: ax status)
#   AX_WORKDIR      checkout directory (default: /tmp/project)
set -eu

: "${AX_REPO_URL:?set AX_REPO_URL to the git URL of the repository to index}"
dir="${AX_WORKDIR:-/tmp/project}"

# Fetch AX_REPO_URL at AX_REPO_REF (the default branch otherwise) into $dir. An existing
# checkout is updated, so a restart picks up new commits and a changed AX_REPO_REF. The URL
# is only passed to git fetch: origin is stored without credentials, so a token in the URL
# never reaches .git/config.
checkout_repo() {
  mkdir -p "$dir"
  [ -d "$dir/.git" ] || git -C "$dir" init --quiet
  git -C "$dir" fetch --quiet --depth 1 "$AX_REPO_URL" "${AX_REPO_REF:-HEAD}"
  git -C "$dir" -c advice.detachedHead=false checkout --quiet --force FETCH_HEAD
  git -C "$dir" remote remove origin 2>/dev/null || true
  git -C "$dir" remote add origin \
    "$(printf '%s' "$AX_REPO_URL" | sed -E 's#^([A-Za-z][A-Za-z0-9+.-]*://)[^@/]*@#\1#')"
}
checkout_repo
cd "$dir"
echo "ax-index-repo: indexing $(git rev-parse --short HEAD) in $dir" >&2
ax init . >&2
exec sh -c "${AX_JOB_COMMAND:-ax status}"
