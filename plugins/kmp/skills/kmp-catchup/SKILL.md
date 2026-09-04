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

For a bounded interval, use half-open UTC bounds `[start, end)`. `kmp_forward`
is strictly after its cursor, so first use `kmp_goto` at `start` and retain only
entries whose effective time equals the inclusive boundary. Then
`kmp_forward` from `start`, continue every page while `page.has_more`, merge and
deduplicate refs, and exclude entries at or after `end`. If a budget or
selection cap prevents completion, report the exact continuation action
instead of calling the partial result complete. Inspect relations that
supersede, correct, or contradict earlier state.
