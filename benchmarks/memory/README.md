# Memory benchmark

`scripts/memory-bench.sh` scores the coding fixture in `coding.json` and writes `results/<commit>.json`. The default test suite runs the same fixture and does not write that file.

`samples/` is a shape check for the LongMemEval and LoCoMo adapters. Those files are not the public datasets, and their scores are stored separately from a real dataset run. Set `AX_LONGMEMEVAL_PATH` or `AX_LOCOMO_PATH` to a local file to score one. The script does not download either set. A missing file is `skipped`.

Mechanism probes are not part of this harness.

## Ignored measurement tests

The ax-neural measurement tests are `#[ignore]`d and run only on request, for example `cargo test -p ax-neural -- --ignored`. They read frozen inputs (`code-retrieval-frozen.json`, `graph-discovery-independent.json`, `doc-comment-frozen.json`), so an edit to the live source tree does not move their figures. Each freeze script (`freeze-*.py`) rebuilds its input from one git snapshot.

Reports go to `AX_BENCH_OUT_DIR`, or to an `ax-bench` folder in the system temporary directory when it is not set; each test prints the path. `independent_graph_discovery` overwrites the checked-in `docs/specs/ax-neural/GRAPH-DISCOVERY-RESULTS.json` only when `AX_BENCH_RECORD=1` is set. The LoCoMo measurement needs `datasets/locomo10.json`, which is not checked in.
