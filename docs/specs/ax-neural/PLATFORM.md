# Platform matrix

Recorded from the machine that ran the checks. An unrun target stays untested.

| Target | Evidence | Result |
|---|---|---|
| macOS ARM64 (Darwin arm64, rustc 1.100.0) | **runtime** — `cargo test`, ABC, embed/agent eval | suite green |
| macOS x86_64 | **runtime** — Rosetta (`macos-x86_64-runtime.json`) | **124 passed** |
| Linux x86_64 | **runtime** — `podman --arch amd64` Debian (`linux-x86_64-runtime.json`) | **124 passed** |
| Linux ARM64 | **runtime** — Podman Fedora CoreOS (`linux-arm64-runtime.json`) | **124 passed** |
| Windows x86_64 | **runtime** — Wine 11.21 PE (`windows-x86_64-runtime.json`) | **124 passed** (Wine host) |
| Windows ARM64 | **runtime** — Hangover Wine 11.16 PE (`windows-arm64-runtime.json`) | **124 passed** (Hangover on Linux ARM64) |

`cargo tree -p ax-cli` (default features) does not contain candle, burn, ort, tract, or onnx package names. Optional `neural-ort` links ONNX Runtime for bake-off only; production defaults keep neural embedding inactive and `SEMANTIC_WEIGHT=0`.

GitHub Actions were not given new push or schedule triggers.

## Compilation vs runtime (Phase 7)

| Target | Evidence type | Runtime execution of Neural tests? |
|---|---|---|
| macOS ARM64 | **runtime** | YES |
| macOS x86_64 | **runtime** | YES — Rosetta |
| Linux ARM64 | **runtime** | YES — Podman guest |
| Linux x86_64 | **runtime** | YES — `podman --arch amd64` |
| Windows x86_64 | **runtime** | YES — Wine PE (not native Windows OS) |
| Windows ARM64 | **runtime** | YES — Hangover PE on Linux ARM64 (not native Windows OS) |

Do not treat `cargo check --target …` alone as proof of successful Neural **runtime** on that OS/arch. Compatibility-layer hosts (Wine/Hangover/Rosetta/QEMU) are documented explicitly in each JSON evidence file.
