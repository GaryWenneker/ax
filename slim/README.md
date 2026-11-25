# ax slim — headless Linux build

A smaller build of the same `ax` binary for Linux sandboxes and containers
(Vercel Sandbox, Azure Container Apps, CI runners). It is not a fork: it is
`crates/ax-cli` built with `--no-default-features --features minimal-grammars`
and the size-focused `release-slim` profile. The normal ax distribution is
unchanged.

## What is in it

| Included | Left out |
|---|---|
| CLI: `init`, `index`, `sync`, `watch`, `status`, `query`, `explore`, `node`, `callers`, `impact`, `report`, `insights`, … | `ax web` and `ax share` (embedded Command Center and its web assets) |
| MCP over stdio (`ax serve --mcp`) and `ax mcp remote serve` (HTTP) | `ax desktop` (native wgpu/egui client) |
| Policy, skills, specs, memory vault, sessions, `preflight` | `ax ship --watch` (dashboard) |
| `ax ship --evaluate` / `--ci`, `diff`, `affected` (need `git` on `PATH`) | `ax upgrade` and the update notice (rebuild the image instead) |
| Neural ranking (pure-Rust runtime, no ONNX) | `ax server …` (standalone knowledge node; it lives in ax-web) |

`web`, `share`, `desktop`, `ship --watch` and `upgrade` still parse and exit 1
with a message that they are not included in this build. `ax server` is
removed entirely, so the CLI reports it as an unknown subcommand.

### Languages

`minimal-grammars` links only the core tree-sitter grammars: Rust, Python, Go,
Java, JavaScript/JSX, TypeScript/TSX, C, PHP, Ruby, Lua, YAML, XML, Vue,
Svelte, Astro, Properties, Liquid and Twig. Kotlin, C#, C++, Swift, Dart,
Scala, Luau, Objective-C, R, Razor and Pascal files are still listed as files,
but get no symbols or call edges. Those grammars are about 40 MB of parse
tables. To keep every language, build without the feature:

```bash
AX_FEATURES= slim/build.sh
```

### Cargo features

All on by default in `crates/ax-cli/Cargo.toml` except `minimal-grammars`:

| Feature | Controls |
|---|---|
| `web` | `ax-web` dependency: `web`, `share`, `ship --watch`, `server`, replication start-up |
| `desktop` | `ax-desktop-client` dependency: `ax desktop` |
| `self-update` | `ax upgrade` and the post-command update check |
| `minimal-grammars` | off by default; links only the core grammars listed above |

## Size

Measured from one build of this checkout (ax 10.1.0), sizes in bytes:

| Binary | Build | Size |
|---|---|---|
| Full ax, macOS arm64 | `cargo build --release -p ax-cli` | 161,708,128 |
| Headless features only, macOS arm64 | `cargo build --release -p ax-cli --no-default-features` | 110,418,256 |
| Headless, `release-slim`, all grammars, macOS arm64 | `cargo build --profile release-slim -p ax-cli --no-default-features` | 71,266,960 |
| **Headless, `release-slim`, `minimal-grammars`, macOS arm64** | `cargo build --profile release-slim -p ax-cli --no-default-features --features minimal-grammars` | **31,157,504** (5.2× smaller) |
| Same, Linux arm64, static musl, thin LTO | `AX_LTO=thin slim/build.sh` | 35,905,656 |
| Same, Linux x86_64, static-pie musl, thin LTO | `AX_LTO=thin slim/build.sh linux/amd64` | 41,182,480 |
| Distroless image (`runtime` target) | `docker build -f slim/Dockerfile --build-arg AX_LTO=thin .` | 39.1 MB |
| Ubuntu 26.04 image with `git` (`ubuntu` target) | `--target ubuntu` | 268.4 MB |

`release-slim` (workspace `Cargo.toml`) is `opt-level = "s"`, fat LTO,
`codegen-units = 1`, `panic = "abort"` and stripped symbols. Trade-offs that
were measured and not taken:

- `opt-level = "z"` saves another 4.4 MB (26.8 MB) but indexing 625 Rust files
  took 143 s instead of 78 s.
- With `"s"`, the same index takes 78 s against 66 s for the full
  `opt-level = 3` build.

Fat LTO needs about 6 GB of memory for the final link. In a smaller container
VM, pass `AX_LTO=thin`; the Linux numbers above were built that way.

## Build

Static Linux binary through a container (Docker with BuildKit, or Podman),
copied to `slim/dist/ax-linux-<arch>`:

```bash
slim/build.sh
```

```bash
slim/build.sh linux/amd64
```

The second form builds x86_64 on an arm64 machine through emulation, which is
slow; on an x86_64 host or CI runner the first form already produces x86_64.

Container image (default runtime: distroless static, non-root, no shell),
tagged with the commit:

```bash
docker build -f slim/Dockerfile -t "ax-slim:$(git rev-parse --short HEAD)" .
```

Runtime with a shell and `git` (Ubuntu 26.04), needed for `diff`, `affected`
and `ship`:

```bash
docker build -f slim/Dockerfile --target ubuntu -t "ax-slim:$(git rev-parse --short HEAD)-ubuntu" .
```

Without containers, on a Linux host with Rust installed:

```bash
cargo build --locked -p ax-cli --no-default-features --features minimal-grammars --profile release-slim
```

That binary links glibc dynamically. For a static binary add
`--target x86_64-unknown-linux-musl` (or `aarch64-unknown-linux-musl`) after
`rustup target add`, with a musl C toolchain and the `CFLAGS` from the
Dockerfile (sqlite-vec uses BSD integer type names that musl lacks).

## Run

The images run as a non-root user (uid 65532 in distroless). A bind-mounted
checkout must be writable by the container user, so on Linux with Docker run
as your own uid and point `HOME` at a writable directory:

```bash
docker run --rm --user "$(id -u):$(id -g)" -e HOME=/tmp -v "$PWD:/workspace" "ax-slim:$(git rev-parse --short HEAD)" init
```

```bash
docker run --rm --user "$(id -u):$(id -g)" -e HOME=/tmp -v "$PWD:/workspace" "ax-slim:$(git rev-parse --short HEAD)" query "where is the config loaded"
```

MCP over stdio for an agent that launches ax itself (the image's default command):

```bash
docker run --rm -i --user "$(id -u):$(id -g)" -e HOME=/tmp -v "$PWD:/workspace" "ax-slim:$(git rev-parse --short HEAD)"
```

The index lives in `<project>/.ax/`; user-level state (memories, settings)
goes to `$HOME/.ax`. Mount a volume at `HOME` to keep it between runs.

### Vercel Sandbox

A sandbox is a Linux VM, so the static binary runs directly; no image is
needed. Upload `slim/dist/ax-linux-x86_64` (or `-aarch64`, matching
`uname -m` in the sandbox) into the sandbox and put it on `PATH`:

```bash
install -m 0755 ax-linux-x86_64 "$HOME/.local/bin/ax"
```

```bash
ax init && ax status
```

Point the agent's MCP configuration at `ax serve --mcp` (stdio), started in the
project directory. The build never contacts the update server.

### Azure Container Apps

Push the image (with a commit or version tag) to a registry the app can pull
from, then run it as a job that indexes a mounted share (Azure Files) or a
cloned repository:

```bash
az containerapp job create --name ax-index --resource-group my-rg \
  --environment my-env --trigger-type Manual \
  --image "myregistry.azurecr.io/ax-slim:<commit>" \
  --command "/usr/local/bin/ax" --args "index /workspace"
```

Use the `ubuntu` target when the job needs `git` or a shell.

## Limits

- `ax mcp remote serve` binds to `127.0.0.1`. Inside a container it is
  reachable only from the same container or pod, so expose it through a
  sidecar proxy or tunnel; Container Apps ingress cannot reach it directly.
- `panic = "abort"`: a panic in a background task ends the process instead of
  being contained. Run long-lived servers under a restart policy.
- `jsonwebtoken`/`octocrab` still bring in `aws-lc-rs` next to `ring` (two
  crypto backends, about 3 MB). Unifying them would change the full build, so
  it is left as is here.
