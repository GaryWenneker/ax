---
title: Headless Linux build
description: Build a smaller ax binary without the Command Center, desktop client or self-update, for Linux sandboxes and containers.
---

The headless build is the same `ax` binary without its graphical parts. It is meant for places with no screen and no browser: a Vercel Sandbox, an Azure Container Apps job, a CI runner. The regular ax download stays the full build.

It keeps the CLI, MCP over stdio (`ax serve --mcp`), `ax mcp remote serve`, indexing, graph queries, policy, specs, sessions and the memory vault. It leaves out `ax web`, `ax share`, `ax desktop`, `ax ship --watch`, `ax upgrade` and `ax server`. The first five still parse and exit with a message that they are not part of this build; `ax server` is removed entirely.

## Languages

By default the headless build links only the core tree-sitter grammars: Rust, Python, Go, Java, JavaScript/JSX, TypeScript/TSX, C, PHP, Ruby, Lua, YAML, XML, Vue, Svelte, Astro, Properties, Liquid and Twig. Files in other languages (Kotlin, C#, C++, Swift, Dart, Scala, Luau, Objective-C, R, Razor, Pascal) are listed as files but get no symbols. Those grammars account for most of the binary size. To keep every language:

```bash
AX_FEATURES=mimalloc slim/build.sh
```

## Build a static Linux binary

The repository's `slim/` directory holds the Dockerfile and a build script. With Docker or Podman installed, run from the repository root:

```bash
slim/build.sh
```

The script builds a static musl binary for the container engine's architecture and copies it to `slim/dist/ax-linux-<arch>`. Pass a platform to build another architecture:

```bash
slim/build.sh linux/amd64
```

Without containers, on a Linux machine with Rust:

```bash
cargo build --locked -p ax-cli --no-default-features --features "minimal-grammars mimalloc" --profile release-slim
```

`--no-default-features` turns off the `web`, `desktop` and `self-update` features of `ax-cli`; `minimal-grammars` links only the core grammars, and `mimalloc` replaces the platform allocator (musl's malloc is slow under parallel indexing). The `release-slim` profile optimizes for size with fat LTO, one codegen unit, `panic = "abort"` and stripped symbols.

## Container image

```bash
docker build -f slim/Dockerfile -t "ax-slim:$(git rev-parse --short HEAD)" .
```

The default image is distroless and runs as a non-root user without a shell. Its default command is `ax serve --mcp`. For `ax diff`, `ax affected` and `ax ship`, which call `git`, build the Ubuntu variant:

```bash
docker build -f slim/Dockerfile --target ubuntu -t "ax-slim:$(git rev-parse --short HEAD)-ubuntu" .
```

The image runs as a non-root user, so a bind-mounted project must be writable by it. On Linux with Docker, run as your own user and point `HOME` at a writable directory:

```bash
docker run --rm --user "$(id -u):$(id -g)" -e HOME=/tmp -v "$PWD:/workspace" "ax-slim:$(git rev-parse --short HEAD)" init
```

```bash
docker run --rm --user "$(id -u):$(id -g)" -e HOME=/tmp -v "$PWD:/workspace" "ax-slim:$(git rev-parse --short HEAD)" query "where is the config loaded"
```

## Speed

The headless build indexes as fast as the full build: 20.6 seconds for 625 Rust files on a 14-core Apple silicon Mac, against 20.4 seconds for the full build. Most of that time goes to resolving references in SQLite, so keep the project's `.ax/` folder on the container's own disk or a volume. A bind mount from a macOS host made the same index take over two minutes. `slim/README.md` has the measurements.

## Sandboxes and Container Apps

A Vercel Sandbox is a Linux VM, so copy the static binary in and run it next to the agent; the agent starts `ax serve --mcp` as its MCP server. On Azure Container Apps, run the image as a job against a mounted share or a cloned repository.

`ax mcp remote serve` listens on `127.0.0.1` only. Inside a container it is reachable from the same container or pod, not from Container Apps ingress directly.

See `slim/README.md` in the repository for measured sizes and more run examples.
