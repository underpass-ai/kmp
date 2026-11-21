# Evidence-group acceptance controls

Issue [#538](https://github.com/underpass-ai/kmp/issues/538) concerns whether a
selected context carries every stored passage a declared proof needs. This page
records the controls that are reproducible in this repository and the boundary
of what they establish.

## Fixed source control and local FTS5 comparator

`crates/kmp-testkit/tests/fixtures/proof_support.json` is a six-passage source
fixture. It was written before the FTS5 control and is kept self-contained:
Alba's `Nora` alias, the R19 responsibility, the boundary-time qualifier, a
later source, a distractor and Boreal's separate `Nora`. The comparison fixes:

| Input | Native lexical control | SQLite FTS5 lexical control |
| --- | --- | --- |
| Eligible sources | original source evidence at or before observed `2026-09-10T10:00:00Z` | the same source evidence and cutoff |
| Question / lexical input | `Who is accountable for R19 in Alba at the boundary?` through `kmp_ask` | every token from that frozen question, quoted and joined by SQLite FTS5 `OR` |
| Reader evidence allowance | read the one complete 200,000-byte native selection, then pack ranked source bodies into 512 bytes | pack the FTS5-ranked source rows into the same 512 UTF-8 bytes |
| Required passages | `alias`, `role`, `boundary` | the same judged set |
| Preparation/query cost | the test prints fresh-store seeding and native query microseconds separately | the test prints FTS5 index preparation and query microseconds separately |

Run it with:

```sh
cargo test --locked -p kmp-testkit --test proof_support_fts5_baseline -- --nocapture
```

The runner emits one JSON record per frozen query with selected source IDs,
complete-support state, source-body bytes, native fresh-store seeding/query
timings and FTS5 preparation/query timings. It separately records native
`structured_content_bytes`; this is not whole MCP transport size. It parses RFC3339 instants before FTS5 insertion, so the source
at `10:00:00.500Z` is excluded from the `10:00:00Z` cut. Timing values are
diagnostic and vary by host.

At this fixed lexical input, native ranking retains `role` and `boundary`
(140 packed source bytes), leaving alias support absent. FTS5's literal-OR
ranking retains `role`, `other_alias`, `boundary`, `distractor` and `alias`
(383 packed source bytes), which happens to meet the fixture's complete-support
judgment. This is an observed difference, not a claim that one result is
better: source packing does not assess answer quality or whether the extra FTS5
passages are useful to a reader. Native MCP transport bytes are reported
separately from the packed source-body allowance. The test records wall-clock
values only as local diagnostics; they are neither a performance claim nor a
CI threshold.

This is a local lexical retrieval control, not a complete external-memory
baseline. It does not assess reader answers, a different memory system, model
costs or general quality. The separate `retrieval_proof_support` route control
starts from the declared R19 anchor and proves that dependency navigation can
deliver the three passages; it is an expansion ablation, so it is deliberately
not combined with this lexical comparison. It must not be cited as satisfying
#538's external baseline criterion.

## Deterministic acceptance matrix

| #538 requirement | Reproducible control | Status and boundary |
| --- | --- | --- |
| Alias, ambiguous name, reassignment, temporal update, missing constraint, conflict and redundant proof cases | `retrieval_proof_support`, `evidence_paths/{joint,traversal,context,anchors}.rs`, and `trace_proof.rs` | Native fixtures cover aliases scoped by workshop, multiple compatible/incompatible proofs, unknown clocks, future identity admission, missing witnesses, temporal validity and duplicate paths. The A17 reassignment fixture retains Alice before the cut and Alice/Bob as separate later candidates; it never emits `same_entity_as`. |
| Recover required alias passages while retaining ambiguity | `focused_role_needs_both_older_identity_and_boundary_qualifier`; `chronological_read_retains_two_scoped_names_without_equating_them` | Complete support requires body plus exact source evidence. The two `Nora` records never create `same_entity_as`. |
| Apply one temporal selection to nodes, relations and support, including boundaries | `retrieval_proof_support`, `temporal_label_admission`, `trace_proof`, `evidence_paths/traversal.rs` | Observed cutoff excludes the source 500ms later; late relations are not admitted early; validity and unknown clocks remain explicit. |
| Do not fill absent proof from a fixture or model | `a_missing_witness_is_not_filled_from_another_route`; proof-support judgments | Missing witnesses remain named. Fixture registries judge delivered content after the read; they do not augment it. No model is called. |
| Stable selection, direct eligibility, continuations and omissions | `evidence_seek.rs`, `joint_trace_proof.rs`, `evidence_paths/traversal.rs` | Embedded/gRPC pages share a fingerprint; cuts remain `Partial`; named expansion cannot introduce out-of-selection refs. |
| Compare lexical retrieval with a source-only control at matched cutoff and allowance | `proof_support_fts5_baseline.rs` | Both arms receive the frozen question, source cutoff and 512-byte allowance. The control reports delivered sources, complete-support state and separate SQLite preparation/query diagnostics. It has no reader and does not compare graph expansion with FTS5. |
| Complete-support, abstention and retrieval-cost measurements | `retrieval_scorecard`, `retrieval_kmp_scorecard`, `proof_support_fts5_baseline` | `has_complete_support_at(k)`, recall and false-UNKNOWN are deterministic retrieval measures. FTS5 reports separate prep/query diagnostics. They do not judge answers. |
| Extend #467's offline regression with multi-passage source support | `retrieval_proof_support` plus the existing judged `retrieval_kmp_scorecard` collection | Covered. The #467 Ask scorecard remains separate from the graph/temporal controls. |
| Reproducible external memory/retrieval baseline with matched reader settings and costs | none | **Pending.** It needs a separately frozen comparator, identical task routing and reader settings, plus preparation/query accounting. The local FTS5 control above is evidence for one lexical boundary only. |

No independent reader or old experiment is used here. The deterministic controls
are safe to rerun on fresh temporary stores; they do not open or modify frozen
evaluation stores.

## Pending external comparison protocol

The external criterion remains open until a comparator and blind readers are
actually available. The executable task is deliberately fixed before choosing
either system:

1. Freeze new source packets and sealed judgments separately. The reader gets
   source text, questions, cuts and explicit total byte/token allowances; it
   never gets required passage ids or expected answers.
2. Ingest the identical sources into fresh KMP and comparator stores. Record
   source hashes, adapter/version hashes, preparation wall time, calls, bytes
   and tokens. Do not reuse either system's closed experimental store.
3. Run semantic-only, temporal-only and combined routes. For each route, run
   ablations that remove labels, routing and complete-group selection one at a
   time while keeping sources, cut, reader and total allowance fixed.
4. Cover old alias, scoped homonyms, evidenced identity, unsupported identity,
   reassignment before/after the cut, restriction, conflict, currency and
   temporal quantities. Preserve the current native lost-alias outcome as a
   named regression case.
5. Require each arm to complete source → justified write → navigation → source
   audit → answer. Save native requests/responses and the exact source bodies
   consulted by the reader. A retrieval-only result is incomplete.
6. A separate evaluator opens the sealed judgments after both arms finish and
   reports complete support, grounded-answer accuracy, unsupported identity,
   correct abstention, input/output tokens, total bytes/calls, preparation time
   and query/reader time. Failures and rejected calls stay in the denominator.

Before execution, the run manifest must name `run_id`, source/judgment hashes,
KMP and comparator versions, adapter hashes, reader model/configuration,
temperature/seed where supported, UTC cut, route, ablation, total byte and token
allowances, concurrency and whether startup is included. Both arms must match
all fields except system/adapter identity. This protocol supplies no comparator
name, reader or result; those remain concrete prerequisites rather than values
inferred from the local FTS5 control.
