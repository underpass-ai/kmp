---
name: kmp-memory
description: Operate KMP agent memory through the kernel-memory MCP server — recover context at session start instead of re-deriving it, answer questions from stored evidence, navigate history in time, audit a claim back to its proof, and record decisions with relations that carry their why. Use whenever the work continues something earlier (an ongoing project, an incident, a task with prior decisions), whenever you are about to re-derive context you may already have, whenever you need to justify a claim with evidence, and whenever a decision, constraint or outcome is reached that later sessions will need.
---

# KMP agent memory

KMP is graph-temporal memory for agents, reachable over MCP as ten tools. It
is a **kernel, not a model**: every answer is derived from stored evidence by
construction. Nothing here generates prose. If the memory does not support an
answer, `kernel_ask` returns `UNKNOWN` — that is a correct result, not a
failure to work around.

## Start here: recover before you re-derive

When the work continues something earlier, the first move is:

```
kernel_wake { about: "project:kmp" }
```

`kernel_wake` returns a compact wake packet: where the work stood, what was
decided, what is open. Call it **before** reading files to reconstruct
context you may already have stored. Abouts are stable ids, conventionally
`project:<name>` or `incident:<id>`.

If `kernel_wake` comes back empty, there is no memory for that about yet.
That is the signal to start writing one, not to keep guessing.

## The ten moves

**Entry**

| Move | Use it when |
| --- | --- |
| `kernel_wake` | Resuming known work. Compact packet: state, decisions, open threads. |
| `kernel_ask` | You have a specific question. Deterministic evidence answer, or `UNKNOWN`. |

**Navigate time** — all four take a timestamp, a sequence number, or a ref.

| Move | Use it when |
| --- | --- |
| `kernel_goto` | Jump to the state at a point in time. Defaults to 50 entries. |
| `kernel_near` | See the neighborhood around a point — what surrounded it. |
| `kernel_rewind` | Walk backward: how did we get here. |
| `kernel_forward` | Walk forward: what happened after this. |

### Catching up

"What happened since I last looked" is two of those moves, not a separate
feature, and it is the second thing to reach for after `kernel_wake` on work
that has been touched by someone else — another session, another host, a
colleague who imported a bundle.

The frontier first: `kernel_rewind` from now with `limit: { entries: 1 }` and
`budget: { detail: "full" }` returns the newest entry with its
`coordinates[].observed_at`, and `page.total` for how much the memory holds.
That timestamp is the bookmark.

Then the delta: `kernel_forward` from that timestamp — or from a plain
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
| `kernel_trace` | Prove a connection: the path between two refs. |
| `kernel_inspect` | Examine one ref: stored object, links, evidence. `include.raw=true` for audit refs. |

**Write**

| Move | Use it when |
| --- | --- |
| `kernel_write_memory` | **Default.** Writer-friendly: validates intent and relation quality, then compiles to canonical ingest. Supports `options.dry_run` to check before committing. |
| `kernel_ingest` | Canonical low-level form. Use when you are producing the exact graph yourself. |

Temporal reads return a `page` object. A bounded partial read is visible, not
silent — if `page` says the slice was truncated, say so rather than treating
it as the whole history.

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

## What to write, and what never to write

Write when something is **decided, constrained, or concluded**. Decisions,
constraints, outcomes — each with coordinates and evidence.

Never write transcripts. Memory is not a log of the conversation; it is the
durable shape of the work. A transcript makes later traversal worthless.

Use one `idempotency_key` per logical write. If a retry conflicts, the write
was already applied — that is success, not an error to retry around.

## Relations carry the why

This is the part most writers get wrong, and the kernel enforces it.

A relation is **rich** or **anemic**. Rich relations — causal, motivational,
evidential, constraint — require both `why` and `evidence`. They are what
makes memory explanatory instead of merely connected.

- Never use a vague relation like `related_to` when a real one applies.
- Never invent a causal or motivational link the evidence does not support.
- When a rich relation points at an existing external ref, include that ref
  in `read_context` — claim the connection only after reading what you are
  connecting to.
- If you cannot justify a rich relation after reading context, fall back
  explicitly and honestly: `follows`/procedural, `answers`/evidential, or
  `uses_background`/evidential. Do not dress an anemic fallback in causal
  language.

**The vocabulary is self-documenting.** `tools/list` carries a catalog
generated from the kernel's own writer spec on `connect_to.rel` and the
ingest `rel` field: every relation type with its quality tier, its allowed
semantic classes, and when to use it. Read the schema in front of you rather
than guessing from these examples — the schema is the authority, and it moves
with the kernel.

## Scope is explicit

Dimension scope is auditable, never implicit:

- omitted → `current_about`
- `abouts` → requires a non-empty list
- `all_abouts` → traverses every memory anchor, explicitly

Widen scope deliberately. `all_abouts` on a large store is a real cost, and
an unscoped sweep buries the answer you wanted.

## When the tools are not there

If the kernel-memory tools are missing from your inventory, do not silently
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
`structuredContent.error.{code,message}`. Read the code; it is specific.
Report what it says rather than retrying blindly.
