#!/bin/sh
# Container entry for a hosted remote MCP server (Azure Container Apps, Render, Koyeb,
# Cloud Run): clone a repository, index it with ax, then serve it over authenticated HTTP
# with `ax mcp remote serve`. The platform's HTTPS ingress terminates TLS and forwards to
# $PORT; clients connect to <public URL>/projects/<id>/mcp with `Authorization: Bearer <key>`.
#
#   AX_REPO_URL       git URL to fetch (required; may carry a token for private repos, which
#                     is used for the fetch only and not stored in the checkout)
#   AX_REPO_REF       branch or tag to check out (optional; default branch otherwise). A
#                     restart with an existing checkout fetches this ref again.
#   AX_API_KEY        bearer key clients send (required, at least 32 characters). Only its
#                     SHA-256 is written to the server configuration.
#   AX_API_KEY_WRITE  1 lets the key write memories and policy (default: read-only context)
#   AX_PROJECT_ID     project ID in the MCP URL (default: project)
#   AX_PUBLIC_URL     public HTTPS origin; detected on Render, Koyeb, Railway and Azure
#                     Container Apps when unset
#   PORT              listen port (default: 8080)
#   AX_WORKDIR        checkout directory (default: /tmp/project)
set -eu

die() { printf 'ax-serve-repo: %s\n' "$*" >&2; exit 1; }

: "${AX_REPO_URL:?set AX_REPO_URL to the git URL of the repository to serve}"
key="${AX_API_KEY:-}"
[ "${#key}" -ge 32 ] || die "set AX_API_KEY to a random key of at least 32 characters"
case "$key" in *[!A-Za-z0-9._~+/=-]*) die "AX_API_KEY may only contain letters, digits and ._~+/=-" ;; esac

public="${AX_PUBLIC_URL:-}"
if [ -z "$public" ]; then
  if [ -n "${RENDER_EXTERNAL_URL:-}" ]; then public="$RENDER_EXTERNAL_URL"
  elif [ -n "${RENDER_EXTERNAL_HOSTNAME:-}" ]; then public="https://$RENDER_EXTERNAL_HOSTNAME"
  elif [ -n "${KOYEB_PUBLIC_DOMAIN:-}" ]; then public="https://$KOYEB_PUBLIC_DOMAIN"
  elif [ -n "${RAILWAY_PUBLIC_DOMAIN:-}" ]; then public="https://$RAILWAY_PUBLIC_DOMAIN"
  elif [ -n "${CONTAINER_APP_NAME:-}" ] && [ -n "${CONTAINER_APP_ENV_DNS_SUFFIX:-}" ]; then
    public="https://$CONTAINER_APP_NAME.$CONTAINER_APP_ENV_DNS_SUFFIX"
  else
    die "set AX_PUBLIC_URL to the service's public HTTPS origin (for example https://ax.example.com)"
  fi
fi
public="${public%/}"
case "$public" in
  https://*) ;;
  *) die "AX_PUBLIC_URL must start with https:// (got '$public')" ;;
esac
case "${public#https://}" in
  ''|*[!A-Za-z0-9.:-]*) die "AX_PUBLIC_URL must be an origin without path, query or credentials" ;;
esac

project="${AX_PROJECT_ID:-project}"
case "$project" in
  ''|*[!A-Za-z0-9_-]*) die "AX_PROJECT_ID may only contain letters, digits, - and _" ;;
esac
write=false
[ "${AX_API_KEY_WRITE:-0}" = 1 ] && write=true
port="${PORT:-8080}"
case "$port" in ''|*[!0-9]*) die "PORT must be a number" ;; esac

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
echo "ax-serve-repo: indexing $(git rev-parse --short HEAD) in $dir" >&2
ax init . >&2
root="$(pwd -P)"
case "$root" in *[\"\\]*) die "checkout path must not contain quotes or backslashes" ;; esac

hash="$(printf '%s' "$key" | sha256sum | cut -d' ' -f1)"
config_dir="${HOME:-/tmp}/.ax"
mkdir -p "$config_dir"
config="$config_dir/remote-serve.json"
umask 077
# Built-in OAuth is required by the configuration format. In this headless build no one can
# approve OAuth requests (that lives in the Command Center), so only the API key grants access.
cat > "$config" <<EOF
{
  "public_url": "$public",
  "oauth": {"built_in": true, "issuer": "$public/oauth", "jwks_uri": "$public/oauth/jwks"},
  "projects": [{"id": "$project", "root": "$root", "subjects": ["owner"]}],
  "api_keys": [{"id": "cloud", "sha256": "$hash", "projects": ["$project"], "write": $write}]
}
EOF
echo "ax-serve-repo: serving $public/projects/$project/mcp on port $port" >&2
exec ax mcp remote serve --config "$config" --bind 0.0.0.0 --port "$port"
