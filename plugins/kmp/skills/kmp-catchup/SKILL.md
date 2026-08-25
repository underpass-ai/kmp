---
name: kmp-catchup
description: Catch up on KMP memory since a timestamp or prior frontier. Use for what happened since, yesterday, today, or another temporal interval.
---

# KMP catchup

Temporal intent has precedence over semantic Ask. Resolve relative dates in
the user's timezone and navigate with `kmp_forward`, `kmp_rewind`, `kmp_goto`,
or `kmp_near`; do not begin with `kmp_ask`.

For a bounded interval, use half-open UTC bounds `[start, end)`. Continue every
page while `page.has_more` and while entries can still fall inside the
interval. Exclude entries outside the bounds. If a budget or selection cap
prevents completion, report the exact continuation action instead of calling
the partial result complete. Inspect relations that supersede, correct, or
contradict earlier state.
