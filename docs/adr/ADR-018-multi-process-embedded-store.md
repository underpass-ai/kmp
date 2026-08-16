# ADR-018: A second embedded engine, so two agent hosts can share one memory

**Status:** Accepted
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

**A — Port to SQLite, replacing redb.** Solves it today, and forces the cost
on everyone: a C toolchain in every build, a store migration for every user,
~5× slower point reads, ~+1.4MB binary.

**B — Wait for redb, mitigate with documentation.** Costs nothing today and
delivers nothing either: the two hosts still do not share memory, and the
timeline is not ours.

**C — Contribute the reader-attach work to redb.** Best outcome for everyone,
worst fit for a product with a launch to run: the remaining design is the
hard part, on a maintainer's roadmap, shipping behind an experimental flag.

**D — Both engines. redb stays the default; SQLite is opt-in.** Chosen.

## Decision

`kmp-adapter-embedded` grows a storage seam and a second implementation
behind a `sqlite` cargo feature. redb remains the default engine and the
default build: pure Rust, one file, static musl and Windows unchanged, no C
toolchain, no migration, nobody's store touched.

A user running two agent hosts opts into SQLite and gets a memory both can
share. The cost lands only on the people who choose it, which is the whole
argument — A charges everyone for a problem some have, B charges the people
who have it, and D charges the people who fix it.

The 16 conformance scenarios are engine-agnostic and become the acceptance
criteria for the second engine, plus a new scenario the suite has never had:
two processes, one store, both writing, no lost events. An engine that cannot
pass it is not a valid backend.

This also keeps every earlier option open. If redb's multi-process work lands
well, the seam makes adopting it a backend change rather than a rewrite, and
the SQLite backend can be retired or kept for the platforms where it wins. If
it stalls, the second engine is already shipping.

### The layout is the gate

A SQLite store stamps `FORMAT_VERSION = 2`. Not because the logical event
format changed — it did not, and bundles from either engine are byte-identical
— but because the version number is the only thing a binary that predates
this decision honours. A 0.1.x binary opening a SQLite directory would read
`1`, look for `store/kernel.redb`, not find it, and create an empty one beside
the real store: silent empty memory, the exact failure ADR-012 exists to
prevent. With `2` it stops with "newer than this binary supports; upgrade the
binary", which is the truth. So `FORMAT_VERSION` names the *layout* — which
engine wrote `store/`, and how — and the logical event format lives in its
own constant, `EVENT_FORMAT_VERSION`, which is what bundles and migrations
carry.

A store is never reopened as another engine's: opening a redb directory as
SQLite, or the reverse, is refused by name. Switching engines is a migration
into a fresh directory, replaying the event log — the same operation a format
bump has always been.

### Staging

The seam is the risk, not the engine. Three steps, each one green before the
next:

1. Introduce the storage seam and move the redb implementation behind it, no
   behaviour change. Conformance stays green — that is the proof the seam is
   faithful.
2. Add the SQLite backend behind the feature. Conformance runs against both.
3. Engine selection, the multi-process conformance scenario, and the docs
   that say which engine to pick and why.

The port surface makes this tractable: 49 call sites across six operations
(`insert`, `get`, `iter`, `len`, `range`, `remove`, `last`) over eleven
tables that are all key-to-JSON maps. There is no relational query to
rewrite.

## Consequences

- **Positive:** two agent hosts can share one memory, for anyone who opts in.
- **Positive:** the default build is unchanged — pure Rust, one file, no C
  toolchain, no migration, static musl and Windows still simple.
- **Positive:** the seam turns the engine into a decision we can revisit
  cheaply, which is what ADR-009 promised and never had to prove.
- **Cost:** a storage abstraction that did not exist, and two engines to keep
  green instead of one. The conformance suite already carries most of that
  weight; the new multi-process scenario is what stops the second engine from
  silently regressing.
- **Cost, on the opt-in path only:** ~5× slower point reads, ~30% slower
  batched writes, ~+1.4MB binary, a C toolchain. Bought back with 2.5×
  smaller stores and 10× faster reopen.
- **Migration:** moving an existing store between engines replays the
  append-only event log rather than translating pages — it is the source of
  truth and projections are rebuildable. Store format is stamped and
  fail-fast checked ([ADR-012](ADR-012-embedded-data-directory.md)), so a
  store is never opened by the wrong engine.
- **[ADR-017](ADR-017-embedded-memory-viewer.md) loosens.** Its reasoning for
  an in-process viewer — "the embedded store is single-writer, so a separate
  viewer process could never watch a live agent session" — stops holding on
  the SQLite engine. In-process stays the default; a standalone viewer
  becomes possible there and is out of scope here.
