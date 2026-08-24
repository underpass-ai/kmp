---
description: Revert a decision in memory without deleting it — write the compensating entry, then show that both states still exist
argument-hint: "[what to revert]"
allowed-tools: mcp__plugin_kmp_kernel-memory__kernel_wake, mcp__plugin_kmp_kernel-memory__kernel_inspect, mcp__plugin_kmp_kernel-memory__kernel_rewind, mcp__plugin_kmp_kernel-memory__kernel_write_memory, mcp__plugin_kmp_kernel-memory__kernel_ask
---

Undo a decision the way an append-only log undoes things: by recording that
it was undone, never by removing it.

`$ARGUMENTS` says what to revert. If it is vague — "the last decision", "that
thing about retries" — find the candidate first with `kernel_wake` or
`kernel_ask` and **confirm which ref you mean before writing**. Reverting the
wrong entry is not undoable by a second revert; it just adds a second wrong
entry to a permanent record.

## Read before you write

`kernel_inspect` the target. You need three things from it: that the ref
exists, what it actually said, and whether something already supersedes it —
reverting an entry that was already reverted means the story is more tangled
than "undo the last thing", and the user should hear that rather than get a
duplicate.

## The compensating write

One `kernel_write_memory` with `intent: "record_decision"`, and a
`connect_to` carrying `rel: "supersedes"`, `class: "evidential"`:

- **`current.summary`** — what is true *now*. Not "reverting X": the entry has
  to stand on its own for someone reading it a year from now who never sees
  the thing it replaced.
- **`why`** on the relation — why the earlier decision stopped being right.
  This is the part that makes a reverted decision worth keeping: the record of
  a wrong turn is only useful if it says what was learned.
- **`evidence`** on both — what showed it was wrong.

Ask the user for the why if they have not given you one. A supersession
without a reason is a deletion with extra steps.

## Then show them nothing was lost

This is the point of the command, so do it rather than describe it:

1. `kernel_ask` about the subject — the answer now leads with the new state.
2. `kernel_rewind` from a time *before* the reversal — the old decision is
   still there, still saying what it said, with the evidence it had.

Report both. **"The memory now says X. As of <before>, it said Y, and here is
why that changed."** That is what an append-only log buys, and it is invisible
until someone shows it.

## What not to do

Never look for a delete. There isn't one, and that is deliberate: a memory you
can quietly edit is a memory nobody can trust as evidence later.

If the user wants the earlier entry to stop being *findable* rather than
stop being *current* — a secret, a name that should not be there — that is a
different operation with different consequences, and this command is not it.
Say so and stop.

**Known limit, and say it if it matters here.** A superseded entry is not yet
marked as superseded in `kernel_wake` or `kernel_ask` output; the supersession
is visible on the relation, through `kernel_inspect`, and by rewinding. A
reader who does neither may still act on the old decision. That is why this
command shows both states rather than trusting the reader to notice.

<!-- kmp:voice -->
**Say it in the house voice.** One line per thing, and detail only where
something needs it. The fix goes next to the problem, never in a footer. Close
with a verdict in plain words and at most one next command.

Write it young, fresh and a little freak: short sentences, present tense,
talking to the person rather than reporting on the software. No emoji soup,
and never a joke inside a failure. If the personality costs an extra line, cut
the personality.
<!-- /kmp:voice -->
