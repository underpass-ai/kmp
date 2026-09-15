# Fresh final-main writer report

## Outcome

The isolated embedded store accepted six source-backed entries, five rich relations, and eleven evidence items under `project:fresh-final-main-n41`. An exact replay of the accepted logical write with idempotency key `fresh-final-main-n41-write-v1` returned `status: replayed` with the original ingestion clock.

The accepted representation separates the 09:00 transfer outcome, initial 62 MiB report, 09:07 checksum verification, 09:08 quantity-only correction to 63 MiB, conflicting 09:09 R3 report of 61 MiB, and the investigator's 10:15 association of the earlier 09:00 job log with J4. The first five entries are observed at the investigator's 10:00 receipt; the association is observed and occurs at 10:15. No relation has an invented occurrence time or validity interval.

Relations record transfer `verified_by` C7, transfer and corrected quantity satisfying the stored encryption and 64 MiB constraints, corrected 63 MiB `corrects` initial 62 MiB, and R3's 61 MiB `contradicts` the corrected quantity. The correction does not supersede the transfer or checksum fact.

## Review and verification

The first semantic packet was rejected because `outcome`, `verification`, and `correction` are invalid writer kinds. I repaired them to `observation` and `semantic_delta`; no part of the rejected packet was written.

The next proposal returned `needs_review`. Its checksum relation direction was wrong (`check -> verified_by -> transfer`). After reviewing the returned neighborhood, the writer guide, stored policy evidence, clocks, selection reasons, and expanded omissions, I corrected it to `transfer -> verified_by -> check`. A second review included the two stored archive constraints and five proposed relations. I completed the returned full-context pages, then resumed the exact opaque continuation; the write was accepted.

I inspected the accepted receipt and the correction entry with complete incoming/outgoing links and evidence. Temporal reads were complete at occurrence 09:08, observation 10:00, and observation 10:15. They showed the correction on the occurrence axis, the five receipt facts known at 10:00, and the later association appearing at 10:15. The receipt confirms two distinct observation values, five distinct occurrence values, and zero relation occurrence/validity clocks.

## Preparation correction and preserved failures

The permitted source and writer instructions did not include the seeded about. Before receiving the preparation correction, three guessed Wake calls (`project:archive-transfer`, `incident:J4`, and `project:writer-clocks`) returned `not_found`; these failures remain in the native trace. The parent supplied the declared preparation about `project:fresh-final-main-n41` afterward. That correction supplied only the missing identifier and no source fact, relation, or expected answer.

I also preserved one invalid experimental `kmp_ask` call using unsupported `all_abouts` and one missing-about `kmp_ask` rejection. No rejected call mutated the store.

## Limitations and omissions

The source label is `N41` for the whole packet even though one entry reports the separately named R3 note included in the same receipt; the R3 provenance is preserved in the entry text and evidence rather than a separate label. The 10:15 association is represented as its own observation; the earlier log's 09:00 source/event time remains explicit in evidence rather than being turned into a relationship clock. The source does not establish when `blue` first became usable, any relation occurrence time, any validity interval, or resolution of the 61/63 MiB disagreement, so none was stored.

KMP reported submitted-packet coverage as complete, but source coverage as `not_assessed`. Successful persistence and served guidance do not establish comprehension, source completeness, or causal improvement.
