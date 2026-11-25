# syntax=docker/dockerfile:1.7
# Headless ax: CLI + MCP server, no Command Center, no desktop client, no self-update.
# Build from the repository root:
#   docker build -f slim/Dockerfile -t "ax-slim:$(git rev-parse --short HEAD)" .
# The binary is a static musl executable for the build platform (linux/amd64 or linux/arm64).

FROM rust:1.100.0-alpine AS builder
RUN apk add --no-cache build-base cmake perl file
WORKDIR /build
# Extra ax-cli features. `minimal-grammars` links only the core tree-sitter grammars and
# `mimalloc` replaces musl's slower malloc (see slim/README.md). Pass
# `--build-arg AX_FEATURES=mimalloc` to keep every language.
ARG AX_FEATURES="minimal-grammars mimalloc"
# Fat LTO needs about 6 GB of RAM for the final link; `--build-arg AX_LTO=thin` fits in 4 GB
# at the cost of a slightly larger binary.
ARG AX_LTO=fat
# sqlite-vec 0.1.x uses the BSD u_intN_t names, which musl does not define.
ENV CFLAGS="-Du_int8_t=uint8_t -Du_int16_t=uint16_t -Du_int64_t=uint64_t"
COPY . .
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/build/target-slim \
    set -eu; \
    target="$(rustc -vV | sed -n 's/^host: //p')"; \
    CARGO_TARGET_DIR=/build/target-slim CARGO_PROFILE_RELEASE_SLIM_LTO="$AX_LTO" \
        cargo build --locked -p ax-cli \
        --no-default-features --features "$AX_FEATURES" \
        --profile release-slim --target "$target"; \
    install -D -m 0755 "/build/target-slim/$target/release-slim/ax" /out/ax; \
    file /out/ax; \
    file /out/ax | grep -q 'static'; \
    /out/ax version

# Optional runtime with a shell and git (`ax diff`, `ax affected`, `ax ship` call git):
#   docker build -f slim/Dockerfile --target ubuntu -t "ax-slim:$(git rev-parse --short HEAD)-ubuntu" .
FROM ubuntu:26.04 AS ubuntu
RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates git \
    && rm -rf /var/lib/apt/lists/* \
    && useradd --create-home --uid 10001 ax \
    && mkdir -p /workspace && chown ax:ax /workspace
COPY --from=builder /out/ax /usr/local/bin/ax
COPY --chmod=0755 slim/ax-index-repo.sh /usr/local/bin/ax-index-repo
USER ax
WORKDIR /workspace
ENV HOME=/home/ax AX_NO_UPDATE_CHECK=1
ENTRYPOINT ["/usr/local/bin/ax"]
CMD ["serve", "--mcp"]

# Default runtime: distroless static (CA certificates, non-root user, no shell).
FROM gcr.io/distroless/static-debian12:nonroot AS runtime
COPY --from=builder /out/ax /usr/local/bin/ax
WORKDIR /workspace
ENV HOME=/home/nonroot AX_NO_UPDATE_CHECK=1
ENTRYPOINT ["/usr/local/bin/ax"]
CMD ["serve", "--mcp"]
