# Human ChronoLoom review packet

The view is prepared from a byte-identical copy of the closed writer SQLite
store. Both database files had SHA-256
`3a9a0918767fa2599e19b058c3181e935d042ee8d17419e2c93b60bd89481211`
before the view was opened. View state is camera state on the copy and does not
change memory records. No independent reader has run.

The live loopback capability is supplied directly to the reviewer. It is
session-local access material and is intentionally absent from this public
artifact.

The prepared view uses the observed axis, episode zoom, `task` and `source`
lanes, and evidential/constraint relations. It focuses these records:

- transfer: `project:fresh-final-main-n41:entry:observation:job-j4-copied-an-archive-to-the-encrypted-blue-destination-1e420c5c29051188`
- checksum: `project:fresh-final-main-n41:entry:observation:check-c7-verified-j4-s-checksum-4857f9757885dcfe`
- corrected 63 MiB: `project:fresh-final-main-n41:entry:semantic_delta:n41-corrected-only-j4-s-copied-quantity-to-63-mib-7133afb29c5fe3ff`
- conflicting 61 MiB: `project:fresh-final-main-n41:entry:observation:r3-reported-61-mib-for-the-same-j4-transfer-the-disagreement-re-7b8793c707c74f97`
- 64 MiB constraint: `project:fresh-final-main-n41:entry:constraint:archive-transfers-must-not-exceed-64-mib-af616641407b950b`
- encryption constraint: `project:fresh-final-main-n41:entry:constraint:archive-destinations-must-be-encrypted-693e8bf3be2288f6`

Please check the visible graph against the source:

1. J4 transfer points through `verified_by` to C7, whose event is 09:07.
2. The 63 MiB record points through `corrects` to 62 MiB; it does not replace
   the transfer or checksum.
3. R3's 61 MiB points through `contradicts` to 63 MiB, and remains unresolved.
4. The transfer satisfies encryption and 63 MiB satisfies the older 64 MiB
   constraint. The unrelated garden constraint does not enter these routes.
5. The receipt facts are observed at 10:00. The log association is a separate
   observation at 10:15 while retaining 09:00 in its evidence.
6. Relations have observation and ingestion clocks, with no invented
   occurrence or validity clock.

The explicit time range took priority over simultaneous trace framing, which
ChronoLoom reported as unhonored. The focused refs and selected correction were
honored. Human approval or a finding should be recorded before any independent
experimental reader is started.
