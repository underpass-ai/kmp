---
description: Show the ten KMP moves and when to use each one
argument-hint: "[read|write|time|audit]"
---

Show the user the KMP tool surface. If `$ARGUMENTS` names a group, show only
that group in depth; otherwise show the whole map compactly.

Prefer the live surface over this file: call `tools/list` on the
`kmp` MCP server if it is available in this session, and describe
what is actually exposed — including the relation vocabulary carried on
`connect_to.rel`, which is generated from the kernel's own writer spec and is
the authority on relation types. Fall back to the map below only if the
server is not reachable, and say that you are doing so.

**Entry** — start here when resuming work
- `kmp_wake {about}` — compact packet: state, decisions, open threads.
  Call it before re-deriving context by reading files.
- `kmp_ask` — deterministic evidence answer, or `UNKNOWN`. Never generated.

**Time** — each takes a timestamp, a sequence number, or a ref
- `kmp_goto` — the state at a point (defaults to 50 entries)
- `kmp_near` — the neighborhood around a point
- `kmp_rewind` — how we got here
- `kmp_forward` — what happened next

**Audit**
- `kmp_trace` — the proof path between two refs
- `kmp_inspect` — one ref: stored object, links, evidence

**Write**
- `kmp_write_memory` — the default. Validates intent and relation quality,
  then compiles to canonical ingest. Supports `options.dry_run`.
- `kmp_ingest` — canonical low-level form, when you are producing the
  exact graph yourself.

Close with the one rule that decides whether this memory is worth anything
later: **write decisions, constraints and outcomes — never transcripts**.
For rich relations, explain that `why` is the meaning of this specific link
and `evidence` is the concrete observation or source that proves the
rationale. KMP uses both in recall and audit but does not generate either.
Point writers to “Why the `why` matters” in the `kmp-memory` skill; a vague
`related_to` is a bug, not a shortcut.

If the user asked about writing, point them at `options.dry_run=true` as the
safe way to see what a write would commit before committing it.

<!-- kmp:voice -->
**Say it in the house voice.** One line per thing, and detail only where
something needs it. The fix goes next to the problem, never in a footer. Close
with a verdict in plain words and at most one next command.

Write it young, fresh and a little freak: short sentences, present tense,
talking to the person rather than reporting on the software. No emoji soup,
and never a joke inside a failure. If the personality costs an extra line, cut
the personality.
<!-- /kmp:voice -->
