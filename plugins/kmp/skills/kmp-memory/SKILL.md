---
name: kmp-memory
description: Operate KMP agent memory through the kmp MCP server — recover context at session start instead of re-deriving it, answer questions from stored evidence, navigate history in time, audit a claim back to its proof, and record decisions with relations that carry their why. Use whenever the work continues something earlier (an ongoing project, an incident, a task with prior decisions), whenever you are about to re-derive context you may already have, whenever you need to justify a claim with evidence, and whenever a decision, constraint or outcome is reached that later sessions will need.
---

# KMP agent memory

KMP is graph-temporal memory for agents, reachable over MCP as ten tools. It
is a **kernel, not a model**: every answer is derived from stored evidence by
construction. Nothing here generates prose. If the memory does not support an
answer, `kmp_ask` returns `UNKNOWN` — that is a correct result, not a
failure to work around.

## Start here: recover before you re-derive

When the work continues something earlier, the first move is:

```
kmp_wake { about: "project:kmp" }
```

`kmp_wake` returns a compact wake packet: where the work stood, what was
decided, what is open. Call it **before** reading files to reconstruct
context you may already have stored. Abouts are stable ids, conventionally
`project:<name>` or `incident:<id>`.

If `kmp_wake` comes back empty, there is no memory for that about yet.
That is the signal to start writing one, not to keep guessing.

Wake also hands back `resume_cursor` — the newest coordinate the packet
covers. Carry it, and the next question ("what changed since I looked?") is
one call:

```
kmp_forward { about: "project:kmp", from: <the resume_cursor> }
```

The kernel does not remember where each reader got to, deliberately: that
would be state about the reader rather than about the work. The bookmark is
yours to hold. `resume_cursor` is `null` when nothing in the packet carries a
temporal coordinate.

## The ten moves

**Entry**

| Move | Use it when |
| --- | --- |
| `kmp_wake` | Resuming known work. Compact packet: state, decisions, open threads. |
| `kmp_ask` | You have a specific question. Deterministic evidence answer, or `UNKNOWN`. |

**Navigate time** — all four take a timestamp, a sequence number, or a ref.

| Move | Use it when |
| --- | --- |
| `kmp_goto` | Jump to the state at a point in time. Set `limit.entries` — there is no default. |
| `kmp_near` | See the neighborhood around a point — what surrounded it. |
| `kmp_rewind` | Walk backward: how did we get here. |
| `kmp_forward` | Walk forward: what happened after this. |

### Catching up

"What happened since I last looked" is two of those moves, not a separate
feature, and it is the second thing to reach for after `kmp_wake` on work
that has been touched by someone else — another session, another host, a
colleague who imported a bundle.

The frontier first: `kmp_rewind` from now with `limit: { entries: 1 }` and
`budget: { detail: "full" }` returns the newest entry with its
`coordinates[].observed_at`. In this temporal response, `page.total` counts
temporal entries in the selected move; it is not the same unit as recall
`projection.page.total`, which counts eligible expansion items. That timestamp
is the bookmark.

Then the delta: `kmp_forward` from that timestamp — or from a plain
"since Friday" the user gives you — returns exactly what came after, in order.
`page.has_more` says whether the slice was cut; a truncated delta reported as
the whole one is worse than no delta.

Carry the newest timestamp forward into your own notes or your next write. The
kernel does not remember where each reader got to, on purpose: a memory that
tracked its readers would be keeping state about you rather than about the
work.

**Audit**

| Move | Use it when |
| --- | --- |
| `kmp_trace` | Prove a connection between two refs in the same memory graph. Abouts are never joined, so cross-about refs have no path. |
| `kmp_inspect` | Examine one ref: stored object, links, evidence. `include.raw=true` for audit refs; `budget.max_bytes` bounds the packet and an oversized hub is refused with narrowing guidance. |

**Write**

| Move | Use it when |
| --- | --- |
| `kmp_write_memory` | **Default.** Writer-friendly: validates intent and relation quality, then compiles to canonical ingest. Supports `options.dry_run` to check before committing. |
| `kmp_ingest` | Canonical low-level form. Use when you are producing the exact graph yourself. |

Temporal reads return a `page` object whose total is temporal entries and whose
continuation is a memory-ref cursor. Wake and Ask return `projection.page`,
whose total is eligible expansion items and whose cursor is an opaque,
selection-bound token. A bounded partial read is visible, not silent — if
either page says the slice was cut, say so rather than treating it as the whole
history.

## Memory can live in the repository

`kmp-mcp export` with no argument writes the event log to
`.kmp/memory.jsonl` at the project root; `kmp-mcp import` reads it back. The
store itself (`.kernel/`) is machine state and stays gitignored — the bundle
is a different thing, and committing it is what makes a fresh clone arrive
with the project's decisions instead of an empty memory.

It is one JSON object per line in sequence order, so an append-only log
appears in `git diff` as appended lines. A session that recorded three
decisions is three new lines plus the header's `event_count`, and each line
carries who wrote it and the rationale of every relation, verbatim. A pull
request that also settled three questions shows them in review.

Two limits to state rather than discover. **Import requires an empty store**:
it is restore, not merge, because replaying a bundle over existing memory
could duplicate history or interleave two timelines and neither has an answer
the kernel could pick. And **a bundle carries the payloads as written** — a
secret in memory is a secret in the committed file, so the hygiene of the
bundle is the hygiene of the store.

## The viewer, when it offers itself

`kmp_write_memory` sometimes comes back with a `viewer` block:

```json
"viewer": {
  "url": "http://127.0.0.1:7317/",
  "tell_the_user": "Their memory is now a graph they can open: …"
}
```

That is the session saying it is already serving this store as a graph — the
same abouts, the same typed relations, the same evidence, in a window instead
of in JSON. It appears on the **first memory a session writes**, because that
is the first moment there is anything to look at, and it does not appear
again.

Pass the link on when it appears. It is the one thing a human can look at
without learning any of this, and most people never find out it exists.

## What to write, and what never to write

Write when something is **decided, constrained, or concluded**. Decisions,
constraints, outcomes — each with coordinates and evidence.

Never write transcripts. Memory is not a log of the conversation; it is the
durable shape of the work. A transcript makes later traversal worthless.

Use one `idempotency_key` per logical write. If a retry conflicts, the write
was already applied — that is success, not an error to retry around.

## `observed_at` is the real clock, in UTC

Every write carries `observed_at`, and the whole read path is ordered by it.
**Read the clock; do not compose a timestamp.** Local wall-clock time with a
`Z` on the end is valid RFC3339 and the wrong instant, and it puts the entry
above the present — where `kmp_forward` from a correct "now" never finds
it, and the delta comes back empty looking exactly like a quiet week.

A stamp more than five minutes ahead of the kernel's clock is refused at
write time. Earlier is fine: writing up yesterday's incident this morning is a
backfill, and stamping when it happened is the point.

## Undoing is a write, not a delete

There is no delete, on purpose: a memory that can be quietly edited is a
memory nobody can trust as evidence later. A decision that stopped being right
is reverted by recording that it was — a new entry saying what is true now,
connected with `supersedes` (evidential) carrying **why** the earlier one
stopped holding, and the evidence that showed it.

Write the new entry so it stands on its own. "Reverting X" is not a summary;
someone reading it a year from now may never see X. And never write a
supersession without a reason — a supersession with no why is a deletion with
extra steps, and it destroys the one thing the record was for.

What this buys is visible only if you show it: after reverting, the current
answer leads with the new state, and `kmp_rewind` to before the reversal
still returns the old decision with the evidence it had. Both are true, at
different times, and that is the whole point of keeping a log.

A replaced entry comes back **marked**. `kmp_wake` and `kmp_ask` carry
`proof.superseded`, one line per entry that a later one replaced:

```json
"superseded": [
  {
    "ref": "project:kmp:decision:usar-redb",
    "superseded_by": "project:kmp:decision:usar-sqlite",
    "why": "two agent hosts need to share the store"
  }
]
```

It is kept apart from `proof.conflicts` on purpose. `contradicts` says two
entries disagree and both may still be live — the tension is the information.
`supersedes` says one replaced the other: no tension, a lifecycle, and the
older entry is history rather than advice. Read the older one as what was
true then, not as what to do now.

## Why the `why` matters

An entry records **what is true**. A relation records **how two entries are
connected**. Its `why` records **why that specific connection holds and what
a later reader should understand by traversing it**. Its `evidence` records
**the concrete observation or source that supports that explanation**.
`confidence` says how certain the writer is; it is not a relevance score.

Those fields are deliberately separate:

| Field | Durable meaning | Good test |
| --- | --- | --- |
| entry | What is true | Can it stand alone a year later? |
| `rel` + `class` | How the endpoints connect | Is this the most specific supported relation? |
| `why` | Why this connection holds | Does it explain both endpoints, not merely repeat one? |
| `evidence` | What proves that rationale | Is it an observed fact or named source rather than an interpretation? |
| `confidence` | Certainty in the relation | Does it reflect the strength of the proof? |

KMP preserves and uses this context; it does not invent it. During recall,
direct evidence determines whether a candidate is eligible. A typed,
evidence-backed relation and its `why` can then improve the ordering and
explain the match, but relation prose cannot promote unrelated evidence into
an answer. The selected relation types are exposed in
`proof.matched_relations`, and the original rationale remains auditable in
`proof.path`, `kmp_trace`, and `kmp_inspect`.

That gives the agent a complete read path:

1. `kmp_wake` recovers the durable state and its causal or motivational
   spine.
2. `kmp_ask` answers a paraphrased question from direct evidence and uses
   the graph context to keep the right citation in the core.
3. `kmp_trace` proves the path between two refs; `kmp_inspect` shows the
   stored object, links, and evidence verbatim.

### Write the rationale and its proof as a pair

For a decision selected from an observation:

```json
{
  "ref": "incident:login:observation:refresh-race",
  "rel": "chosen_because",
  "class": "motivational",
  "why": "Retry was chosen because it addresses the refresh race without weakening the timeout policy.",
  "evidence": "Auth logs show refresh success followed by a 401 on the next request.",
  "confidence": "high"
}
```

For an outcome governed by a constraint:

```json
{
  "ref": "project:kmp:constraint:shared-store",
  "rel": "satisfies_constraint",
  "class": "constraint",
  "why": "SQLite WAL satisfies the requirement that independent agent processes share one embedded store.",
  "evidence": "The two-process integration test completed concurrent reads and writes without a single-writer lock failure.",
  "confidence": "high"
}
```

For a lifecycle change:

```json
{
  "ref": "project:kmp:decision:redb",
  "rel": "supersedes",
  "class": "evidential",
  "why": "SQLite WAL replaces redb because the architecture now requires multi-process access to one store.",
  "evidence": "The shared-store test failed at redb's process lock and passed under SQLite WAL.",
  "confidence": "high"
}
```

In all three, swapping `why` and `evidence` would lose information. The
rationale interprets the edge; the evidence anchors that interpretation in
something observed.

Before committing a rich relation:

- inspect or traverse every existing target and name it in `read_context`;
- choose the most specific relation supported by what you read;
- make `why` mention the actual relationship between the two endpoints;
- make `evidence` concrete and independent enough to audit;
- preview with `options.dry_run=true`, inspect the compiled relation, then
  commit the same logical write with a stable `idempotency_key`.

A relation is **rich** or **anemic**. Rich relations — causal, motivational,
evidential, constraint — require both `why` and `evidence`. If the context
does not justify one, use a narrow fallback and say only what is known:

| What the evidence supports | Honest fallback |
| --- | --- |
| Only temporal sequence | `follows` / procedural |
| A response to a prior turn | `answers` / evidential |
| Background was consulted | `uses_background` / evidential |

Never use a vague relation like `related_to`, invent a causal link, or dress
one of these fallbacks in motivational or constraint language. A weak honest
edge is safer than a rich false one.

**The vocabulary is self-documenting.** `tools/list` carries a catalog
generated from the kernel's own writer spec on `connect_to.rel` and the
ingest `rel` field: every relation type with its quality tier, its allowed
semantic classes, and when to use it. Read the schema in front of you rather
than guessing from these examples — the schema is the authority, and it moves
with the kernel.

## Reading across projects

Abouts are **not** joined by relations, and that is the design rather than a
gap. An edge between two projects would bake the link into the graph: anyone
traversing MADE would drag KMP along whether they wanted it or not, and the
frontier an about exists to bound would stop being bounded.

So the join lives with the reader, at read time, and `dimensions.scope` is
where you make it:

```json
{
  "about": "project:made",
  "question": "Why does the store conversion copy rows instead of replaying?",
  "dimensions": { "scope": "abouts", "abouts": ["project:made", "project:kmp"] }
}
```

One call, both projects, and the evidence comes back attributed to each — the
ADR-018 reasoning that lives in `project:kmp` delivered into a conversation
about MADE, without either project's graph growing an edge it did not ask for.

Reach for this whenever a decision made in one project governs work in
another: a shared contract, an ADR that both sides implement, a constraint one
repository imposes on its sibling.

Scope is auditable, never implicit:

- omitted → `current_about`
- `abouts` → requires a non-empty list
- `all_abouts` → traverses every memory anchor, explicitly

Widen deliberately. `all_abouts` on a large store is a real cost, and an
unscoped sweep buries the answer you wanted — but scoping to the two or three
abouts that actually bear on the question costs almost nothing and is the
whole point of the mechanism.

## When the tools are not there

If the kmp tools are missing from your inventory, do not silently
fall back to re-deriving everything — say so. The usual causes are specific
and fixable, and `/kmp:doctor` distinguishes them:

- the `kmp-mcp` binary is not installed or not on `PATH`;
- another session holds this project's `.kernel/` store — the embedded store
  is single-writer by contract (ADR-011), and the tools are withheld rather
  than risking corruption;
- the session started before the MCP registration changed, so it is still
  carrying the old inventory — restart the session.

## Errors

Tool failures set `isError=true` and carry
`structuredContent.error.{code,message}`. Read the code; it is produced where
the failure happened, never inferred from the words, and `tools/list` carries
the closed set under `_meta."kmp/errorCodes"` with what each one means.

| code | what to do |
| --- | --- |
| `invalid_argument` | fix the arguments — retrying unchanged cannot work |
| `not_found` | the memory is not in this store |
| `conflict` | the write already landed under this idempotency key. **That is success**, not something to retry around |
| `unavailable` | the kernel was unreachable; the same call may work later |
| `unknown_tool` | no such tool here |
| `backend_error` | the kernel failed for a reason no argument can fix |

An unknown argument is refused rather than dropped: every tool declares
`additionalProperties: false` and the boundary enforces it, so a misspelling
comes back naming the key instead of being answered from defaults.
