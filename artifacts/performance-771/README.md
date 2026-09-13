# Performance #771 evidence

See [the report](../../docs/development/lexical-index-performance.md) for the
cache contract, measurements, limitations and reproduction commands.

Accepted measurements:

- `phases.json`: current executable, separate PMI/BM25/cache-hit/ranking phases;
- `native-final/results.json`: original/candidate native ABBA, four shapes,
  40 warm samples per side and shape, 384 complete equal results including
  first queries and warmups;
- `native-parity/summary.json`: 64 complete results per binary, including
  nine historical continuation pages;
- `allocations/results.json`: startup plus one/five-query allocation controls;
- `writes/summary.json`: 24 edits and 48 post-edit reads per side; all results
  equal across binaries and no kernel-table mutations during Ask. The full
  measured rows are `writes/results.json.gz`.

Every runner directory contains the complete request/response traces, synthetic
inputs and environment/binary hashes. Large files are losslessly gzip-compressed;
`compression-manifest.json` records both hashes and sizes. Read with `gzip -dc`.
`measurement-manifest.json`, `source-hashes.json` and `measured-rust.patch.gz`
identify the measured code and the release-only rebase. No store, binary,
target directory or credential is part of this evidence.

`ci-code-6a0a0100.json` records the green CI for the final executable Rust code
and tests. The PR reruns its checks for the subsequent runner/report/evidence
commit. `quality-gate.log.gz` records the complete local gate.

Diagnostic files are retained separately and are excluded from the accepted
performance comparison. `native/` and `native-run.log.gz` are the stopped run whose
fixture had expired entries and returned UNKNOWN. `native-parity-invalid-axis/`
and its log record a runner request that used the unsupported literal
`axis: default`; the final runner correctly omits the field. Earlier mapping
logs record fixture/compilation corrections. `ci-first-failure.log.gz` records
an overbroad test expectation for a supersession marker outside the retained
answer path; the corrected test retains the positive marker assertion for the
matching question and checks that the replaced entry is never cited.
