#!/bin/sh
# Build the headless ax binary for Linux and copy it to slim/dist/ax-linux-<arch>.
#
#   slim/build.sh                 # native arch of the container engine
#   slim/build.sh linux/amd64     # explicit platform (emulated when not native)
#   AX_FEATURES=mimalloc slim/build.sh   # keep all tree-sitter grammars
#   AX_LTO=thin slim/build.sh     # less memory for the final link
#
# Uses docker (BuildKit) or podman, whichever is installed.
set -eu

root="$(cd "$(dirname "$0")/.." && pwd)"
platform="${1:-}"
engine="${CONTAINER_ENGINE:-$(command -v docker >/dev/null 2>&1 && echo docker || echo podman)}"

set -- build -f "$root/slim/Dockerfile" --target builder -t ax-slim-builder
if [ -z "$platform" ] && [ "$engine" = podman ]; then
    # podman reuses a locally tagged base image of another architecture; pin the host's.
    platform="linux/$(podman info --format '{{.Host.Arch}}')"
fi
[ -n "$platform" ] && set -- "$@" --platform "$platform"
# AX_FEATURES overrides the Dockerfile default ("minimal-grammars mimalloc").
[ -n "${AX_FEATURES+set}" ] && set -- "$@" --build-arg "AX_FEATURES=$AX_FEATURES"
# AX_LTO=thin for container engines with less than ~6 GB of memory.
[ -n "${AX_LTO:-}" ] && set -- "$@" --build-arg "AX_LTO=$AX_LTO"
if [ "$engine" = podman ]; then
    # podman does not pick up <Dockerfile>.dockerignore on its own.
    set -- "$@" --ignorefile "$root/slim/Dockerfile.dockerignore"
fi
"$engine" "$@" "$root"

arch="$("$engine" run --rm --entrypoint uname ax-slim-builder -m)"
mkdir -p "$root/slim/dist"
cid="$("$engine" create ax-slim-builder)"
trap '"$engine" rm -f "$cid" >/dev/null' EXIT
"$engine" cp "$cid:/out/ax" "$root/slim/dist/ax-linux-$arch"
ls -l "$root/slim/dist/ax-linux-$arch"
