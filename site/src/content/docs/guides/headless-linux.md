---
title: Headless Linux build
description: Install or build a smaller ax binary without the Command Center, desktop client or self-update, for Linux sandboxes and containers.
---

The headless build is the same `ax` binary without its graphical parts. It is meant for places with no screen and no browser: a Vercel Sandbox, an Azure Container Apps job, a CI runner. The regular ax download stays the full build.

It keeps the CLI, MCP over stdio (`ax serve --mcp`), `ax mcp remote serve`, indexing, graph queries, policy, specs, sessions and the memory vault. It leaves out `ax web`, `ax share`, `ax desktop`, `ax ship --watch`, `ax upgrade` and `ax server`. The first five still parse and exit with a message that they are not part of this build; `ax server` is removed entirely.

## What is in it

| Included | Left out |
|---|---|
| CLI: `init`, `index`, `sync`, `watch`, `status`, `query`, `explore`, `node`, `callers`, `impact`, `report`, `insights`, … | `ax web` and `ax share` (embedded Command Center and its web assets) |
| MCP over stdio (`ax serve --mcp`) and `ax mcp remote serve` (HTTP) | `ax desktop` (native desktop client) |
| Policy, skills, specs, memory vault, sessions, `preflight` | `ax ship --watch` (dashboard) |
| `ax ship --evaluate` / `--ci`, `diff`, `affected` (need `git`) | `ax upgrade` and the update notice |
| Neural ranking (pure-Rust runtime) | `ax server` (standalone knowledge node) |
| Core tree-sitter languages (see [Languages](#languages)) | Grammars for Kotlin, C#, C++, Swift, Dart, Scala, Luau, Objective-C, R, Razor and Pascal |

## Size

The headless binary is about five times smaller than the full ax. Sizes in megabytes (10⁶ bytes), from the v10.1.1 release:

| Build | Size |
|---|---|
| Full ax, macOS arm64 | 161.7 MB |
| Headless, macOS arm64 (local build) | 31.3 MB |
| Headless, Linux arm64 (static) | 33.2 MB, 12.0 MB download |
| Headless, Linux x86_64 (static) | 37.8 MB, 12.9 MB download |
| Container image, distroless (`ax-slim:<version>`) | about 41 MB |
| Container image, Ubuntu 26.04 with `git` (`ax-slim:<version>-ubuntu`) | about 268 MB |

Most of the saving comes from leaving out the Command Center, the desktop client and the less common language grammars. Grammar parse tables make up most of a binary's data, so a headless build with every language (`AX_FEATURES=mimalloc slim/build.sh`) is about 71 MB.

Indexing is as fast as with the full build; see [Speed](#speed).

## Install

Releases from v10.1.1 on include static headless binaries for Linux x86_64 and arm64 (`ax-slim-linux-x64.tar.gz`, `ax-slim-linux-arm64.tar.gz`) and container images. The installer picks the newest release that has them, verifies the SHA-256 checksum and installs a single binary:

```bash
curl -fsSL https://getax.wenneker.io/install-slim.sh | sh
```

Without curl:

```bash
wget -qO- https://getax.wenneker.io/install-slim.sh | sh
```

As root it installs to `/usr/local/bin`, otherwise to `~/.local/bin`. Options: `--version vX.Y.Z` (or `AX_VERSION`) pins a release, `--bin-dir DIR` (or `AX_BIN_DIR`) picks the directory, and `--uninstall` removes the binary. Pass them after `sh -s --`:

```bash
curl -fsSL https://getax.wenneker.io/install-slim.sh | sh -s -- --bin-dir /opt/ax/bin
```

The installer does not touch agent configuration or create an `~/.ax` install tree, and the headless build never updates itself: run the installer again to upgrade. It refuses to replace a full ax at the same path unless you pass `--force`.

Container images, for `linux/amd64` and `linux/arm64`, are tagged with the release version without the `v`:

| Image | Contents |
|---|---|
| `ghcr.io/garywenneker/ax-slim:<version>` | distroless, non-root, no shell; default command `ax serve --mcp` |
| `ghcr.io/garywenneker/ax-slim:<version>-ubuntu` | Ubuntu 26.04 with `git` and the `ax-index-repo` job entry point |

To add ax to your own image, copy the binary from the published image:

```dockerfile
COPY --from=ghcr.io/garywenneker/ax-slim:<version> /usr/local/bin/ax /usr/local/bin/ax
```

## Languages

By default the headless build links only the core tree-sitter grammars: Rust, Python, Go, Java, JavaScript/JSX, TypeScript/TSX, C, PHP, Ruby, Lua, YAML, XML, Vue, Svelte, Astro, Properties, Liquid and Twig. Files in other languages (Kotlin, C#, C++, Swift, Dart, Scala, Luau, Objective-C, R, Razor, Pascal) are listed as files but get no symbols. Those grammars account for most of the binary size. To keep every language:

```bash
AX_FEATURES=mimalloc slim/build.sh
```

## Build a static Linux binary

To build it yourself instead of installing a release: the repository's `slim/` directory holds the Dockerfile and a build script. With Docker or Podman installed, run from the repository root:

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

To build the image from source:

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

The binary is static, so any Linux machine can run it. The installer picks the right architecture. To test a local build instead, copy the binary that matches `uname -m` on the remote side: `x86_64` needs `ax-linux-x86_64`, `aarch64` needs `ax-linux-aarch64` (`slim/build.sh linux/amd64` builds x86_64, slowly under emulation on an arm64 Mac).

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

Install ax with the installer. With `--sudo` it runs as root and lands in `/usr/local/bin`, which is on `PATH`:

```bash
sandbox exec --sudo ax-test -- sh -c "curl -fsSL https://getax.wenneker.io/install-slim.sh | sh"
```

Clone a project, index it and ask the graph something:

```bash
sandbox exec ax-test -- git clone --depth 1 https://github.com/<owner>/<repo> /vercel/proj
```

```bash
sandbox exec -w /vercel/proj ax-test -- bash -c "time ax init . && ax status && ax query main"
```

Check the MCP server over stdio. The reply lists the server info and its tools:

```bash
sandbox exec -w /vercel/proj ax-test -- bash -c 'printf "%s\n" "{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"initialize\",\"params\":{\"protocolVersion\":\"2025-06-18\",\"capabilities\":{},\"clientInfo\":{\"name\":\"t\",\"version\":\"1\"}}}" "{\"jsonrpc\":\"2.0\",\"method\":\"notifications/initialized\"}" "{\"jsonrpc\":\"2.0\",\"id\":2,\"method\":\"tools/list\"}" | ax serve --mcp'
```

With the `@vercel/sandbox` SDK, the same install is one command:

```ts
await sandbox.runCommand({
  cmd: 'sh',
  args: ['-c', 'curl -fsSL https://getax.wenneker.io/install-slim.sh | sh'],
  sudo: true,
});
```

To try a local build instead of a release, copy it in with `sandbox copy slim/dist/ax-linux-x86_64 ax-test:/vercel/ax` and run it as `/vercel/ax`.

`sandbox connect ax-test` opens an interactive shell. When you are done, `sandbox stop ax-test` keeps a snapshot and `sandbox remove ax-test` deletes it. On the Hobby plan a session lasts at most 45 minutes.

### Any Linux host over SSH

```bash
ssh user@host 'curl -fsSL https://getax.wenneker.io/install-slim.sh | sh && cd <project> && ~/.local/bin/ax init . && ~/.local/bin/ax status'
```

### Azure Container Apps

The `-ubuntu` image contains `ax-index-repo`, a job entry point that clones `AX_REPO_URL` (optionally `AX_REPO_REF`), runs `ax init` and then `AX_JOB_COMMAND` (default `ax status`).

#### Deploy with one click

[![Deploy to Azure](https://aka.ms/deploytoazurebutton)](https://portal.azure.com/#create/Microsoft.Template/uri/https%3A%2F%2Fgetax.wenneker.io%2Fdeploy%2Fazure-slim.json)

The button opens a custom deployment in the Azure portal. Pick a subscription and resource group, enter the repository URL, and choose **Review + create**. It creates:

- a Log Analytics workspace (`<environment>-logs`) for the job's output;
- a Container Apps environment (default `ax-env`);
- a manual Container Apps job (default `ax-index`) running `ghcr.io/garywenneker/ax-slim:<version>-ubuntu` with the command `ax-index-repo`.

The repository URL is stored as a job secret, so a URL with a token for a private repository stays out of plain environment variables. The version field defaults to the newest release. Other fields: branch or tag, the command to run after indexing, the job and environment names, CPU (0.5, 1 or 2 vCPU with twice that in GiB of memory) and the maximum run time. If the resource group already has an environment with the chosen name, the deployment updates it.

The deployment does not start a run. Open the job in the portal and choose **Run now**, or use the `startCommand` and `logsCommand` from the deployment's **Outputs**:

```bash
az containerapp job start --name ax-index --resource-group my-rg
```

The template is `slim/azure/azuredeploy.json` in the repository; you can also deploy it with `az deployment group create --resource-group my-rg --template-file slim/azure/azuredeploy.json --parameters repoUrl=https://github.com/<owner>/<repo>`.

#### Deploy from the command line

`slim/azure/create-job.sh` in the repository creates a manual Container Apps job around it, stores the repository URL as a job secret (so a URL with a token stays out of plain environment variables) and can start the first run. It needs the Azure CLI, logged in with `az login`:

```bash
slim/azure/create-job.sh --resource-group my-rg --environment my-env --location westeurope --repo https://github.com/<owner>/<repo> --version <version> --command "ax status && ax query main" --start
```

`--location` creates the environment when it does not exist yet. Running the script again updates the job. Follow the output with:

```bash
az containerapp job logs show --name ax-index --resource-group my-rg --container ax-index --follow
```

Without the script, create a job from `ghcr.io/garywenneker/ax-slim:<version>-ubuntu` with the command `ax-index-repo` and the environment variables above.

## Limits

An agent in a sandbox or container starts `ax serve --mcp` itself and talks to it over stdio. `ax mcp remote serve` listens on `127.0.0.1` only, so inside a container it is reachable from the same container or pod, not from Container Apps ingress directly.

See `slim/README.md` in the repository for measured sizes and more run examples.
