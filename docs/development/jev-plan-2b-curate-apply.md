# `kmp_curate` apply — implementation plan (2b of 3)

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans. Steps use checkbox (`- [ ]`) syntax.

**Goal:** Add `mode: "apply"` to `kmp_curate`. The agent sends the frozen
review's `review_token` and the items it accepts, each with its own `why` and
`evidence`. Jev checks each item once more against that text (the pre-write
check). Doubted items come back unwritten. The rest are written through the
same path as `kmp_write_memory`: planner, kernel neighbourhood review,
idempotency. Their evidence metadata records that curation produced them.

**Architecture:**
- **Shared write path.** `serving/write_commit.rs` extracts the commit half of
  `handle_kmp_write_memory`: plan → receipt context → neighbourhood token →
  `kmp_ingest` → result mapping. Both tools call it. It has one optional hook
  that adds metadata to every evidence item of the ingest packet.
- **Backend preparation.** The frozen review and Jev live in the embedded
  backend. `apply` therefore first asks the backend, through an internal
  `kmp_curate` call with `mode: "prepare_apply"`, to resolve the accepted
  items, enforce the allowed types, and run and freeze the pre-write check.
  `prepare_apply` is not in the public schema, so an external call using it
  is refused by argument validation.
- **Dispatch.** `serving/curate_dispatch.rs` routes `apply`. `review` keeps
  going straight to the backend.

**Spec:** `docs/development/jev-curate-design.md`, section Apply.

## Global Constraints

- Every constraint from plans 1 and 2a still holds.
- An item is doubted when support < 0.3, or when a different **offered**
  type wins with confidence ≥ 0.7. `confirm_doubted: true` on an item writes
  it anyway. Jev never refuses a write.
- Pre-write findings are frozen by the SHA-256 of the canonical accepted
  items, in an in-process cache of 16 entries. A continuation does not call
  Jev again. If Jev is unavailable, the check is skipped with a warning and
  the write goes ahead.
- One about per call: only items whose written `from` belongs to the call's
  `about` are written. The rest are `rejected` with a reason. `reverse: true`
  swaps `from` and `to` before that check.
- Allowed `rel`: `relation_options(crosses_abouts)` minus `none`. Across
  abouts only the equivalences, carried with their kernel relate proposal
  (`proposed_by` = the pair's kernel signals, restricted to `identifier`,
  `summary` and `entity`).
- Evidence metadata on every written relation: `curated_by: kmp_curate`,
  plus `curated_with: <model>` when Jev ran and `proposed_by: jev` for Jev
  pairs. `kmp_write_memory` passes no metadata, and its output is unchanged:
  `tool_surface_parity` and the write suites stay green without blessing.
- Tokens: `review_token` names the curate review. `write_review_token` is
  the kernel neighbourhood token. After `needs_review`, `next_actions[0]` is
  a `kmp_curate` apply call carrying both, never a `kmp_write_memory` call.
- `apply` needs `actor`. `call_guidance` fills it from `context_id`, as for
  the other writers. The schema's `if/then` actor rule is **not** applied,
  because review needs no actor.

## Tasks

### Task 1: Extract the shared commit path

- Create `serving/write_commit.rs` with:
  `impl KernelMcpServer { pub(super) async fn commit_write_plan(&self, arguments: &Value, plan: &KernelWritePlan, evidence_metadata: Option<&Map<String, Value>>) -> Result<Value, ToolError> }`.
  The body is `write_dispatch.rs` lines 74–113, returning the structured
  value (pending review, dry run or commit) instead of the JSON-RPC string.
  When `evidence_metadata` is `Some`, merge it into
  `ingest_arguments["memory"]["evidence"][*]["metadata"]` before the ingest.
- `handle_kmp_write_memory` keeps planning and telemetry. It calls
  `commit_write_plan(arguments, &plan, None)` and wraps the result with
  `tool_success_result`.
- Test: `cargo test -p kmp-mcp --test relation_write_admission --test relation_write_retries --test writer_relation_triples --test tool_surface_parity` stays green without blessing.
- A unit test on the metadata merge (a packet with two evidence items gains
  the keys on both) lives with the merge helper `fn with_evidence_metadata(ingest: &mut Value, metadata: &Map<String, Value>)` in `write_commit.rs`.
- Commit: `refactor(mcp): share the write commit path between writers`.

### Task 2: Pre-write check and apply preparation (use case)

- Domain:
  - `curate/domain/apply_item.rs`: `ApplyItem { item_id: String, why: String, evidence: String, confidence: Option<String>, rel: Option<String>, reverse: bool, confirm_doubted: bool }`.
  - `curate/domain/apply_rejection.rs`: `ApplyRejection { item_id: String, reason: String }`.
  - `curate/domain/apply_doubt.rs`: `ApplyDoubt { item_id: String, support: f64, best: JevVerdict }`.
- Application:
  - `curate/application/prepared_relation.rs`: `PreparedRelation { item_id, from, to, rel, why, evidence, confidence: Option<String>, proposal: Option<Vec<String>>, origin: PairOrigin }`.
  - `curate/application/prepared_apply.rs`: `PreparedApply { relations: Vec<PreparedRelation>, doubted: Vec<ApplyDoubt>, rejected: Vec<ApplyRejection>, jev: Option<JevUsage>, warnings: Vec<String> }`.
  - `judgement_plan::precheck_request(material: &CurateMaterial, items: &[PreparedRelation]) -> JudgementRequest`: `s<n>` is a noul on whether the agent's `why` and `evidence` support `rel`; `b<n>` is a choice for the best type. Same wording as `suspect_request`.
  - `use_cases/prepare_apply.rs`: `PrepareApply<'a> { judgement: Option<&'a dyn JudgementModel> }` with `async fn run(&self, about: &str, review: &CurateReview, material: &CurateMaterial, items: Vec<ApplyItem>, frozen: Option<Vec<ApplyDoubt>>) -> (PreparedApply, Vec<ApplyDoubt>)`.
    1. Resolve each `item_id` against the review's `Missing` findings, numbered as the DTO numbers them (`m<n>`). If unknown or a `Suspect` id: rejected, "not a missing item of this review".
    2. Apply `reverse`.
    3. Ownership: the from-fact must belong to `about`, else rejected.
    4. `rel` = the item's `rel`, or the finding's `suggested_rel`. It must be in `relation_options(crosses)` and not `none`, else rejected.
    5. Proposal for crossing pairs: the kernel signals ∩ {identifier, summary, entity}. If empty: rejected.
    6. Doubts: use `frozen` when given; otherwise one `precheck_request` (if Jev is configured and there are items). Items with a doubt that is not `confirm_doubted` move from `relations` to `doubted`.
    7. Return the prepared result and the full doubt list (for freezing).
- Tests with the `Scripted` model from 2a:
  - an unknown id and a foreign-about item are rejected;
  - a doubted item is withheld;
  - the same item with `confirm_doubted` is kept;
  - with `frozen` given, no Jev call is made;
  - `rel` `none` or a structural type is rejected;
  - a crossing pair carries its proposal.
- Commit: `feat(mcp): prepare curate apply with a frozen Jev pre-write check`.

### Task 3: Backend `prepare_apply`, pre-check cache and DTO

- `serving/adapters/curate_precheck_cache.rs`: `CurateDoubtCache`, the same
  shape as `CurateReviewCache`, keyed by digest, holding `Vec<ApplyDoubt>`.
  It is a field of the embedded backend.
- `EmbeddedCurateTool::call` handles `mode: "prepare_apply"`: it reads
  `review_token`, `accepted[]` and `about`, gets the review and material from
  the cache (or returns `review expired; run a fresh review`), computes the
  digest of the canonical accepted JSON, runs `PrepareApply` with the frozen
  doubts, inserts the new ones, and returns
  `curate_application_dto::prepared_to_value(&PreparedApply)`:

```json
{"relations":[{"item_id":"m0","from":"…","to":"…","rel":"supports","why":"…","evidence":"…","confidence":"high","proposal":["identifier"] | null,"proposed_by":"kernel"}],
 "doubted":[{"item_id":"m1","support":0.12,"suggested_rel":"updates_state","jev":{"confidence":0.8,"top":[…]}}],
 "rejected":[{"item_id":"m9","reason":"…"}],
 "jev":{"model":"jev-1.13.0","requests":1,"input_tokens":900} | null,
 "warnings":[]}
```

- Test: a DTO unit test for `prepared_to_value`.
- Commit: `feat(mcp): serve curate apply preparation on the embedded store`.

### Task 4: Apply dispatch through the shared write path

- `serving/curate_dispatch.rs`, `handle_kmp_curate(id, arguments, start) -> String`:
  - If `mode` ≠ `apply`: the backend call as today, via the generic path. The dispatcher checks `apply` only.
  - Apply:
    1. Require `actor` (`invalid_argument` otherwise).
    2. Call `backend.call_tool("kmp_curate", {mode: "prepare_apply", about, review_token, accepted})`.
    3. If there are no relations: return `{status: "nothing_written", doubted, rejected, jev, warnings, next_actions: []}`.
    4. Otherwise build the write arguments:
       `{about, actor, relations: [{from, to, rel, why, evidence, confidence?}], read_context: {relate_proposals: [{from, to, proposed_by}]}?, idempotency_key?, review_token: write_review_token?}`,
       then call `build_relation_plan` and `commit_write_plan(&write_args, &plan, Some(metadata))`.
    5. Metadata: `{curated_by: "kmp_curate"}`, plus `curated_with` from `jev.model` when present. When any relation has `proposed_by: jev`, add `proposed_by: "jev"`. This is per packet: document that a packet mixing kernel and Jev pairs is tagged with `jev`.
    6. If the result's `status` is `needs_review`, replace `next_actions[0]` with `{tool: "kmp_curate", arguments: original + {write_review_token: <neighbourhood token>, idempotency_key: <key>}}`.
    7. Add `curate: {doubted, rejected, jev, warnings}` to the result.
    8. Telemetry uses the name `kmp_curate`.
  - Route it in `rpc_dispatch.rs` next to `kmp_relabel`: `if name == "kmp_curate" && arguments["mode"] == "apply"`.
- Contract (`contract/tools/curate.rs`):
  - `mode` enum `["review", "apply"]`.
  - New properties: `accepted` (array of `{item_id, why, evidence, confidence?, rel?, reverse?, confirm_doubted?}`), `write_review_token`, `idempotency_key` and `actor`.
  - Extend the output description with `status`, `curate`, and the write result fields.
  - Update the description: apply writes through the writer's own review.
  - Add `kmp_curate` to the `call_guidance.rs` actor-filling list only.
- Integration tests in `tests/curate_review.rs` (renamed `curate.rs`), without Jev:
  1. Review, then apply one item with `why` and `evidence`. Expect `needs_review`, then run `next_actions[0]` unchanged and expect `committed`. `kmp_relate` then shows the declared relation, and `kmp_inspect` on the evidence shows `curated_by: kmp_curate`.
  2. Apply the same call again: `replayed`.
  3. A foreign-about item is rejected without a write.
  4. `mode: prepare_apply` from outside is refused.
- Bless the surface fixtures (the schema changed). Check that the review
  fixture response did not change. Update the guide texts (`verbs/curate.md`
  step 5, `cards/curate.md` step 4, `examples/curate-review.md` step 4) so that
  `kmp_curate apply` becomes the declaration path, then regenerate the guide
  and bless `kmp_guide.json`.
- Run every gate from plan 2a Task 5, Step 9.
- Commit: `feat(mcp): apply curated relations through the writer's own path`.

### Task 5: Operator path

- On a fresh copy of the store, with Jev: review `project:made`, pick two
  items, and apply them with written `why` and `evidence`:
  - one sound item, which should pass the pre-check;
  - one deliberately weak `why`, which should come back in `doubted`.
- Then confirm the weak one with `confirm_doubted`, resume the
  `needs_review`, and verify both relations with `kmp_relate`. Record the
  outcome in the plan's Deviations section and in memory. Never touch the
  original store.

## Deviations recorded during execution
- Tasks 2 and 3 were committed together: the use case's types have no
  consumer until the backend serves `prepare_apply`, and clippy denies dead
  code.
- `CurateReview::numbered()` now owns the `m<n>`/`s<n>` ids, and the review DTO
  and `PrepareApply` both read them, so they cannot disagree.
- The pre-check digest excludes `confirm_doubted`, so confirming an item
  reuses the frozen doubts instead of calling Jev again.
- The curate output schema is the writer's output schema plus review's fields
  and `curate`, because `apply` answers with the writer's result.
  `write_memory_output_schema` became `pub(crate)`.
- `tests/curate_review.rs` was renamed `tests/curate.rs`. It proves that the
  written evidence carries `curated_by: kmp_curate` through `kmp_inspect`.
- Task 5, first live run (fresh copy of the store, `project:made`): the
  `why` "The weather was sunny that day." came back doubted at support 0.03
  and was not written. The sound PR181 `updates_state` passed. That run found
  a real bug: the resumed apply used frozen doubts without the checking model,
  so `curated_with` vanished, the packet changed, the kernel token no longer
  matched, and review ran twice. The written relation also lacked
  `curated_with`. Fixed with `FrozenCheck` (the checking model plus its
  doubts, frozen whole, exposed as `checked_by`). Second run: one resume to
  `committed`, and all three evidence items carry `curated_by: kmp_curate`
  and `jev-1.13.0`. The weak item was written only after `confirm_doubted`.
  The pre-check cost 1 request of 1,910 input tokens.
