# Baseline evidence for #765

This is a reproducible reference measurement, not a claimed optimization.
The reference binary is main `0f4b73023378388a42458c602d8909e3d1c1dc3c`;
the instrumented source is `f0a9ad11bb16a8fcc4f51b7445c518e305cd3f76`.
Their only runtime difference is opt-in existing query-phase tracing.
The final runner/fixture source is `13931b85ad714914443a282d8636ad4c160ae7d2`.
These executables do not include the later #766/#773/#776 improvements.
Each run's environment records exact executable, runner and helper hashes.

Both builds use dev/sqlite with the inherited workspace Cargo profile, on
Linux aarch64 (20 logical CPUs, 127,535,148 KiB RAM). Browser controls use
Playwright 1.58.0 / Chromium with SwiftShader. This is not a release-build or
physical-GPU benchmark. Read the complete accounting boundaries and commands
in `docs/development/performance-baseline.md`.

## Sampling and correctness

The uninstrumented campaign ran before then after, with other campaign builds
paused: one first-operation sample, two excluded warmups and eight warm
samples per operation and shape. It did not flush OS caches. At n=8 the
nearest-rank p95 is the maximum; these tails are descriptive, not a stable
population estimate. The CLI default remains 20 samples for future runs.
There were 176 read responses and 32 accepted writes per side. All 16 distinct
complete read results matched across independently seeded binaries, and every
repeated read matched its first response. Raw traces retain all samples.

Browser controls per shape and side comprise one fresh-context open, five
reloads and five focus/trace revisions. All 88 journeys reached the required
nonempty scene, state revision, selection and trace without browser errors.
Open includes one MCP call; focus/trace includes get-state and apply-intent.
Timing ends after state adoption and two animation frames. The HTTP waterfall
is retained through that boundary. CPU tick resolution is 10 ms; short
navigation CPU values are correspondingly coarse.

## Native warm results

All times are milliseconds. CPU is the total of eight warm calls; VmHWM is
the after-process lifetime high-water mark, not an operation heap delta.
Full first-operation/startup, I/O, token and write data are in `results.json`
and the raw traces of each run.

| Shape / operation | Before p50 / p95 | After p50 / p95 | After CPU total | After VmHWM KiB | After compact response tokens |
| --- | ---: | ---: | ---: | ---: | ---: |
| small / wake | 105.59 / 107.50 | 104.29 / 104.64 | 840 | 61488 | 8112 |
| small / ask | 154.82 / 155.52 | 153.21 / 153.96 | 1220 | 61816 | 7479 |
| small / goto | 5.71 / 7.75 | 4.77 / 5.72 | 30 | 62072 | 1600 |
| small / forward | 5.60 / 7.92 | 5.01 / 6.13 | 30 | 62072 | 1578 |
| medium / wake | 600.84 / 604.97 | 616.09 / 616.84 | 4920 | 64940 | 35426 |
| medium / ask | 798.46 / 800.68 | 808.67 / 811.01 | 6450 | 66056 | 11079 |
| medium / goto | 8.14 / 9.04 | 8.80 / 11.32 | 60 | 66312 | 1600 |
| medium / forward | 8.39 / 9.22 | 11.60 / 13.42 | 70 | 66312 | 1578 |
| high-degree / wake | 1949.75 / 1951.85 | 1953.62 / 1959.84 | 15570 | 72804 | 106890 |
| high-degree / ask | 1867.17 / 1870.94 | 1849.44 / 1853.55 | 14780 | 73732 | 18487 |
| high-degree / goto | 11.55 / 13.71 | 10.63 / 12.29 | 90 | 73732 | 1600 |
| high-degree / forward | 11.64 / 13.66 | 14.42 / 15.16 | 90 | 73732 | 1578 |
| large-body / wake | 2307.29 / 2312.07 | 2350.84 / 2352.05 | 18790 | 70124 | 161104 |
| large-body / ask | 3943.64 / 3950.41 | 3988.14 / 3990.60 | 31900 | 74868 | 83975 |
| large-body / goto | 16.16 / 19.00 | 10.24 / 11.49 | 70 | 75124 | 1600 |
| large-body / forward | 16.01 / 16.51 | 10.55 / 12.24 | 70 | 75124 | 1578 |

The mixed changes do not establish a speedup. The after run enables debug
phase logging, and sequential rather than randomized run order leaves host
and cache drift as confounders. The 88 phase events expose graph/detail/bundle
work but omit ranking, rendering, transport and telemetry. They must not be
subtracted from total latency to invent a precise attribution of the remainder.

## Browser journeys

Times are before → after p50 milliseconds. Open has one sample; reload and
focus/trace have five. Raw per-journey p95, response bytes, timing and browser
metrics are in each shape's `journeys.json`.

| Shape | Open | Reload | Agent focus/trace | After HTTP requests open/reload/focus |
| --- | ---: | ---: | ---: | --- |
| small | 777.98 → 795.06 | 589.08 → 554.17 | 194.80 → 180.59 | [41] / [40] / [7] |
| medium | 848.44 → 774.87 | 606.02 → 605.92 | 196.72 → 214.15 | [41] / [40] / [7] |
| high-degree | 832.79 → 815.05 | 639.44 → 637.49 | 247.53 → 213.96 | [41] / [40] / [7] |
| large-body | 834.32 → 833.17 | 620.22 → 639.03 | 215.35 → 214.54 | [41] / [40] / [7] |

## Separate allocation control

One process per shape and side covers startup/initialize, eight native reads
(one first plus one warm sample for each of four operations) and one accepted
write. Seeding and Python/browser allocation are excluded. These instrumented
runs overlapped agent builds, so their timings are not used. Counts are
allocation requests and cumulative requested bytes, not live/peak memory.
All complete read results also matched in this campaign.

| Shape | Before requests → after | Before requested bytes → after |
| --- | ---: | ---: |
| small | 1,165,242 → 1,165,274 | 116,623,203 → 116,626,461 |
| medium | 4,251,233 → 4,251,266 | 438,707,622 → 438,711,422 |
| high-degree | 15,593,296 → 15,593,327 | 1,382,878,638 → 1,382,881,424 |
| large-body | 13,166,675 → 13,166,706 | 1,468,609,518 → 1,468,612,256 |

The small increases are consistent with opt-in tracing, without isolating its
exact cost. No allocation reduction is claimed for this baseline issue.

## Audit and excluded diagnostics

`validation.json` records verified manifest hashes, complete parity and journey
counts. Each run's manifest checks both compressed and original bytes. The
paired summaries are `comparison.jsonl` and `allocation-parity.jsonl`; do not
interpret allocation-run latency. Local compiler/test logs are adjacent.
`diagnostics.json` identifies smoke failures and explains their corrections;
smoke outputs are excluded from all reported measurements.
