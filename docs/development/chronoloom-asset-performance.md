# ChronoLoom asset delivery

Issue [#776](https://github.com/underpass-ai/kmp/issues/776) concerns the bytes
and allocation work needed to deliver ChronoLoom's static code. It does not
change memory projection, authorization, viewer intent, selection, or evidence
semantics.

## Delivery contract

The build embeds identity and gzip representations of every browser asset.
Each URL contains the SHA-256 digest of the decoded source:

```text
/assets/<sha256>/<name>
```

A current URL is public because its contents are fixed application code. Its
response carries `Cache-Control: public, max-age=31536000, immutable` and
`Vary: Accept-Encoding`; gzip clients receive `Content-Encoding: gzip`, while
identity clients receive the byte-exact source. Unknown or old versions return
404. The private index and every live memory response still require the viewer
capability and carry `Cache-Control: private, no-store`.

Static response bodies borrow the embedded bytes instead of allocating a copy
per request. The MCP Apps resource is composed at build time and returned as a
static string, preserving the exact self-contained HTML and decoded Three.js
source without doing runtime decompression or assembly.

Persistent HTTP connections were implemented and measured, then removed. They
reduced a cold browser load from 40 connections to 6, but the small server
serializes requests on each connection. Cold first-scene p50 increased from
750.01 ms to 1088.20 ms and warm p50 from 687.87 ms to 847.27 ms. Keeping the
one-request-per-connection behavior preserved browser parallelism. Bundling was
also unnecessary: immutable caching removes static transfer on reload without
changing module boundaries or adding a build framework.

## Reproduction

Build the unchanged baseline and candidate with the same Cargo dev profile,
then run the real HTTP journey serially in a quiet slot:

```bash
python scripts/performance/chronoloom_assets.py \
  tmp/performance-776/baseline/kmp-mcp \
  tmp/performance-776/candidate/kmp-mcp \
  tmp/performance-776/native-allocation-counter.so \
  artifacts/performance-776/measurement-final \
  tmp/performance-776/run --samples 8
```

The fixture contains one about, 32 memories, one dimension and no relations.
Every coordinate has deterministic occurred, observed and ingested clocks. A
browser sample completes only after the page shows exactly `1 about · 32
memories`, the Moment projection contains all 32 entries, the selection total
is 32, no entry is selected, shared-view sync has settled, and two animation
frames have passed. Cold means a fresh server process and browser context; warm
means reload in that context and process. Browser and MCP latency runs are
uninstrumented. Allocation counters run in separate processes, and their
latency is not interpreted.

## Measured controls

Eight samples per side ran serially in a quiet slot on Linux aarch64 with Rust
1.97.1 and headless Chromium 145.0.7632.0 at 1,280 × 900 using SwiftShader.
Values are local informational controls and add no absolute timing gate.

| Measure | Baseline p50 / p95 | Candidate p50 / p95 | Change at p50 |
| --- | ---: | ---: | ---: |
| Cold first scene (ms) | 780.41 / 878.63 | 751.34 / 775.05 | -3.7% |
| Warm first scene (ms) | 714.55 / 772.37 | 690.11 / 720.99 | -3.4% |
| Cold encoded bytes | 993,273 / 993,273 | 317,242 / 317,242 | -68.1% |
| Warm encoded bytes | 1,013,871 / 1,013,871 | 75,318 / 75,318 | -92.6% |
| Browser requested allocation bytes | 25,511,319 / 25,511,831 | 22,608,315 / 22,608,699 | -11.4% |
| Browser peak server RSS (KiB) | 37,892 / 38,000 | 36,092 / 36,260 | -4.7% |
| MCP App first read (ms) | 69.38 / 78.14 | 29.35 / 42.14 | -57.7% |
| MCP App warm read (ms) | 22.56 / 23.44 | 22.98 / 26.19 | +1.9% |
| MCP App requested allocation bytes | 96,282,614 / 96,283,126 | 13,272,975 / 13,272,975 | -86.2% |
| MCP App peak RSS (KiB) | 29,806 / 29,940 | 27,612 / 27,692 | -7.4% |

Cold and warm request counts remain 41 and 40; the change optimizes their
static payloads rather than claiming fewer requests. Candidate cold static
transfer is 262,513 bytes, and warm static transfer is zero. Dynamic transfer
remains nonzero: 54,729 bytes cold and 75,318 bytes warm in the candidate.
MCP response size remains 980,010 bytes and its HTML SHA-256 is identical on
both sides. The slight warm MCP read regression is retained rather than hidden.

Chrome reports cached static responses in the warm request event stream while
their `encodedDataLength` is zero; its `fromDiskCache` flag remains false, so
the evidence claims zero transferred static bytes rather than a particular
cache tier. OS page-cache state is uncontrolled, the sides ran in baseline then
candidate order, SwiftShader does not represent hardware GPU timing, and the
allocation counter reports cumulative allocation requests and requested bytes,
not retained heap. The rejected keep-alive experiment and two invalid fixture
attempts remain in the evidence with explicit exclusion reasons.
