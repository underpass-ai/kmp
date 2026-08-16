# ADR-018: Let two agent hosts share one embedded memory

**Status:** Proposed
**Date:** 2026-08-16
**Revisits:** the concurrency model in
[ADR-011](ADR-011-embedded-concurrency-model.md) and, conditionally, the
engine choice in [ADR-009](ADR-009-embedded-storage-engine.md)
**Evidence:** [spikes/e-concurrency-spike/](spikes/e-concurrency-spike/README.md)

## The problem

ADR-011 accepted a single-writer store on an explicit premise:

> Two Claude Code sessions on the same project is realistic but occasional;
> v1 optimizes for correctness and an explicit UX in that case, not for
> concurrent throughput.

That premise is false now, and not because people open more windows. The
plugin installs into **two different hosts** — Claude Code and Codex CLI —
and a developer who uses both has them open at once as a matter of course.
Whichever starts first owns the memory. The other gets an error it cannot act
on without closing the tool it was about to work in.

Worse, the outcome depends on start order: it works, then it does not, and
the user changed nothing. For a memory product that is the most expensive
failure shape there is, because the response is to stop relying on it.

ADR-011 wrote its own trigger — "if concurrent sessions prove common" — and
it has fired.

## What the spike measured

Two real OS processes, one store, 300 durable events each. This is the
question ADR-009's single-process benchmark could not ask.

| engine | processes that opened | events durably written |
| --- | --- | --- |
| redb 4.1.0 | 1 / 2 | 300 / 600 |
| SQLite (WAL) | **2 / 2** | **600 / 600** |

A SQLite reader alongside a live writer completed 31,843 consistent,
monotonic snapshots without blocking it. redb never reached that test.

ADR-009's deciding criteria, re-measured here because the originals are from
different hardware:

| engine | batched write (ev/s) | reopen (ms) | point reads (r/s) | size (MB) |
| --- | --- | --- | --- | --- |
| redb 4.1.0 | 88,427 | 1.93 | 4,028,475 | 64.3 |
| SQLite (WAL) | 61,594 | **0.18** | 781,421 | **26.1** |

## Upstream is building this

Before choosing an engine, the obvious question is what it would cost to fix
redb instead. It is already being fixed, by its maintainer, and that changes
the arithmetic.

`cberner/redb` has three open PRs dated 2026-08-14 — #1375, #1376, #1377,
~2,000 lines — adding a `MultiProcessDatabase` behind an
`experimental-multiprocess` feature flag. None are merged.

What matters is what they say about themselves. From #1375:

> Today that is the same restriction by a different mechanism -- one process
> at a time [...] **this type has no advantage over `Database` yet**. It is
> the first step of an incomplete feature [...] and then the actual point of
> the exercise -- readers attaching to a database another process is writing,
> and handing the writer role between processes.

So the scaffolding is landing and the part we need is explicitly later. The
design notes also record why it is the hard part:

> readers are only safe once they can publish what they are reading, which is
> a later step [...] making room for readers means *removing* this lock, not
> weakening it, and all reader/writer coordination has to live in the
> directory's separate lock files.

That is, in outline, SQLite's WAL-index in a `-shm` file — a design SQLite
has had for over a decade and has tested harder than anything we would write.
Contributing it to redb is not a patch we can scope; it is joining an active
design on someone else's timeline, and the result would be gated experimental
for a while after it lands.

## Options

**A — Port to SQLite now.** Solves it today. Costs ~2,300 lines in
`kmp-adapter-embedded`, a one-time import of existing stores, ~5× slower
point reads, ~30% slower batched writes, ~+1.4MB binary, and the return of a
C toolchain that complicates the static musl and Windows targets ADR-009 kept
simple. Buys 2.5× smaller stores and 10× faster reopen.

**B — Wait for redb, mitigate now.** Costs almost nothing today and keeps the
pure-Rust story. The mitigation is documentation plus a per-host store path
for people who hit it, which means the two hosts do not share memory — the
value we are trying to deliver — so it is relief, not a fix. Timeline is not
ours: the piece we need is unscheduled.

**C — Contribute the reader-attach work to redb.** Best outcome for everyone
and the worst fit for a product with a launch to run. The remaining design is
the hard 60%, on a maintainer's roadmap, for a feature that ships behind an
experimental flag.

## Recommendation

**B, with a scheduled re-decision.** Not because A is wrong on the merits —
the spike says it works — but because of what we would be buying it with.
The embedded edition's pitch is a single pure-Rust binary that runs anywhere;
paying a C dependency and a store migration to solve a problem upstream is
visibly working on, for a user base that is currently near zero, spends the
wrong currency at the wrong time.

Concretely:

- ship the per-host path escape and say plainly in the docs that two hosts
  cannot share one store yet, and why;
- watch #1375/#1376/#1377 and whatever follows them;
- re-decide when the reader-attach work lands, or when it has visibly
  stalled, or when a user who is not us reports this — whichever is first.

If any of those arrive and the answer is still no, A is on the shelf with its
numbers already measured, and ADR-009 already promised the swap is an adapter
change rather than a redesign.

## Consequences

- **Accepted for now:** two agent hosts still cannot share one memory. This
  is a real product gap and the docs must say so rather than let a user
  discover it by start order.
- **Unchanged:** pure Rust, one file, static musl and Windows stay simple.
- **If we later take A:** the append-only event log is the source of truth
  and projections are rebuildable, so the import replays rather than
  translating pages; store format is stamped and fail-fast checked
  ([ADR-012](ADR-012-embedded-data-directory.md)), so an old store is
  imported or refused, never opened with the wrong engine; and the 16
  engine-agnostic conformance scenarios are the acceptance criteria, plus a
  new one for two processes writing without lost events.
- **Either way**, [ADR-017](ADR-017-embedded-memory-viewer.md)'s reasoning
  for an in-process viewer — "the embedded store is single-writer, so a
  separate viewer process could never watch a live agent session" — stops
  being true the day this changes, and a standalone viewer becomes possible.
