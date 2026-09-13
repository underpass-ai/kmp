# Reproducing KMP and ChronoLoom performance

Issue [#765](https://github.com/underpass-ai/kmp/issues/765) supplies one local
runner over synthetic stores. The reported Fable timings (Wake 300–365 ms, Ask
475–520 ms, viewer about 946 ms) came without executable provenance or raw
traces. They are hypotheses, not the baseline or a CI threshold.

## Existing controls

The runner complements these committed controls; it does not replace their
focused semantic or allocation checks:

| Control in `scripts/performance/` | Measured boundary |
| --- | --- |
| `temporal_body_selection.py`, `recall_body_selection.py` | Paired native temporal/Wake/Ask admission and full-result parity |
| `dimensional_lookup.py`, `dimensional_read_parity.py` | Dimension lookup and selector parity |
| `node_batch_framing.py` | Bounded node framing and snapshot handling |
| `visual_projection_cache.py` | Revision-bound visual projections |
| `chronoloom_scene.py`, `chronoloom_scene_control.js`, `chronoloom_scene_checks.js` | Isolated real Three.js scene allocation, render and disposal; injected scene, no HTTP startup |
| `lexical_index_cache.py`, `lexical_index_parity.py`, `lexical_index_writes.py` | Native revision-bound lexical-index reuse and write invalidation |
| `lexical_index_allocations.py`, `native_allocation_control.py`, `native_allocation_counter.c` | Separate Linux/glibc allocation-request controls |

Existing reports are `temporal-read-performance.md`,
`dimensional-read-performance.md`, `visual-projection-performance.md`,
`chronoloom-scene-performance.md`, and `lexical-index-performance.md` in this
directory. Their profiles, fixture shapes and scopes differ: do not combine
their medians into an end-to-end latency.

## Run

Build the compared revisions with the same toolchain, features and Cargo
configuration. Preserve each binary before building the next revision. The
runner records its SHA-256; `--commit` and `--profile` are explicit declarations
of its source and build provenance. Inside the maintainer's workspace the
parent `.cargo/config.toml` also applies. Do not override it with profile
environment variables.

```sh
uv venv tmp/performance-venv
uv pip install --python tmp/performance-venv/bin/python tiktoken==0.14.0 playwright==1.58.0
tmp/performance-venv/bin/python -m playwright install chromium
cargo build --locked -p kmp-mcp
tmp/performance-venv/bin/python scripts/performance/baseline.py \
  target/debug/kmp-mcp artifacts/performance-baseline/run-1 tmp/baseline-run-1 \
  --commit FULL_SOURCE_COMMIT --profile 'dev, inherited workspace Cargo configuration' \
  --samples 20 --warmups 2 --browser --browser-samples 5
```

The output and scratch directories must be new. The runner cleans only the
scratch directory it created. All stores are generated there; it does not open
the user's memory. Failure traces are retained as diagnostics and must be
excluded from reported measurements. Use `--samples 1 --warmups 0 --shapes small
--browser-samples 1` for a smoke test before reserving an otherwise idle host.

Four bounded shapes cover 8 entries/256-byte sources, 32 entries/1-KiB sources,
64 entries with up to 8 outgoing proof edges, and 8 entries/32-KiB sources.
Coordinates span occurred, observed and ingested clocks and two dimensions.
The current positive fixture has no expiry; historical navigation explicitly
selects the observed clock. Each shape is seeded and closed, then reopened in
a fresh process. OS caches are not flushed.

Wake, Ask, Goto and Forward each record one first-operation sample, optional
warmups and the requested warm samples. Every repeated complete native result
must equal its first result, and positive recall must contain evidence.
Sequential `kmp_write_memory` calls create distinct accepted memories after
the read controls. Startup is reported separately from read latency.

The browser path uses the actual HTTP server and bundled application. It
measures MCP open through the first drawn nonempty scene, then reloads within
one browser context, and finally measures get-state/apply-focus-and-trace
through the browser adopting that revision. Completion requires a settled
loading indicator, matching about, revision, selection and proof trace, followed
by two animation frames. A new context has a cold browser cache; server and OS
caches may already be warm. One first-open sample is not a stable p95.

The waterfall retains requests, response sizes and Resource Timing entries,
including dynamic requests and view polling. No `networkidle` heuristic waits
for a long poll to stop. MCP and HTTP call counts are reported separately.
Chromium uses software rendering in this environment; these figures describe
that environment, not desktop GPU performance.

## Accounting and attribution

`environment.json` names the source commit, binary and runner hashes, profile,
hardware, Python, Chromium, concurrency and sample counts. Raw JSON-RPC
responses are kept losslessly compressed. `manifest.json` gives compressed
and original hashes. The named encoder is `cl100k_base` from tiktoken 0.14.0,
counting compact complete JSON-RPC responses after native latency measurement.
These are reference transport token counts, not billed model input.

Native CPU comes from Linux process ticks, at the recorded clock frequency.
`VmHWM` is the server's lifetime high-water mark; it is not a per-call heap
delta. Browser CDP metrics are separate cumulative browser metrics. Linux
`rchar/wchar` and syscall counters include pipes and cached I/O;
`read_bytes/write_bytes` are process storage accounting. None is presented as
SQLite cache hits, SQLite page reads or a physical device trace. SQL statement
counts are unavailable in this runner; MCP calls and HTTP requests are exact.

Embedded Wake/Ask now expose their existing application timing breakdown at
the opt-in tracing target `kmp_mcp::query_phases=debug`. The runner saves these
events in `phases.json`: graph/catalogue and selected-node loading, selected
detail loading, bundle assembly, role count and materialization batch size.
These phases exclude ranking, prose rendering, telemetry and transport. Their
sum is not the complete latency. No timings are added to the MCP contract.

Allocation measurement runs separately, because instrumentation changes time:

```sh
cc -shared -fPIC -O2 -Wall -Wextra -Werror \
  scripts/performance/native_allocation_counter.c -o tmp/native-allocation-counter.so
tmp/performance-venv/bin/python scripts/performance/baseline.py \
  target/debug/kmp-mcp artifacts/performance-baseline/allocations tmp/baseline-allocations \
  --commit FULL_SOURCE_COMMIT --profile 'same dev build; allocation instrumentation' \
  --samples 5 --warmups 0 --allocation-counter tmp/native-allocation-counter.so
```

The preload counter covers native process startup, reads and writes, excluding
seeding and the Python/browser processes. It reports allocation requests and
cumulative requested bytes, not peak live allocation. Do not interpret timings
from this run. Run one and five samples separately when isolating marginal
repeated-work allocations; the individual hotspot controls provide narrower
attribution.

After collecting comparable uninstrumented runs, verify complete read parity:

```sh
python3 scripts/performance/baseline_compare.py \
  artifacts/performance-baseline/before artifacts/performance-baseline/after
```

Performance remains informational. Semantic differences, missing proof or
failed journeys stop the runner instead of being reported as faster queries.
