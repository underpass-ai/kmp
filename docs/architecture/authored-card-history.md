# Authored card history

Resolves [#816](https://github.com/underpass-ai/kmp/issues/816).

## Contract

A Condense card is authored presentation state. It does not change the source
memory and is not evidence, but its prose cannot be regenerated from the source
body. Consequently its accepted versions belong in the event log.

Each accepted write appends a `node_card` change to the about's `node_cards`
aggregate, independently of its canonical memory aggregate. The payload records
the full card: node, language, text, source revision/hash/record digest/body size,
author, authorship instant and compare-and-set revision. The event envelope also
records when the kernel accepted it. Source validation, card compare-and-set,
event append and projections commit in one SQLite transaction. A refusal or
projection error commits none of them.

`NodeCardEvent` owns the event representation. The application derives a
`RecordNodeCard` projection mutation; the embedded adapter applies that mutation
both during live writes and during replay. The current `node_cards` projection
and immutable `node_card_versions` projection contain no independent truth.
The latter is indexed by node, language, normalized authorship instant and card
revision. Historical reads load at most one version per requested slot, then
apply the ordinary source-version and cutoff checks. They do not scan prose
from all prior revisions. Equal authorship instants are ordered by revision.

Rebuild reads the event frontier and replaces projections under the same write
transaction, preventing a concurrent writer's accepted card from disappearing
between the log read and projection replacement. Canonical node/body projections
are derived only from their own events.

## Portability and upgrade

Full and about-filtered bundles, named snapshots and rebuild preserve recorded
card revisions. Bundles with card history advertise event format 3. Memory-only
bundles retain event format 2; the current reader supports both, while an older
reader refuses format 3. Import checks card payload integrity and revision
continuity before replaying any event.

SQLite layout format 4 fences older binaries from reopening the extended log.
Format-3 stores retain their SQLite file and upgrade their format stamp before
card adoption. Stop older processes before upgrading a shared store: a format
stamp cannot stop a process that already has the database open.

On first open, the surviving pre-event cards are recorded as explicit `BASELINE`
events, with their original authorship and card revision. The baseline event's
acceptance time is the adoption time. Its revision may be greater than one;
previously overwritten prose is unknown and is never invented. Adoption and its
migration receipt commit together, so interruption is retryable and later opens
do not rescan all cards. Invalid legacy cards fail explicitly without deleting
the source data.

Adoption extends the local event stream. If the maintained project bundle is
behind afterward, export the extended history before another guarded write;
do not relax the existing stale-checkout guard. Subsequent successful MCP
Condense calls publish the maintained bundle before reporting success, using
the same guard as Ingest. A publication failure retains its pending marker.

## Verification

- `node_card_history_tests.rs`: event/body separation, two-card history,
  historical selection, rebuild/reopen/export/import, legacy baseline adoption,
  concurrent compare-and-set, rollback and malformed import preflight.
- `condense_candidates.rs`: public MCP writes publish the event-bearing bundle;
  rejected compare-and-set and stale checkout leave it unchanged.
- Existing card policy, compact pagination, reader-card and gRPC parity tests
  continue to cover source checks, proof boundaries and transport behavior.

Tool effect annotations remain a separate review. Event sourcing proves that
authored revisions are retained; it does not turn a write into a read-only tool.
