# Performance evidence for #766

This directory records the before/after evidence for reusing the immutable
`Cl100kEstimator` in the MCP recall projection. The baseline is main at
`0f4b73023378388a42458c602d8909e3d1c1dc3c`; the candidate changes the runtime
fixture-backend projection caller from `new()` to `shared()`. The test-only
budget helper follows the same shared path, while its normalization oracle
keeps a fresh estimator for an independent comparison.

The correctness controls run the focused MCP projection tests, the full
`kmp-mcp` library tests, the proto-mapping recall tests, Clippy with warnings
denied, and the MCP architecture gate. The parallel projection regression uses
24 concurrent calls and requires complete JSON equality with a serial result.

Latency evidence is collected only in a quiet measurement slot. The direct
fixture-backend runner records first-use latency in a fresh process separately
from 20 steady-state samples for three synthetic trial inputs. The fixture
backend ignores the seeded corpus, so these are three independent trials over
the built-in Wake and Ask contract responses; the input sizes are retained as
reproducible controls and are not presented as selected-entry shapes. No model
or network call is involved. The allocation runner uses the existing native
allocation counter and one-query versus five-query processes so first-use
requests remain separate from additional warm-query requests. Instrumented
timings are not interpreted.

Reproduction commands used for the committed controls:

```text
python3 scripts/performance/fixture_tokenizer_control.py \
  target/performance-766/baseline/kmp-mcp \
  target/performance-766/candidate/kmp-mcp \
  artifacts/performance-766/fixture-final \
  tmp/performance-766/fixture-final

cc -shared -fPIC -O2 -Wall -Wextra -Werror \
  -o tmp/performance-766/counter.so scripts/performance/native_allocation_counter.c
python3 scripts/performance/fixture_tokenizer_allocations.py \
  target/performance-766/baseline/kmp-mcp \
  target/performance-766/candidate/kmp-mcp \
  tmp/performance-766/counter.so \
  artifacts/performance-766/fixture-allocations \
  tmp/performance-766/fixture-allocations
```

The timing report is descriptive evidence, not a CI threshold or acceptance
gate. Raw traces are compressed losslessly after validation; scratch stores stay
under `tmp/` and are removed after the measurement run.
