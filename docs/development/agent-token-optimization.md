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
c88472b9, PR #840): the candidate passes all six scenarios at both budgets.
The baseline passes only A01 and E01. W04 at `as_of` still lists the later
lift in `current_state` (2 lines, reported as
`post_as_of_memory_in_state_count`). This is allowed because the packet
declares that field as unbounded context and the selection excludes the lift.

W01 is now smaller in the candidate: −918 tokens at 4096 and −797 at 10000,
3 calls each. Its 12 `proof.path` hops cite 27 evidence ids, the same as the
baseline; efb5b6ae cited 62. Summed over the three responses at 10000:

- `proof.evidence`: +463. The pinned spine evidence repeats on every page.
- Savings: `current_state` −387, `causal_spine` −563, `next_actions` −96,
  content text and summary −87 each.

A01 is −5/−20 tokens with both passing (`reference_reduction_with_quality_pass`).
E01 is +14, from the larger `tools/list` catalogue.
Not delivered: T01 and G03, the 512-byte recovery budget, latency and cache
temperature, H4/H5. The report lists them as limitations.

### C1 — Catalogue without output schemas

Every session pays `tools/list` at startup, and the output schemas were 43 %
of it. Claude Code presents MCP tools to the model with their input parameters
only (observed in the host's own tool listing), so there `outputSchema` was
transport cost with no reader. Other hosts are unverified, so the schemas are
opt-in rather than deleted: MCP makes `outputSchema` optional and
`structuredContent` is still returned without it.

- Default `tools/list` omits every `outputSchema`; nothing else changes.
- `KMP_MCP_OUTPUT_SCHEMAS=1` (stdio, viewer and HTTP startup) advertises them;
  unset or `0` omits them, any other value refuses to start.
- The schemas stay in `contract/` and tests validate against them there.
  `fixtures/contract/tools_list_with_output_schemas.json` pins the opt-in
  catalogue and is byte-identical to the previous `tools_list.json`.
- Shared passages extend a read's `outputSchema` only when it is advertised.

Measured on the pinned fixtures, whole document as compact JSON
(`ensure_ascii=False`), `tiktoken 0.14.0` / `o200k_base`, `encode_ordinary`:

| Catalogue | Bytes before | Bytes after | Tokens before | Tokens after |
| --- | ---: | ---: | ---: | ---: |
| Default (`tools_list.json`) | 248,704 | 137,654 | 51,629 | 29,460 (−42.9 %) |
| MCP Apps (`tools_list_with_apps.json`) | 253,687 | 142,637 | 52,825 | 30,655 (−42.0 %) |
| Opt-in (`KMP_MCP_OUTPUT_SCHEMAS=1`) | 248,704 | 248,704 | 51,629 | 51,629 |

Contract break, named: a client that read `outputSchema` from the default
catalogue must now set `KMP_MCP_OUTPUT_SCHEMAS=1`.

### PR 3 — Incremental continuation pages

I3 showed Wake and Ask repeating the stable core on every continuation: in
W01 at 10000 B the pinned spine evidence cost +463 tokens over three calls,
and `truncation` restated `projection` on every page.

- A continuation (`page.cursor` past page 1) carries only new expansion
  items, `projection` and its own warning, and marks
  `projection.core_reused=true`. Summary, answer, because, scope, clocks,
  pinned proof and core warnings travel once. The cursor still binds the
  whole core by hash, so another snapshot is still rejected.
- `page.repeat_core=true` resends the core for a host that lost page 1
  (compaction, reconnect); the action it proposes returns to incremental
  pages. A continuation whose next item does not fit reports a stall and a
  sufficient budget instead of shortening a core it does not send.
- `truncation` is retired (JSON and proto); `projection` is the one progress
  block and keeps every omission cause apart: `page.offset` (earlier pages),
  `sections.*.remaining` (not yet delivered), `excluded_by_detail`,
  `selection_omitted` (entries cap), `core_text_shortened`.
- Reconstruction tests: page 1 plus every continuation equals a single
  large-budget read, for typed Wake, typed Ask and the JSON projection.

Contract break, named: default continuation shape, `truncation` removed
(`WakeResponse`/`AskResponse` field 7 reserved, `RecallTruncation` and
`RecallOmitted` deleted), `kmp.recall.projection.v2`.

Measured against the integration head d339c02f (C1 included on both sides),
same machine and harness, `o200k_base`, `json_compact_lexical_v1`, whole
journey; all 11 journeys pass the oracle in both runs, A/A controls zero:

| Journey | 4096 B | 10000 B | Calls 4096 | Calls 10000 |
| --- | ---: | ---: | --- | --- |
| W01 | −666 | −1,895 | 3 → 3 | 3 → 2 |
| W02 | −2,264 | −13 | 4 → 2 | 1 → 1 |
| W03 | −1,214 | −13 | 3 → 2 | 1 → 1 |
| W04 | −352 | −13 | 2 → 2 | 1 → 1 |
| A01 | −1,315 | −13 | 2 → 1 | 1 → 1 |
| E01 (write) | +46 | | 1 → 1 | |

Weighted −3.4 % at 4096 B, −1.2 % at 10000 B. Startup is +46 tokens (the
`repeat_core` input property on two tools); every single-page read is −59 in
memory calls (no `truncation`). W01 at 10000 B: `proof.evidence` 1,942 →
1,532 tokens, `projection` + `truncation` 1,103 → 584.

### C2 — One time-navigation verb

After C1 the four temporal tools were a third of the default catalogue:
`kmp_rewind`, `kmp_forward`, `kmp_goto` and `kmp_near` carried the same
input schema (~1,650 tokens each) and differed only in the name of their
cursor. They are now one tool, `kmp_time`, with a required
`move: rewind | forward | goto | near` and the union of their arguments.
Each move keeps its cursor name — `from` (or `interval` alone) for rewind and
forward, `at` for goto, `around` for near — so a call changes only by its name
and one field. The guide verb was already `verb:time`.

- Schema: the shared object stays at the root, as
  `agent-schema-conditions.md` requires. The per-move requirement is a root
  `anyOf` of three branches keyed on `move` (the same device rewind already
  used for `from`/`interval`), which the continuation wrapper moves under
  `else` like every other initial-call requirement.
- Server: `TimeMove::from_arguments` refuses a missing or unknown move
  (`TIME_INVALID_MOVE`), another move's cursor (`TIME_CURSOR_MISMATCH`) and a
  move without its cursor (`TIME_MISSING_CURSOR`), naming the field and the
  accepted call, before any backend reads the call. Hosts that ignore the
  conditional schema get the same answer.
- Semantics unchanged: every temporal response fixture is byte-identical
  except its actions. `next_actions`, `detail_action`, `READ_SELECTION_CHANGED`
  restarts and Trace's `review_context` now read
  `{"tool":"kmp_time","arguments":{…,"move":"<move>"}}`.
- Old names answer with an unknown-tool error that names `kmp_time` and the
  move. The hidden `kernel_goto|near|rewind|forward` aliases are gone. The
  lifecycle holds engines up to 0.19.0 to the four old tools.

Measured on the pinned fixtures, whole document as compact JSON, harness
counter (`tiktoken 0.14.0`, `o200k_base`, verified asset):

| Catalogue | Bytes before | Bytes after | Tokens before | Tokens after |
| --- | ---: | ---: | ---: | ---: |
| Default (`tools_list.json`) | 137,872 | 116,604 | 29,506 | 24,899 (−15.6 %) |
| MCP Apps (`tools_list_with_apps.json`) | 142,855 | 121,587 | 30,701 | 26,094 (−15.0 %) |
| Opt-in (`KMP_MCP_OUTPUT_SCHEMAS=1`) | 248,270 | 199,384 | 51,545 | 41,601 (−19.3 %) |

The four definitions cost 6,588 tokens and `kmp_time` costs 1,994. Tool count
18 → 15 (22 → 19 with MCP Apps).

Contract break, named: `kmp_goto`, `kmp_near`, `kmp_rewind`, `kmp_forward`
and their `kernel_*` aliases are removed; call `kmp_time` with `move`.
Continuation handles saved under an old name no longer resolve; submit the
original read again.

### C3 — Minimal continuation actions and lean progress

After PR 3 every paged response still paid two costs. `next_action` restated
the whole request beside the cursor (W02 at 4096 B: 327 bytes, 115 tokens, of
which the cursor was 74 bytes), and the host sent that echo back as the next
request. `projection.sections` carried five counters per section on every
page, zeros included; Time pages listed all ten sections even when nine were
empty (639 bytes, 181 tokens per response).

- Actions. The MCP server retains the call a page proposes and returns
  `{"continuation":"read_<32 hex>"}`: every cursor page (Wake, Ask, Trace,
  Relate, Inspect, `kmp_time` with its `move`) and a write resuming its
  review. The existing handle store gains a context-free table
  (`open_continuations`) beside the store in `agent-users.sqlite3` (24 h,
  256 calls, 32 KiB each), so a handle outlives a restarted process; a server
  without a path keeps it in memory. HTTP resolves before it authorizes, as
  for context handles. Restarts (`core_text_shortened`, `feedback[].action`)
  and time navigation stay readable: the agent has to choose them.
- Validation. Only Wake/Ask `page.repeat_core=true` may accompany a handle;
  any other argument is `invalid_argument` naming the stateless form
  (original arguments plus `page.cursor`), which still works. The retained
  call keeps its cursor, so a changed selection is still `conflict` with a
  full restart; an expired or foreign handle is `CONTINUATION_UNAVAILABLE`.
- Progress. `projection.sections` (`kmp.recall.projection.v3`) omits zero
  counters and empty sections. `core` and `excluded_by_detail` (was
  `total - eligible`) appear on the page that carries the core; `eligible`
  and `total` are retired and derivable (mapping in `agent-surface.md`).
  Proto: `RecallProjectionSection` 4/5 reserved, `excluded_by_detail = 7`.
  Time and Inspect `page.sections` keep `returned_on_page` and `remaining`;
  Inspect `page.omitted`, a copy of `remaining`, is retired.
- The catalogue's `continuation` descriptions are shorter: default
  `tools/list` 24,899 → 24,754 tokens (116,604 → 115,847 bytes).

Measured against the integration head dcb8b5d8 (C1, PR 3, C2 and #844 on both
sides), same machine and harness, `o200k_base`, `json_compact_lexical_v1`,
whole journey; all 11 journeys pass the oracle in both runs, call counts
unchanged, A/A controls zero:

| Journey | 4096 B | 10000 B |
| --- | ---: | ---: |
| W01 | −473 | −409 |
| W02 | −434 | −199 |
| W03 | −386 | −199 |
| W04 | −425 | −199 |
| A01 | −173 | −173 |
| E01 (write) | −145 | |

Weighted −1.3 % at 4096 B, −0.85 % at 10000 B (startup −145 per session).
Summed over the 11 journeys: `next_action` 1,375 → 568 bytes (497 → 212
tokens), `projection.sections` 5,520 → 2,269 bytes (1,672 → 602 tokens),
continuation request arguments 1,087 → 280 bytes (409 → 124 tokens). On the
pinned fixtures, outside the harness scenarios: a Time page's sections
181 → 9 tokens, Inspect sections plus `omitted` 90 → 20, a write-review
resume 173 → 27 tokens (packet of the neighborhood fixture).

Contract break, named: continuation actions are handles, not complete calls;
a client that edited an action must send the original arguments with
`page.cursor`. `eligible`/`total` and Inspect `page.omitted` are gone. Direct
gRPC `next_call` still carries the complete call.

Deferred: the recall projection still sizes a page with the complete action,
so the ~130 bytes a handle frees are not refilled with items; Inspect
`page.repeat_object` cannot accompany a handle; byte counts inside warnings
(the Inspect floor) are measured before shortening and are upper bounds;
restart actions still echo default budget fields; the remaining per-page
envelope (`contract`, budget echo, `false`/`null`/`0` fields) is untouched;
the harness has no Time, Inspect, Trace or review scenario (T01/G03).

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
| I2 | `fix/544-wake-state` | #837 | merged; state from live memories, `Next: none recorded` |
| I2b | `fix/544-wake-scope` | #840 | merged; hops join sources by the graph (W03), spine without bookkeeping (W04), no endpoint over-citation (W01) |
| I3 | `feat/544-wake-oracle` | #839 | merged; against #840 (c88472b9) the candidate passes W01–W04/A01/E01 and is smaller on every wake and ask journey; E01 +14. Weighted −6.9 % at 4096 B, −1.0 % at 10000 B |
| C1 — catalogue without output schemas | `feat/544-lean-catalogue` | #841 | draft; default `tools/list` 51,629 → 29,460 tokens (−42.9 %), 248,704 → 137,654 bytes; `KMP_MCP_OUTPUT_SCHEMAS=1` restores the previous catalogue byte for byte |
| PR3 — incremental continuation pages | `feat/544-incremental-pages` | #842 | draft; continuations carry only new items (`projection.core_reused`), `page.repeat_core=true` rehydrates, `truncation` retired; all 11 journeys pass; −3.4 % at 4096 B, −1.2 % at 10000 B, W01 −1,895 tokens and 3 → 2 calls at 10000 B |
| C2 — one time-navigation verb | `feat/544-time-verb` | #843 | draft; `kmp_time` + `move` replaces goto/near/rewind/forward; default `tools/list` 29,506 → 24,899 tokens (−15.6 %), 137,872 → 116,604 bytes; 18 → 15 tools; temporal responses unchanged except action shape |
| C3 — minimal continuation actions and lean progress | `feat/544-lean-actions` | #845 | draft; continuations are `{continuation}` handles to the retained call, sections without zero or derivable counters (`kmp.recall.projection.v3`); all 11 journeys pass, every one smaller: −1.3 % at 4096 B, −0.85 % at 10000 B; `next_action` 497 → 212 tokens, sections 1,672 → 602 over the run |
| Consolidated main → integration | `docs/544-consolidated-report` | PR_TBD | draft; main a22b6402 against integration 23f95fd3 (I1–I3, C1, PR 3, C2, C3), report in `agent-token-optimization-report.md`. Startup 52,075 → 25,214 tokens per session (`tools/list` 51,650 → 24,789). Candidate passes 11/11 journeys, baseline 3/11 (A01 at both budgets, E01). Weighted whole journey −53.7 % at 4096 B, −50.4 % at 10000 B, −52.0 % overall; memory calls only −66.8 %, −31.7 %, −56.1 %. Calls 37 → 17 over the 11 journeys; A/A controls zero |

Compatibility policy (maintainer, 2026-09-24): lighter and better wins; a
contract break is acceptable when it serves that. Breaks are named in each PR.
