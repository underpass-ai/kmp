# Agent token optimization — integration track

Opened: 2026-09-24, from `main` a22b6402 (v0.19.0). Continues #544; it is not
a new initiative and does not close #544 on its own.

Inputs: the maintainer's review *KMP: optimización de la superficie y del
recorrido del agente* (findings F1–F9, stages 0 and PR 1–5) and its annex A
(*harness de medición de tokens*, increments H0–H5). Both were written against
a22b6402 without compiling KMP; every claim below that says "verified" was
re-checked in this worktree.

## Goal

Cheaper complete agent tasks with the same evidence, not shorter JSON. A
smaller response that loses a source, a negation, a clock or a continuation is
a regression, whatever it saves. A semantic fix that grows a packet is
reported as a quality fix, never as compression.

Out of scope: JEV or any remote evaluator, ranking changes, auxiliary models,
new relations generated automatically, store migrations for transport reasons,
absolute token quotas as CI gates.

## Branch and CI policy

| Item | Value |
| --- | --- |
| Integration branch | `integration/agent-token-optimization`, cut from `main` a22b6402 |
| Child branches | `feat/544-*`, `fix/544-*`, `docs/544-*` from the integration head |
| Child PRs | Draft, base = integration branch. Drafts run `dev-loop` (fmt, clippy, tests of `DEV_PACKAGES`, architecture ratchet, arm64 binary) in minutes |
| Tracking PR | Draft, integration → `main`. Marked ready only when the track is releasable; then the full `quality-gate` runs |
| `DEV_PACKAGES` | Widened on this branch to `-p kmp-mcp -p kmp-proto-mapping`, where PR 1 lives |
| Local gates before push | `cargo fmt`, `cargo clippy`, crate tests, `scripts/ci/kmp-mcp-architecture-gate.sh`; when the contract changes also `scripts/ci/mcp-registry.sh`, `documentation-spine.sh`, `kmp-capability-contract.py` and the vendored-proto check |

Released bugs still go to `main` directly; this branch merges `main` when it
needs them.

## Increments

Order follows the review's recommendation: baseline plus the Wake repair
first, everything that promises savings only after measurement.

### I0 — Token meter (annex H0–H1)

`scripts/performance/token_harness/`, Python 3.11+, stdlib + `tiktoken==0.14.0`.

- `domain/`: `TokenizerIdentity`, `Exposure`, `Measurement`,
  `RepresentationId`; immutable dataclasses; `null` with a reason, never an
  invented zero.
- `adapters/tiktoken_counter.py`: explicit `get_encoding` (`o200k_base`,
  `cl100k_base`), `encode_ordinary`, strict UTF-8, no normalization, asset
  SHA-256 checked against the pinned table before counting, cache directory set
  once; a missing or altered asset raises `TOKENIZER_NOT_READY` and never
  downloads inside a measurement.
- Representations: `mcp_json_compact_legacy_v1` (what
  `surface_acceptance_metrics.py` counted), `json_compact_lexical_v1`
  (whitespace outside strings removed, numbers and escapes untouched),
  `structured_compact_v1`, `content_blocks_standalone_v1`. Views overlap;
  totals are never summed across them.
- `verify`: manifest hashes and lengths before reading; paths outside the root,
  duplicate entries and oversized gzip rejected; an unpaired request is a
  visible failure, not a silently dropped event.
- `measure`: whole-unit counts per exposure over existing journey captures
  (`acceptance_surface_journeys.py` output), stage split kept
  (startup / guide / memory).
- Tests: unit tests with a fake counter for our accounting; one integration
  test gated by `KMP_RUN_TIKTOKEN_INTEGRATION=1` that fails, not skips, when
  enabled without the tokenizer.

Done when the historical journeys re-measure to the same
`mcp_json_compact_legacy_v1` totals as `surface_acceptance_metrics.py` and a
tampered capture is rejected.

### I1 — Wake evidence identity (review F1, PR 1 part 1)

Verified: `wake_response_from_result` fills `WakeClaim.evidence_ref` with
`relationship.evidence` (prose), while `recall_projection/plan.rs` treats every
`evidence_ref` as a `proof.evidence[].id` to pin in the core. Prose never
matches an id, so the evidence object that would carry identity and provenance
is pushed to expansion while its body sits in the causal spine.

Change:

1. Failing regression first: a Wake whose causal spine cites evidence must pin
   the matching `proof.evidence` object in the first page and never emit a body
   where an id is expected.
2. `WakeClaim` gains `repeated string evidence_refs` resolved from the typed
   graph — evidence nodes incident to the relation's endpoints or joined by a
   `supports` edge to them, the same rule `normalized_proof_relation` already
   applies to `proof.path`. No text matching across abouts, no ids built from
   text.
3. When nothing resolves, the relation's evidence text stays inline in a
   separate `evidence` field and the claim says so; it is never presented as a
   ref.
4. Field 3 `evidence_ref` is retired (`reserved`), not repurposed. Canonical
   proto, vendored copy, schemas, `typed_response.rs`, fixtures and guide
   examples change together.

No token saving is claimed; the pinned core may grow.

### I2 — Wake state and actions (review F2–F3, PR 1 part 2)

Verified: `rendered_current_state` ranks every non-structural relation first,
so five `--supports-->` sentences can fill `current_state`; `next_actions` and
`open_loops` are parsed from the L0 summary's `Next:` / `Blocker:` lines, which
can name a historical `updates_state` relation.

Change: `current_state` built from typed memories and their lifecycle, with
`supports` / membership edges demoted below memory content (they stay in
proof). `next_actions` only from memories that record an action with
provenance; when none is recorded, say so without claiming nothing is pending
outside the read scope. `updates_state` is never read as `supersedes`, and
unbounded context is never shown as state at `as_of`.

Tests: many-supports/few-facts, historical-relation-is-not-an-action,
superseded-decision-not-current, dated wake.

### I3 — Wake fixtures, oracle and paired report (annex H2–H3)

Synthetic fixtures W01–W04, A01, T01, G03, E01 (annex §A.12) loaded into
disposable stores whose path is validated before the first write. A
deterministic oracle per scenario (`first_supported_unit_after_rpc`,
`unresolved_evidence_reference_count`, `evidence_body_disguised_as_ref_count`,
`support_bookkeeping_in_state_count`, completeness split into
transport / selected packet / requested scope / task obligations). Comparator
emits the annex conclusion states; A/A on identical artifacts must be equal.

Baseline = `main` binary, candidate = integration binary, same profile and
machine. Report in `docs/development/agent-token-optimization-report.md`,
generated from `report.json`.

### Backlog, gated by I3 measurements

| Review item | Starts when |
| --- | --- |
| PR 2 — evidence units selected before paging, opt-in representation | I3 shows orientation cost dominated by unit fragmentation |
| PR 3 — incremental continuations, targeted review expansion | I3 shows repeated cores or full-about review expansions dominating |
| PR 4 — guide/skills "when to reuse, when to expand" | PR 2/3 give the output the data to decide |
| PR 5 — catalogue text and compact write receipt | Host capture (H5) shows catalogue/receipt cost dominating |
| H4–H5 — latency, a real Codex session | After I3; H5 needs an authorized host capture |

## Status

| Increment | Branch | PR | State |
| --- | --- | --- | --- |
| Plan + CI scope | `docs/544-token-plan` | — | in progress |
| I0 | `feat/544-token-meter` | — | pending |
| I1 | `fix/544-wake-evidence-refs` | — | pending |
| I2 | `fix/544-wake-state` | — | pending |
| I3 | `feat/544-wake-oracle` | — | pending |
