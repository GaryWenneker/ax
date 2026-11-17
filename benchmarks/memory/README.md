# Memory benchmark

`scripts/memory-bench.sh` scores the coding fixture in `coding.json` and writes `results/<commit>.json`. The default test suite runs the same fixture and does not write that file.

`samples/` is a shape check for the LongMemEval and LoCoMo adapters. Those files are not the public datasets, and their scores are stored separately from a real dataset run. Set `AX_LONGMEMEVAL_PATH` or `AX_LOCOMO_PATH` to a local file to score one. The script does not download either set. A missing file is `skipped`.

Mechanism probes are not part of this harness.
