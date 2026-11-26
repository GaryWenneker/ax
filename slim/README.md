# ax slim — headless Linux build

A smaller build of the same `ax` binary for Linux sandboxes and containers
(Vercel Sandbox, Azure Container Apps, CI runners). It is not a fork: it is
`crates/ax-cli` built with `--no-default-features --features "minimal-grammars mimalloc"`
and the size-focused `release-slim` profile. The normal ax distribution is
unchanged.

## Install

Releases from v10.1.1 on publish `ax-slim-linux-x64.tar.gz` and
`ax-slim-linux-arm64.tar.gz` (in `SHA256SUMS`) and the images
`ghcr.io/garywenneker/ax-slim:<version>` (distroless) and
`ghcr.io/garywenneker/ax-slim:<version>-ubuntu` (with `git` and
`ax-index-repo`), for `linux/amd64` and `linux/arm64`.

```bash
curl -fsSL https://getax.wenneker.io/install-slim.sh | sh
```

`install-slim.sh` (repository root, served from the website) resolves the
newest release that has the asset for the host's CPU through the GitHub API,
verifies the checksum and installs one binary to `/usr/local/bin` (root) or
`~/.local/bin`. It works with curl or wget and needs `tar` and `sha256sum`
(or `shasum`). Options: `--version`, `--bin-dir`, `--force` (replace a full ax)
and `--uninstall`. The Release workflow builds the binaries natively on x64 and
arm64 runners (`build-slim`) and pushes the images from those same binaries
(`publish-slim-image`, `slim/Dockerfile.release`).

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
AX_FEATURES=mimalloc slim/build.sh
```

### Cargo features

`web`, `desktop` and `self-update` are on by default in `crates/ax-cli/Cargo.toml`;
the slim build turns them off and turns on the last two:

| Feature | Controls |
|---|---|
| `web` | `ax-web` dependency: `web`, `share`, `ship --watch`, `server`, replication start-up |
| `desktop` | `ax-desktop-client` dependency: `ax desktop` |
| `self-update` | `ax upgrade` and the post-command update check |
| `minimal-grammars` | links only the core grammars listed above |
| `mimalloc` | uses mimalloc as the global allocator instead of the platform malloc |

## Size

Measured from one build of this checkout (ax 10.1.0), sizes in bytes:

| Binary | Build | Size |
|---|---|---|
| Full ax, macOS arm64 | `cargo build --release -p ax-cli` | 161,708,128 |
| Headless features only, macOS arm64 | `cargo build --release -p ax-cli --no-default-features` | 110,418,256 |
| Headless, `release-slim`, all grammars, macOS arm64 | `cargo build --profile release-slim -p ax-cli --no-default-features` | 71,266,960 |
| Headless, `release-slim`, `minimal-grammars`, macOS arm64 | `cargo build --profile release-slim -p ax-cli --no-default-features --features minimal-grammars` | 31,157,520 |
| **Same plus `mimalloc` (the slim default), macOS arm64** | `… --features "minimal-grammars mimalloc"` | **31,277,024** (5.2× smaller) |
| Slim default, Linux arm64, static musl, thin LTO | `AX_LTO=thin slim/build.sh` | 36,047,056 |
| Slim default, Linux x86_64, static-pie musl, thin LTO | `AX_LTO=thin slim/build.sh linux/amd64` | 41,336,176 |
| Release v10.1.1 `ax-slim-linux-arm64` (fat LTO, CI) | Release workflow `build-slim` | 33,163,280 |
| Release v10.1.1 `ax-slim-linux-x64` (fat LTO, CI) | Release workflow `build-slim` | 37,821,680 |
| Distroless image (`runtime` target) | `docker build -f slim/Dockerfile --build-arg AX_LTO=thin .` | 39.1 MB |
| Ubuntu 26.04 image with `git` (`ubuntu` target) | `--target ubuntu` | 268.4 MB |

`release-slim` (workspace `Cargo.toml`) is `opt-level = "s"`, fat LTO,
`codegen-units = 1`, `panic = "abort"` and stripped symbols.

## Speed

Benchmark: `ax index . --force` on a copy of this repository's `crates/`
(625 Rust files, about 98,000 references to resolve), median of three runs.
Wall, user and system seconds from `/usr/bin/time`.

| Binary (macOS arm64, 14 cores) | Wall | User | Sys |
|---|---|---|---|
| Full ax, `--release` (`opt-level = 3`) | 20.4 | 11.1 | 15.4 |
| Slim, `release-slim` (`opt-level = "s"`) | 20.6 | 11.3 | 15.5 |
| Slim plus `mimalloc` (the slim default) | 20.3 | 11.0 | 15.3 |
| Slim with `opt-level = "z"` | 23.0 | 14.5 | 15.5 |

The measurements above are for ax 10.1.1. After 10.1.1, reference resolution
no longer commits once per reference and no longer queries SQLite for every
lookup. The same benchmark (660 indexed files, slim default, macOS arm64,
median of three; edge, node and unresolved rows identical to 10.1.1):

| Change | Wall | User | Sys | Peak RSS |
|---|---|---|---|---|
| ax 10.1.1 | 20.6 | 11.3 | 15.2 | 224 MB |
| Resolved references deleted in one transaction | 17.4 | 10.3 | 12.9 | |
| Resolved edges written in one transaction | 14.3 | 9.4 | 10.6 | |
| Node lookups memoized during resolution | **7.0** | 4.3 | 3.7 | 266 MB |

A one-file `ax sync` stays at 0.5 s. With an MCP server attached, the WAL
left after a full index went from 13.8 MB to 0.75 MB (`wal_checkpoint(TRUNCATE)`
without waiting, `journal_size_limit` 64 MB). These changes are in shared
crates, so the full build gets them too. The Linux container numbers below
are from 10.1.1 and were not re-measured.

| Binary (Linux arm64 static musl, 4 vCPU container, project on the container's own disk) | Wall | User | Sys |
|---|---|---|---|
| Slim, musl malloc | 36.8 | 21.4 | 15.6 |
| Slim plus `mimalloc` (the slim default) | 35.6 | 18.9 | 16.4 |

Where the earlier gap came from: before this change, the same index took
66 s with the full build, 78 s with `"s"` and 143 s with `"z"`. A profile
(`sample` on macOS) showed most CPU time in regex compilation: the Laravel
resolver in `ax-resolution` compiled two regexes for every reference it
checked, in every project. Regex compilation is CPU-heavy and sensitive to the
optimisation level, which is why `"s"` and `"z"` fell behind. Those regexes are
now compiled once, which makes every build faster, the full one included.

In 10.1.1, what was left was mostly reference resolution, which spent its
time in SQLite (system time above user time; `pwrite`, `fsync` and `pread`
led the profile): one commit and several queries per reference. That is the
part the table above removes.
Measured and not taken, because they did not pay off:

- `opt-level = 3` for the C libraries (SQLite, sqlite-vec, tree-sitter,
  zstd): 20.5 s, +0.8 MB.
- The same plus `ax-resolution`, `ax-db`, `ax-extraction`, `ax-graph`, sqlx
  and `regex-automata`: 20.3 s, +1.3 MB.
- `opt-level = 3` for everything: 20.3 s, +7.2 MB.
- `opt-level = "z"`: 4.4 MB smaller but 13% slower (more user CPU in parsing).

`mimalloc` saves 12% user CPU in the musl container at a cost of 0.1 MB.
On macOS the system allocator is already fast, so it makes no difference
there.

On a bind mount from macOS into the Podman VM, the same index took 124 s
with either allocator: SQLite's `fsync` on the shared folder dominates. Keep
the project (or at least `.ax/`) on the container's own disk or a volume.

The x86_64 build targets the baseline `x86-64` CPU. Raising it (for example
`-C target-cpu=x86-64-v3`) would only speed up CPU-bound parts, which are now
a minority of indexing time, and would make the binary crash with an illegal
instruction on hosts without AVX2. It was not measured, because the x86_64
build here runs under emulation.

## Concurrency

Several ax processes can share one project's `.ax/`: an indexer, `ax sync`,
the MCP server, the watcher and CLI queries. Writers take `.ax/ax.lock`, an
advisory `flock` on a file that is never deleted, so the kernel releases it
when a process dies (also after `kill -9` or a stopped container) and a
leftover file never blocks anyone. Readers use SQLite's WAL and do not wait
for writers. `scripts/slim-concurrency.sh [AX_BINARY] PROJECT_DIR` checks this
across processes: reads during an index, a second writer waiting for the
first, and a new writer right after `kill -9`.

Keep `.ax/` on a local disk or a container volume. SQLite's WAL needs shared
memory and working file locks, which network filesystems (Azure Files/SMB,
NFS) do not reliably provide.

## Memory

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
cargo build --locked -p ax-cli --no-default-features --features "minimal-grammars mimalloc" --profile release-slim
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

A sandbox is a Linux microVM (Ubuntu 26.04), so the static binary runs
directly; no image is needed. With the `sandbox` CLI (`npm i -g sandbox`,
then `sandbox login`):

```bash
sandbox create --name ax-test --timeout 30m
```

```bash
sandbox exec --sudo ax-test -- sh -c "curl -fsSL https://getax.wenneker.io/install-slim.sh | sh"
```

```bash
sandbox exec ax-test -- git clone --depth 1 https://github.com/<owner>/<repo> /vercel/proj
```

```bash
sandbox exec -w /vercel/proj ax-test -- bash -c "time ax init . && ax status"
```

Point the agent's MCP configuration at `ax serve --mcp` (stdio), started in
the project directory. `sandbox remove ax-test` deletes the sandbox. The guide
on the website (Guides → Headless Linux build) has the full walkthrough,
including an MCP check and the SDK call.

### Azure Container Apps

`slim/azure/create-job.sh` creates (or updates) a manual job from the
`-ubuntu` image. The job runs `ax-index-repo` (`slim/ax-index-repo.sh`), which
clones `AX_REPO_URL`, runs `ax init` and then `AX_JOB_COMMAND`. The repository
URL is stored as a job secret.

```bash
slim/azure/create-job.sh --resource-group my-rg --environment my-env --location westeurope --repo https://github.com/<owner>/<repo> --version <version> --start
```

```bash
az containerapp job logs show --name ax-index --resource-group my-rg --container ax-index --follow
```

## Limits

- `ax mcp remote serve` binds to `127.0.0.1`. Inside a container it is
  reachable only from the same container or pod, so expose it through a
  sidecar proxy or tunnel; Container Apps ingress cannot reach it directly.
- GHCR packages can start out private on their first publish. If Azure cannot
  pull `ghcr.io/garywenneker/ax-slim`, set the package to public in its GitHub
  package settings (or give the job registry credentials).
- `panic = "abort"`: a panic in a background task ends the process instead of
  being contained. Run long-lived servers under a restart policy.
- `jsonwebtoken`/`octocrab` still bring in `aws-lc-rs` next to `ring` (two
  crypto backends, about 3 MB). Unifying them would change the full build, so
  it is left as is here.
