# Issue 773 — lexical bridge loading evidence

Measured on 2026-09-14 from base
`0f4b73023378388a42458c602d8909e3d1c1dc3c`, Rust 1.97.1,
Linux/aarch64, on the committed 13,326,412-byte bridge with SHA-256
`556e2ba9657f702f8a33f1b1d4bb402eb5a5bf8c7b65fe5df4b937e5b0bd6712`.
Both binaries used the workspace development profile. The quiet measurement
slot had all other campaign builds paused. These numbers are informational,
not CI thresholds.

## Native startup

`scripts/performance/lexical_bridge_loading.py` alternated the base and
candidate binaries. Each condition discarded three warmups and retained 20
samples. The measured interval is total native process spawn through the MCP
`initialize` response, so it includes embedded-store startup as well as bridge
file loading and parsing. “Cold” requests `POSIX_FADV_DONTNEED` on the bridge;
“warm” reads that whole file immediately before spawning. It does not claim a
machine-wide cold boot or isolate physical I/O.

| bridge cache | binary | first ms | p50 ms | p95 ms | RSS p50 KiB | peak HWM max KiB |
|---|---:|---:|---:|---:|---:|---:|
| cold | base | 259.14 | 275.27 | 281.79 | 39,396 | 51,780 |
| cold | candidate | 52.74 | 66.68 | 78.40 | 39,624 | 39,624 |
| warm | base | 259.35 | 264.37 | 274.97 | 39,372 | 51,780 |
| warm | candidate | 48.20 | 65.58 | 70.68 | 39,622 | 39,624 |

Warm p50 falls 75.2% and cold p50 75.8%. Peak HWM falls 12,156 KiB
(23.5%). Steady RSS does not improve: its 226–250 KiB measured increase is
allocator/page noise around a representation whose exact retained payload is
77 bytes larger. The change targets transient peak memory and CPU work, not
retained heap.

The glibc allocation control covers the full process startup and one
`initialize`, not just the parser. It records allocation request count and
cumulative requested bytes, not live heap. Requests move from 2,916 to 2,912;
requested bytes from 33,295,425 to 19,968,206, a reduction of 13,327,219 bytes.
Instrumented latency is not interpreted.

## Phase attribution

`scripts/performance/lexical_bridge_phases.rs` mirrors the base format-v1
reader and times its work independently. The exact original parser is archived
losslessly in `baseline-lexical-bridge-source.tar.gz`; its source SHA-256 is
`f8115a0d1a0635c8d87647a4188210a5cd8afa8a623fb8865d46e4c088bac292`.
The first three phase samples are warmups and the next 20 form this summary:

| legacy phase | p50 ms | p95 ms |
|---|---:|---:|
| file read | 4.30 | 4.49 |
| decode offsets | 8.94 | 8.99 |
| copy words | 0.41 | 0.48 |
| copy and convert vectors | 41.81 | 42.20 |
| validate UTF-8 and sort order | 8.27 | 8.36 |
| eagerly normalize all vectors | 173.38 | 174.34 |

The reported approximately 45 ms is reproduced here as the vector-copy phase,
not as the whole old reader. On this unoptimized development profile, eager
normalization dominates. The candidate preserves required header, offset,
UTF-8, ordering, exact-length and version validation, owns the original bytes
without word/vector copies, and computes an integer norm only when a queried
row is first used.

## Parity, ownership and packaging

Three native bilingual `kmp_ask` responses using the shipped bridge are exact
JSON matches between binaries. The uncompressed response SHA-256 is
`20571d8bf60b91f43bc5870168958ba574d6662249e0f29e2423548c02b5f405`.
Unit controls additionally compare score `f64::to_bits()` with an independent
integer-cosine oracle for negative values, `-128`, zero norms, a score just
above 0.45 and the maximum u16 vector width. Eight cloned handles race cold
norm initialization repeatedly and return the oracle bits.

The bridge remains format version 1 and the committed asset and checksum are
unchanged. Retained payload for the shipped table is 14,745,575 bytes plus
container metadata, 77 bytes above the copied representation because the
59-byte provenance and 18-byte header remain in the owned buffer. Peak parsing
no longer holds that whole buffer beside copied word and vector sections.
Adding mmap or another packaged layout would introduce platform, validation and
release contracts without a retained-memory benefit, so neither is used.

Raw timing and parity traces are gzip-compressed without loss. Hashes, profile,
sample counts and measurement scopes are in `paired/environment.json` and
`phase-environment.json`; summaries are in `paired/results.json` and
`phase-summary.json`.

The base binary SHA-256 is
`50d56fe04b6db7cbb2ed578b524cd83e5e14913d2d221dc4ee2af84dadc8ee3f`;
the candidate is
`af35ef242ac6067242c96a7c0c96c2e88dcdb550952db2ac2b713bc6dac91629`.
The uncompressed timing JSON SHA-256 was
`8216e23391b6fffc77d68c4ce5fec0bc5d1f9494ea094cb0b74e595a4b29dd13`;
its gzip is
`694db044be5009be81dcb010b821c7cdf53002df23d2fffeb97baecf4c5c3039`.
Both uncompressed parity files had the response hash stated above; their gzip
hashes are `0f72fdab54ae7ddcb3d07f37ec95ab1703b80be396df83aa1e0413fe14b2ccbd`
and `91281a6ce88ed990683fdd683ceaa5a5760631c5ae158caa7260e9861dde218b`.
