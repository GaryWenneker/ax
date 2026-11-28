---
title: Headless Linux build
description: Install or build headless ax, the CLI and MCP server without a built-in interface, for Linux servers, sandboxes and containers.
---

The headless build is ax going forward: the CLI and the MCP server, without a built-in interface. It runs anywhere, including places with no screen and no browser: a server, a Vercel Sandbox, an Azure Container Apps job, a CI runner. Its interface is [ax-admin](/guides/remote-admin/), a separate application on your own computer.

:::note[Direction]
The full ax is being phased out. This headless build becomes ax on every platform, with [ax-admin](/guides/remote-admin/) as the only visual interface. Today it is released for Linux x86_64 and arm64 only. On macOS and Windows the only released ax is still the full ax from `install.sh` / `install.ps1`; its CLI and MCP server are the same commands. See [Headless ax and ax-admin](/guides/phase-out/).
:::

It keeps the CLI, MCP over stdio (`ax serve --mcp`), `ax mcp remote serve`, indexing, graph queries, policy, specs, sessions and the memory vault. It leaves out `ax web`, `ax share`, `ax desktop`, `ax ship --watch`, `ax upgrade` and `ax server`. The first five still parse and exit with a message that they are not part of this build; `ax server` is removed entirely.

## What is in it

| Included | Left out |
|---|---|
| CLI: `init`, `index`, `sync`, `watch`, `status`, `query`, `explore`, `node`, `callers`, `impact`, `report`, `insights`, … | `ax web` and `ax share` (the built-in web interface of the full ax and its web assets) |
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

Most of the saving comes from leaving out the built-in web interface, the desktop client and the less common language grammars. Grammar parse tables make up most of a binary's data, so a headless build with every language (`AX_FEATURES=mimalloc slim/build.sh`) is about 71 MB.

Indexing is as fast as with the full ax; see [Speed](#speed).

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

Container images, for `linux/amd64` and `linux/arm64`, are tagged with the release version without the `v`, and the newest release also as `latest`, `latest-ubuntu` and `latest-server`:

| Image | Contents |
|---|---|
| `ghcr.io/garywenneker/ax-slim:<version>` | distroless, non-root, no shell; default command `ax serve --mcp` |
| `ghcr.io/garywenneker/ax-slim:<version>-ubuntu` | Ubuntu 26.04 with `git` and the `ax-index-repo` job entry point |
| `ghcr.io/garywenneker/ax-slim:<version>-server` | the same, with `ax-serve-repo` as entry point: a hosted remote MCP server ([Deploy to the cloud](#deploy-to-the-cloud)) |

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

The headless build indexes as fast as the full ax. In ax 10.1.1, a 660-file Rust project took 20.6 seconds on a 14-core Apple silicon Mac (20.4 seconds with the full ax). Since then, reference resolution batches its database writes and caches node lookups, which brings the same index to 7.0 seconds, with the same graph. A one-file `ax sync` takes about half a second.

Keep the project's `.ax/` folder on the container's own disk or a volume. A bind mount from a macOS host made the same index take over two minutes, and network filesystems such as Azure Files do not give SQLite reliable locks. `slim/README.md` has the measurements.

Several ax processes can share one project: an index, `ax sync`, the MCP server and CLI queries. Writers wait for each other through `.ax/ax.lock`, which the system releases when a process dies, so a stopped container never leaves a lock behind that needs `ax unlock`.

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

## Deploy to the cloud

These buttons run the headless build as a hosted **remote MCP server** for one repository. The `-server` image's entry point, `ax-serve-repo`, clones the repository, runs `ax init` and starts `ax mcp remote serve --bind 0.0.0.0 --port $PORT` behind the platform's HTTPS ingress. Agents and IDEs then connect to:

```text
https://<your service>/projects/project/mcp
Authorization: Bearer <AX_API_KEY>
```

### Azure Container Apps

[![Deploy to Azure](https://aka.ms/deploytoazurebutton)](https://portal.azure.com/#create/Microsoft.Template/uri/https%3A%2F%2Fgetax.wenneker.io%2Fdeploy%2Fazure-slim-server.json)

Log Analytics workspace, Container Apps environment and an always-on Container App with HTTPS ingress. The MCP URL is in the deployment's **Outputs**.

### Render

[![Deploy to Render](https://render.com/images/deploy-to-render-button.svg)](https://render.com/deploy?repo=https://github.com/GaryWenneker/ax)

A web service from `render.yaml` in the repository (plan `starter`). Render generates `AX_API_KEY`; copy it from the service's **Environment** tab.

### Koyeb

[![Deploy to Koyeb](https://www.koyeb.com/static/images/deploy/button.svg)](https://app.koyeb.com/deploy?type=docker&name=ax-mcp&image=ghcr.io%2Fgarywenneker%2Fax-slim%3Alatest-server&ports=8080%3Bhttp%3B%2F&env%5BPORT%5D=8080&env%5BAX_REPO_URL%5D=https%3A%2F%2Fgithub.com%2FOWNER%2FREPO&env%5BAX_API_KEY%5D=CHANGE_ME&env%5BAX_PROJECT_ID%5D=project)

A web service from the image. Replace `AX_REPO_URL` and `AX_API_KEY` in the form before you deploy; the placeholder key is rejected on purpose.

Every deployment asks for:

- **`AX_REPO_URL`**: the git URL to serve. For a private repository, include a read-only token in the URL. Azure stores it as a secret.
- **`AX_API_KEY`**: the bearer key clients send, at least 32 characters (letters, digits and `._~+/=-`). Generate one with `openssl rand -hex 32`. Only its SHA-256 is written to the server configuration.

Optional variables: `AX_REPO_REF` (branch or tag), `AX_API_KEY_WRITE=1` (let the key write memories and policy; read-only otherwise), `AX_PROJECT_ID` (the `project` part of the URL) and `AX_PUBLIC_URL`. The public URL is detected on Azure Container Apps, Render, Koyeb and Railway; set it yourself anywhere else.

The checkout and index live on the container's disk. A restart or redeploy clones and indexes again, so the first requests after a start wait for `ax init`. Nobody can approve a browser (OAuth) sign-in on this server: approving one is done in the Command Center of the full ax, which the headless build does not have. The API key is the only way in.

The container pulls `ghcr.io/garywenneker/ax-slim:<version>-server`; Render and Koyeb use the `latest-server` tag, and Azure lets you pick a version.

Not offered as a button: Vercel and Netlify do not run containers (use [Vercel Sandbox](#vercel-sandbox) instead), Google Cloud Run's button builds from a Dockerfile rather than running a published image, and a Railway button needs a template created in Railway's dashboard. On Railway, a service from the `latest-server` image works the same way.

To index once and run a command instead of serving, use the [Azure Container Apps job](#deploy-with-one-click).

## Run it yourself with Docker or Podman

On your own server or VM, the same `-server` image runs with [Caddy](https://caddyserver.com/) in front of it. Caddy gets an HTTPS certificate for your domain on its own, which the server needs because it only accepts an `https://` address.

| File | Use |
|---|---|
| [`compose.yaml`](/deploy/containers/compose.yaml) | Docker Compose or Podman Compose: the ax server and Caddy |
| [`Caddyfile`](/deploy/containers/Caddyfile) | HTTPS for `AX_PUBLIC_URL`, forwarded to the ax container |
| [`ax.env`](/deploy/containers/ax.env) | Your settings: public address, repository and API key |
| [`ax-mcp.container`](/deploy/containers/ax-mcp.container) | Podman Quadlet: the ax server as a systemd service |
| [`ax-caddy.container`](/deploy/containers/ax-caddy.container) | Podman Quadlet: Caddy as a systemd service |
| [`ax-mcp.network`](/deploy/containers/ax-mcp.network) | Podman Quadlet: the network both containers share |

Before you start, point your domain's DNS at the host and open ports 80 and 443. The files pin the ax release they were published with and an exact Caddy version; change the image version in them to upgrade.

### Docker or Podman Compose

```bash
mkdir ax-mcp && cd ax-mcp
for f in compose.yaml Caddyfile ax.env; do curl -fsSLO "https://getax.wenneker.io/deploy/containers/$f"; done
```

Edit `ax.env`: set `AX_PUBLIC_URL` to your domain (`https://ax.example.com`), `AX_REPO_URL` to the repository and `AX_API_KEY` to the output of `openssl rand -hex 32`. Then start it:

```bash
docker compose up -d
```

With Podman:

```bash
podman compose up -d
```

Rootless Podman may bind ports 80 and 443 only after `sudo sysctl net.ipv4.ip_unprivileged_port_start=80` (add the setting to `/etc/sysctl.d/` to keep it after a reboot). `docker compose logs -f ax` shows the clone and the index. The server is ready when it prints `serving https://…/projects/project/mcp`.

### Podman as a systemd service

Quadlet turns the three unit files into systemd services that start at boot (rootless shown):

```bash
mkdir -p ~/.config/containers/systemd ~/.config/ax-mcp
cd ~/.config/containers/systemd
for f in ax-mcp.container ax-caddy.container ax-mcp.network; do curl -fsSLO "https://getax.wenneker.io/deploy/containers/$f"; done
cd ~/.config/ax-mcp
for f in Caddyfile ax.env; do curl -fsSLO "https://getax.wenneker.io/deploy/containers/$f"; done
```

Edit `~/.config/ax-mcp/ax.env` as above. Rootless Podman may bind ports 80 and 443 only after `sudo sysctl net.ipv4.ip_unprivileged_port_start=80`; to keep services running after you log out, run `sudo loginctl enable-linger "$USER"`. Then start it:

```bash
systemctl --user daemon-reload
systemctl --user start ax-caddy
```

Starting `ax-caddy` starts `ax-mcp` and the network first. `journalctl --user -u ax-mcp -f` shows the clone and the index.

Clients connect to `https://<your domain>/projects/project/mcp` with `Authorization: Bearer <AX_API_KEY>`, the same as for the cloud buttons above. To try it on your own computer, set `AX_PUBLIC_URL=https://localhost`: Caddy then uses its own local certificate, which clients do not trust until you add Caddy's root certificate.

## An interface for the headless build

[ax-admin](/guides/remote-admin/) is the interface of the headless build, and the only visual interface of Ax. It is a separate application that runs on your own computer and manages the environment through `ax mcp remote serve`. Install it there, not in the container:

```bash
curl -fsSL https://getax.wenneker.io/install-admin.sh | sh
```

It runs on Windows, macOS and Linux; on Windows use `install-admin.ps1` (see [Install](/guides/remote-admin/#install)). It connects to a hosted server from the section above with that server's address and `AX_API_KEY`. Its pages cover status, search, nodes, explore, insights, rules, skills, memory and sessions. To re-index, sync and read the MCP call log from it, the key needs the [admin grant](/guides/remote-mcp/#admin-access) in the server's remote MCP configuration; the configuration that `ax-serve-repo` writes does not grant it.

## Limits

An agent in a sandbox or container usually starts `ax serve --mcp` itself and talks to it over stdio. `ax mcp remote serve` listens on `127.0.0.1` by default; pass `--bind 0.0.0.0` only behind an HTTPS ingress or proxy, as `ax-serve-repo` does. It serves plain HTTP.

See `slim/README.md` in the repository for measured sizes and more run examples.
