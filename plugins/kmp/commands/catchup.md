---
description: Catch up on an about — what changed since you last looked, from the event log rather than from re-reading files
argument-hint: "[about] [since]"
allowed-tools: mcp__plugin_kmp_kernel-memory__kernel_wake, mcp__plugin_kmp_kernel-memory__kernel_rewind, mcp__plugin_kmp_kernel-memory__kernel_forward, mcp__plugin_kmp_kernel-memory__kernel_inspect
---

Answer one question: **what happened here since I last looked?**

The arguments are optional. `$1` is the about; with none, use the about this
work belongs to, and if that is ambiguous, ask rather than guess. `$2` is the
point to catch up from — a timestamp, or a phrase like "yesterday" you resolve
to one.

## Without a `since`

Find the frontier first, then decide.

1. `kernel_rewind` with `from: { time: <now> }`, `limit: { entries: 1 }` and
   `budget: { detail: "full" }`. The newest entry comes back with its
   `coordinates[].observed_at`. That is when this memory was last written,
   and `page.total` is how much it holds.
2. If that timestamp is older than the work in front of you, there is nothing
   to catch up on — say so in one line and stop. Do not pad.
3. Otherwise use it as the anchor and report what the memory says now, via
   `kernel_wake`.

## With a `since`

`kernel_forward` with `from: { time: <since> }` and
`budget: { detail: "full" }` returns exactly the entries after that moment,
in order, with their coordinates. `page.has_more` tells you whether the slice
was cut — if it was, say so rather than presenting a partial list as the whole
delta.

For anything that changed a decision — a `contradicts`, a `supersedes`, a
`corrects` — follow it with `kernel_inspect` and report what it replaced. That
is the part a file diff cannot tell them.

## Reporting

Lead with the count and the span: *"Four entries since Friday 14:00."* Then
the entries, newest first, each in one line: what it is, and if it changed
something earlier, what it changed.

Close with the cursor they can reuse:

> Next time: `kernel_forward` from `<newest observed_at>`.

Two things to get right. **If the memory is empty or the about does not
exist**, say exactly that — an empty memory is not a failure, it is the signal
that nothing has been written here yet. And **never fill a quiet delta with
narration**: "nothing has changed since Friday" is a complete answer and the
most useful one when it is true.
