---
name: kmp-catchup
description: Catch up on KMP memory since a timestamp or prior frontier. Use when the user asks what KMP memory holds for an interval — since, yesterday, today, a date range — or runs /kmp:catchup. A temporal question that does not reach for memory is answered without it.
---

# KMP catchup

Temporal intent has precedence over semantic Ask. Resolve relative dates in
the user's timezone and navigate with `kmp_forward`, `kmp_rewind`, `kmp_goto`,
or `kmp_near`; do not begin with `kmp_ask`. A catch-up enumerates a period.
A semantic question that merely carries a date — why something was decided
in March — is not a catch-up: it is one `kmp_ask` with that interval as
`interval`, or the instant as `as_of`.

For a bounded interval, pass half-open UTC bounds `[start, end)` directly as
`interval` to `kmp_forward` or `kmp_rewind`, with the clock the question needs.
Omit `from` on the first read. KMP includes the start, excludes the end and
retains ties. Execute returned `next_actions`: finish the packet's entries and
proof while `page.has_more`, then navigate remaining history while
`selection.has_more`. Keep the returned interval and filters. If a budget or
selection cap prevents completion, report the exact continuation action.
Inspect relations that supersede, correct, or contradict earlier state. An
open-ended interval does not assess expiry; provide an end when that state
matters and consult the extended time guide for proof boundaries.
