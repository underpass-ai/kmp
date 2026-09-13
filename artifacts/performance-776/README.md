# Performance #776 evidence

See [the report](../../docs/development/chronoloom-asset-performance.md) for the
HTTP/cache contract, measurements, limitations and reproduction command.

Accepted measurements are in `measurement-final/`. `results.json` contains
the p50/p95/min/max summaries for eight samples per side. The four `*.json.gz`
files contain every browser, MCP resource and allocation-control row;
`environment.json` records binary, runner, toolchain, browser, host, fixture and
measurement definitions. Large raw evidence is losslessly gzip-compressed and
can be read with `gzip -dc`.

`measurement-keep-alive-rejected/` preserves the complete experiment that
reused HTTP connections. It is excluded from the accepted implementation
because the resulting request serialization regressed real first-scene
latency. `experiment-provenance.json` identifies the distinct accepted and
rejected executables and environments; `rejected-keep-alive.patch.gz` plus
its exact source and Git blob hashes reconstructs the executable Rust delta.
`measurement-diagnostics.json` records two earlier invalid fixture attempts
and why neither contributed a retained sample.

`baseline-commit.txt`, the binary hashes, `runner-hashes.sha256`,
`source-hashes.sha256`, `binary-sizes.json` and
`measurement-manifest.sha256` identify the measured inputs, executable-size
tradeoff and evidence. No executable, store, credential, target directory or
scratch data is committed.
