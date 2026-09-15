# Writer clocks acceptance fixture

`SOURCE.md` and `WRITER-INSTRUCTIONS.md` are the only materials supplied to a
fresh writer. `driver.py` starts one persistent embedded store and records its
handshake, every native response, and every driver error.

Run the writer interactively. Each stdin line is a JSON object with `tool` and
`arguments`; the driver emits the result before reading the next line. Send
`quit` when finished.

```bash
python3 artifacts/writer-clocks-20260915/driver.py \
  --binary /absolute/path/to/kmp-mcp \
  --trace artifacts/writer-clocks-20260915/attempt.native.jsonl
```

For the final-main acceptance run, supply `FRESH-SOURCE.md`,
`FRESH-WRITER-INSTRUCTIONS.md`, and `FRESH-SEED-REQUEST.json` with the matching
driver options. The seed creates only source-backed older context in a new
store; it is recorded in the native trace before the writer's first call. The
fresh writer receives the source and instructions, not the seed payload or any
expected response.

`fresh-final-main-metrics.json` is the initially published derived measurement;
`fresh-final-main-metrics-v1.json` preserves it with its known limits.
`fresh-final-main-metrics-v2.json` is authoritative: it uses the stated
nearest-rank formula and separates hidden preparation from driver-emitted
responses. None of these counts measures actual host context.

## Recorded independent writer

`fresh-luna-report.md` and `fresh-luna-native.jsonl.gz` retain a fresh
`gpt-5.6-luna` writer's choices, native results and errors. The model received
only the source, writer instructions and driver invocation; it did not receive
the authored regression or expected answers. It then consulted KMP's native
guide. The driver made no model calls. This is one observed writer run, not a
controlled comparison of models or a claim of improved comprehension.

The trace contains the handshake and 21 tool calls, including two rejected
writes, the review response, a failed suggested context expansion, the accepted
receipt, eight completed occurrence-axis pages and one observation-axis page.
Its header records SHA-256 digests for the binary, source, instructions and
driver. `fresh-luna-manifest.json` records the trace digest and model settings.
The source and driver contain no golden KMP payload or expected response.

This run used the pre-fix runtime. The suggested `kmp_wake` addressed an about
that existed only in the proposed packet and returned `not_found`. The trace
preserves that defect; subsequent runtime regression tests exercise its fix.

The writer used its current observation time, which KMP defaulted to ingestion.
That is distinct from modeling the investigator's historical receipt at 12:00.
The authored native regression separately checks that historical observation
timeline and a typed late evidence attachment at 12:10. This fresh writer
represented the late association as another observation, so it does not
independently validate the typed attachment behavior. Neither check establishes
source coverage, semantic comprehension or independent reader performance.
