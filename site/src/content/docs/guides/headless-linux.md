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

## Test it remotely

The binary is static, so any Linux machine can run it. Build the one that matches the remote CPU: `slim/build.sh` builds for your container engine's own architecture, and `slim/build.sh linux/amd64` builds x86_64 (slow under emulation on an arm64 Mac). Check the remote side with `uname -m`: `x86_64` needs `ax-linux-x86_64`, `aarch64` needs `ax-linux-aarch64`.

Keep the project and its `.ax/` folder on the machine's own disk. On a network or shared mount, SQLite's `fsync` makes indexing several times slower.

### Vercel Sandbox

A Vercel Sandbox is a Linux microVM (Ubuntu 26.04, internet access on by default), so the binary runs directly, without an image. Install the `sandbox` CLI and log in:

```bash
npm i -g sandbox
```

```bash
sandbox login
```

Create a sandbox that stays up for 30 minutes and check its architecture:

```bash
sandbox create --name ax-test --timeout 30m
```

```bash
sandbox exec ax-test -- uname -m
```

Copy in the matching binary and clone a project to index:

```bash
sandbox copy slim/dist/ax-linux-x86_64 ax-test:/vercel/ax
```

```bash
sandbox exec ax-test -- chmod +x /vercel/ax
```

```bash
sandbox exec ax-test -- git clone --depth 1 https://github.com/<owner>/<repo> /vercel/proj
```

Index, then ask the graph something:

```bash
sandbox exec -w /vercel/proj ax-test -- bash -c "time /vercel/ax init . && /vercel/ax status && /vercel/ax query main"
```

Check the MCP server over stdio. The reply lists the server info and its tools:

```bash
sandbox exec -w /vercel/proj ax-test -- bash -c 'printf "%s\n" "{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"initialize\",\"params\":{\"protocolVersion\":\"2025-06-18\",\"capabilities\":{},\"clientInfo\":{\"name\":\"t\",\"version\":\"1\"}}}" "{\"jsonrpc\":\"2.0\",\"method\":\"notifications/initialized\"}" "{\"jsonrpc\":\"2.0\",\"id\":2,\"method\":\"tools/list\"}" | /vercel/ax serve --mcp'
```

`sandbox connect ax-test` opens an interactive shell. When you are done, `sandbox stop ax-test` keeps a snapshot and `sandbox remove ax-test` deletes it. On the Hobby plan a session lasts at most 45 minutes.

### Any Linux host over SSH

```bash
scp slim/dist/ax-linux-x86_64 user@host:ax
```

```bash
ssh user@host 'chmod +x ax && cd <project> && ./ax init . && ./ax status'
```

### Azure Container Apps

Push the image (tagged with a commit or version) to a registry the environment can pull from. Then create a manual job from the `ubuntu` target, which includes `git`, with a command such as `sh -c "git clone --depth 1 <repo> /tmp/p && cd /tmp/p && ax init . && ax query main"`. Start the job and read its output in the job's logs.

## Limits

An agent in a sandbox or container starts `ax serve --mcp` itself and talks to it over stdio. `ax mcp remote serve` listens on `127.0.0.1` only, so inside a container it is reachable from the same container or pod, not from Container Apps ingress directly.

See `slim/README.md` in the repository for measured sizes and more run examples.
