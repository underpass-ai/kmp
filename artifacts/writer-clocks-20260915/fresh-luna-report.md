# Fresh Luna native writer report

## Scope and execution

- Source read: `SOURCE.md` once.
- Writer instructions read: `WRITER-INSTRUCTIONS.md` once.
- Isolated about: `project:fresh-luna-f17-g2`.
- Driver: `driver.py` with the supplied fresh binary and trace output at `fresh-luna-native.jsonl`.
- KMP native agent: `traveler-38413bd3b7a1`; generation was done by this writer and the deterministic driver did not invoke a model or paid API.

## Stored representation

The accepted packet contains 7 `observation` entries, 20 label memberships, 10 evidence items, and 3 rich evidential relations:

- F17 receipt at `2026-09-04T12:00:00Z`.
- R8 staging execution at `2026-09-04T11:00:00Z`.
- H8 test-file restoration and checksum check at `2026-09-04T11:05:00Z`.
- F17's log association with R8 at `2026-09-04T12:10:00Z`, while retaining the log's `11:00Z` source/event time.
- F17's earlier copied quantity of 81 MB, with occurrence time left unknown.
- F17's quantity-only correction from 81 MB to 80 MB at `2026-09-04T11:06:00Z`.
- G2's 79 MB report for the same copy at `2026-09-04T11:07:00Z`.

The relations are directed as follows: R8 `verified_by` H8; 80 MB `corrects` 81 MB; and G2's 79 MB report `contradicts` F17's 80 MB correction. The last relation carries the source's unresolved-disagreement wording. No relationship occurrence or validity clock was supplied.

All seven entries defaulted to the accepted command's observation and ingestion instant, `2026-09-14T22:36:17.415583185Z`. The accepted receipt reports six entries with occurrence clocks and one without; all three relations have observed and ingested clocks, with zero occurrence or validity clocks.

## Validation and errors

The first packet was rejected because all seven `summary_en` values repeated their source text. The repaired packet was rejected once more because the first search summary dropped the source date/time identifiers. The complete corrected packet then returned `needs_review`; its served neighborhood showed the five proposed endpoint records and their event clocks. The requested expansion via `kmp_wake` returned `not_found` because the about had not yet been committed. That error is preserved in the trace. The unchanged served continuation was then resumed and committed atomically.

The receipt was inspected at `receipt:v1:project%3Afresh-luna-f17-g2:fresh-luna-f17-g2-batch-v1`; it confirms `accepted=true`, revision 1, 7 entries, 3 relations, and 10 evidence items. `source_coverage` remains `not_assessed`.

## Temporal checks

1. A complete `kmp_forward` read on the `occurred` axis over `[2026-09-04T00:00:00Z, 2026-09-05T00:00:00Z)` returned six entries in event order (11:00, 11:05, 11:06, 11:07, 12:00, 12:10). Its completed proof reports the persisted unresolved 79 MB versus 80 MB conflict.
2. A complete `kmp_forward` read on the `observed` axis over `[2026-09-14T22:36:17Z, 2026-09-14T22:36:18Z)` returned all seven entries at the single accepted observation/ingestion instant. The undated 81 MB fact remains present without an invented occurrence time.

These checks validate the supplied representation and native store behavior. They do not establish source completeness, writer understanding, causal improvement, or a benchmark result. The temporary store was left intact for audit.
