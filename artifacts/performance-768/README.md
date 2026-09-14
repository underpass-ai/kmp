# Performance evidence for #768

The paired baseline and candidate JSON files are produced by
`scripts/performance/quality_metric_rendering.py` with the same runner source,
four synthetic shapes, uninstrumented render/metric latency, and separate
LD_PRELOAD allocation controls. Latency uses eight warm samples; each
allocation control is one fresh process per phase and shape, with its first
call plus five warm calls. Instrumented timings are ignored.

The large-payload fixture uses 512 repetitions of `payload-` in the generated
node, detail, and relationship fields (approximately 600 KB of canonical flat
text). The Unicode shape includes emoji, accented text, quotes, backslashes,
and the literal backslash+n text `\\n+line` from the Rust raw string; it has no
literal newline in that payload. Actual newline boundaries are covered by the
independent exactness tests. Raw arrays remain in the JSON; summary percentiles
use nearest rank.

The baseline runner was built from commit `7af38d73a6bd2dcf5ea9e83e394838af8d612a6f`.
The final manifest records both executable hashes, source hashes, Rust toolchain,
profile, host, and the exact commands. A first calibration used an oversized
8,192-repeat fixture and was interrupted; a second 512-repeat allocation run
was also interrupted after demonstrating that the fixture exceeded the slot.
Those interrupted runs are retained as calibration notes and are not result
rows. The successful paired run uses the bounded 512-repeat fixture and the
final candidate implementation.

The successful fixture has 97 canonical records and 545,324 UTF-8 bytes of
flat raw text. Quality values and complete render fingerprints match between
baseline and candidate for every shape. First-use and warm nearest-rank
latencies (milliseconds, baseline -> candidate) are recorded below; the warm
rows use eight samples and the first-use rows are separate one-process
observations:

| Shape | Render first | Render warm p50/p95 | Metric first | Metric warm p50/p95 |
| --- | ---: | ---: | ---: | ---: |
| small | 233.44 -> 225.77 | 4.19/4.21 -> 4.27/4.28 | 216.16 -> 214.15 | 1.00/1.01 -> 1.02/1.04 |
| unicode | 242.46 -> 234.44 | 29.46/29.52 -> 29.23/29.30 | 224.21 -> 235.17 | 7.77/7.79 -> 7.79/7.84 |
| large payload | 2076.54 -> 2057.03 | 1861.62/1865.51 -> 1846.52/1851.73 | 745.19 -> 749.10 | 526.05/528.10 -> 522.96/523.59 |
| relations | 487.60 -> 491.26 | 275.04/275.63 -> 273.53/274.71 | 312.29 -> 328.18 | 102.50/102.65 -> 104.08/104.22 |

The candidate's warm metric p50 is slightly lower for the large payload (522.96
ms versus 526.05 ms in this single-host campaign); other shapes are mixed. The
result is attribution evidence, not a latency gate.

The metric allocation controls report cumulative process-scoped requests and
requested bytes (baseline -> candidate):

| Shape | Requests | Requested bytes |
| --- | ---: | ---: |
| small | 426,799 -> 427,147 | 31,187,870 -> 31,192,859 |
| unicode | 462,700 -> 464,464 | 35,022,604 -> 34,965,205 |
| large payload | 3,269,437 -> 3,276,799 | 435,688,556 -> 420,845,837 |
| relations | 945,874 -> 979,954 | 90,525,640 -> 90,089,539 |

The large-payload request count increases while cumulative requested bytes fall;
these controls do not show a decrease in allocation-request count.

The historical raw JSON files retain the metadata label `candidate: "none;
baseline attribution only"` from the measured script. The script now emits the
paired-campaign label for future runs; `validation.json` records both script
hashes and this metadata erratum.
Allocation controls include process startup and bundle setup, so they are
reported as separate request/byte observations rather than per-call heap
claims. See `comparison.json` for the raw arrays' nearest-rank summaries and
exact parity fields.
