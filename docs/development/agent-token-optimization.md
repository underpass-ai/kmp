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

Usage, from the repository root (`prepare` is the only step that touches the
network; `verify` needs no tokenizer):

```bash
TT="uv run --no-project --with tiktoken==0.14.0 python"
$TT -m scripts.performance.token_harness prepare        # assets into tmp/tiktoken-cache
python3 -m scripts.performance.token_harness verify --run artifacts/<evidence>/journeys-main
$TT -m scripts.performance.token_harness measure --run artifacts/<evidence>/journeys-main --out <new file>
python3 -m unittest discover -s scripts/performance/token_harness/tests -t .
KMP_RUN_TIKTOKEN_INTEGRATION=1 $TT -m unittest discover -s scripts/performance/token_harness/tests -t .
```

The replay traces keep parsed objects, not wire bytes, so on them
`json_compact_lexical_v1` equals the legacy view; the two diverge only on
captures that retain original lexemes.

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

Delivered on `feat/544-wake-oracle` (same package as I0):

- `scenarios/`: W01–W04, A01, E01 as data; each fixture is written through the
  binary's own `kmp_write_memory` (review rounds followed verbatim).
- `native/`: disposable store per scenario and variant, child environment
  built from an allowlist with `HOME`, `XDG_*`, `CODEX_HOME` and
  `KMP_MCP_DATA_DIR` inside a fresh temp dir; the binary's startup log must
  confirm that directory by the `env` rule before any tool call. One process
  per session (initialize → initialized → tools/list → calls); driver
  `kmp.native_driver.v1` follows the server's `next_action` verbatim at 4096
  and 10000 bytes. Traces keep wire lexemes; `<case>.fixture.jsonl`
  (fixture_preparation) and `<case>.oracle.jsonl` (E01 read-back) are hashed
  beside the journeys but never measured as them.
- `oracle/`: deterministic checks; wake claim contracts `kmp.wake_claim.v1`
  (`evidence_ref`) and `v2` (`evidence_refs`) declared per capture and
  validated strictly. Besides the metrics named above it reports
  `identical_body_merged_citation_count` (one hop citing two sources with the
  same text) and `support_displacement_count`, which the verdict uses; the raw
  count of `--supports-->` lines stays descriptive.
- `compare/`: re-verifies both runs, binds metrics and oracle to the manifest
  digest, rejects mixed encoders and a capture repeated under another name,
  pairs by journey × encoding × representation with the A.14 columns; `render`
  writes the Markdown.

```bash
H="python3 -m scripts.performance.token_harness"
TT="uv run --no-project --with tiktoken==0.14.0 python -m scripts.performance.token_harness"
A=artifacts/544-wake-oracle-20260924
$H capture --binary <copy of main kmp-mcp> --variant baseline --wake-contract kmp.wake_claim.v1 \
  --build-provenance $A/baseline-build-provenance.json --out $A/baseline
$H capture --binary <copy of candidate kmp-mcp> --variant candidate --wake-contract kmp.wake_claim.v2 \
  --build-provenance $A/candidate-build-provenance.json --out $A/candidate
$TT measure --run $A/baseline --encoding o200k_base --out $A/baseline-metrics.json   # same for candidate
$H oracle --run $A/baseline --out $A/baseline-oracle.json                             # same for candidate
$H compare --baseline-run $A/baseline --baseline-metrics ... --candidate-run $A/candidate ... --out $A/report.json
$H render --report $A/report.json --control $A/aa-baseline.json.gz --out docs/development/agent-token-optimization-report.md
```

W04 follows the wake scope contract (`crates/kmp-mcp/tests/wake_scope.rs`):
`current_state` is about context. When the packet declares it in
`scope.context` with `scope.context_time == "unbounded"`, a memory later than
`as_of` may appear there. The oracle then requires the declared
`scope.selection` (proof, causal spine, resume cursor, guardrails) to exclude
it. Without that declaration, state must exclude it too. The raw count stays
visible as `post_as_of_memory_in_state_count`.

Run of 2026-09-24 (baseline `main` a22b6402, candidate `fix/544-wake-scope`
f171b3a7, PR #840): the candidate passes W01, W02, W03, A01 and E01, and fails
W04 at both budgets.

- W04: at `as_of` 2026-09-10 the declared selection still names the lift
  (observed 2026-09-20). `wake.causal_spine` carries the structural claims
  `fixture:w04 -> evidence:…lift…:current` and `…:relation:1` ("Memory anchor
  includes this evidence item."). `proof` itself excludes the lift. The lift
  also appears twice in `current_state` (the memory and its `supersedes`
  line), which the declared unbounded context allows.
- W01 is larger in the candidate: +1267 tokens at 4096 and +2589 at 10000
  (4 calls instead of 3). This is a quality fix, not compression.
Not delivered: T01 and G03, the 512-byte recovery budget, latency and cache
temperature, H4/H5. The report lists them as limitations.

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
| Plan + CI scope | `docs/544-token-plan` | #834 | merged |
| I0 | `feat/544-token-meter` | #836 | merged; legacy totals reproduced exactly on the 2026-09-15 captures |
| I1 | `fix/544-wake-evidence-refs` | #835 | merged; contract wake pins 3 cited sources in page 1 (was 0) |
| I2 | `fix/544-wake-state` | #837 | draft; state from live memories, `Next: none recorded` |
| I3 | `feat/544-wake-oracle` | #839 | draft; against #840 the candidate passes W01–W03/A01/E01 and fails W04 (causal spine names anchor→evidence claims of a memory later than `as_of`) |

Compatibility policy (maintainer, 2026-09-24): lighter and better wins; a
contract break is acceptable when it serves that. Breaks are named in each PR.
