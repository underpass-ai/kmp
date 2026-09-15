# Acceptance audit for issues 653 and 683

This audit runs against `fc352d7d37ac0480cc79120a6288bfd446fa4c86`,
the fetched `origin/main` on 2026-09-15. PR 805 is present as squash
`3637de5f161d81b92f31f94c29cf735db21b91c0`. The locally built `kmp-mcp`
binary has SHA-256
`8ff8ca3217639aa61d72d258b0c8b880803e3bd7d8705fb666d14bd38ca72675`.

## Issue 653: negative kernel finding

The historical Marea report and the final-main control answer the same four
cuts:

| Axis and inclusive instant | Selected state |
| --- | --- |
| occurred 11:02 | R8 execution; no H8 entry or H8 relation endpoint |
| occurred 11:05 | R8 execution and H8 verification |
| observed 11:02 | no X3/F17 records |
| observed 12:00 | both records, learned at report reception |

`TEMPORAL-VERIFICATION-20260910.md` establishes those selections on consistent
copies of the three closed Marea stores. `writer_clock_audit.rs` reproduces the
boundaries on final main with a separate authored store, adds receipt relation
inspection, and checks a support association declared at 12:10 on both observed
and ingested axes.

The selected kernel graph and the writer's prose are separate levels. Luna and
Sol put later recoverability wording in the 11:00 execution record. A record
filter cannot split that sentence, but the 11:02 occurrence selection still
excludes H8 as an entry and as a proof endpoint. Terra used separate records.
The observed-axis cut answers when the investigator had the report; it must not
be replaced by the fresh Luna run's actual 2026-09-14 ingestion observation.

Result: this audit does not reproduce a temporal-selection defect. The issue's
negative-outcome acceptance is met. It does not establish that every writer
will choose separate records, understand the source, or improve because of a
guide change.

## Issue 683: native acceptance matrix

| Criterion | Final-main control | Result |
| --- | --- | --- |
| old active constraint | `old_constraint_and_explicit_conflict_precede_recent_noise` | retained before recent noise |
| explicit conflict | same control | both endpoints retained; no conflict omitted |
| same-batch relation | `local_rich_links_review_before_commit_even_without_strict_validation` | review required before commit, including `strict:false` |
| absent context | `review_does_not_offer_a_wake_for_a_new_proposed_about` | proposed-only about has no dead expansion; continuation commits |
| scope isolation | `irrelevant_scope_changes_reuse_review_and_simple_observations_need_no_review` plus the old-constraint control | unrelated scope excluded and does not invalidate review |
| partial budget | `long_source_is_omitted_whole_and_exact_inspect_preserves_qualification` | omission declared; exact Inspect action preserves full qualifier |
| omitted stored context | `review_retains_stored_about_expansion_when_compact_items_omit_it` | `stored_abouts` retains an executable Wake owner |
| stale context | `changed_neighborhood_refreshes_and_a_corrected_proposal_cannot_reuse_the_token` | stale review refreshes without committing |
| idempotent retry | `an_exact_retry_replays_and_the_same_key_with_a_different_link_is_refused` | exact replay keeps evidence identity; changed content is refused |
| restart | `retained_write_survives_restart_and_preserves_generated_identity` | reviewed identity and acceptance survive restart |
| readable/absent clocks | three clock-rendering controls in `write_neighborhood_selection.rs` | RFC3339, preserved precision, no invented clock |

Focused local execution passed 16 tests: 7 neighborhood selection, 5
neighborhood commit, 3 relation retry, and 1 writer clock audit. The merged PR's
full quality gate is GitHub Actions run 34906749789; main after all combined
merges passed run 34933839163.

## Fresh final-main writer and review boundary

The earlier Luna trace is diagnostic evidence only. It used the pre-fix runtime,
encountered the dead Wake, and defaulted observations to its 2026-09-14 ingestion
instead of modeling the historical noon receipt. It cannot close the final-main
writer criterion.

`FRESH-SOURCE.md` supplies a distinct N41/C7 case. The driver seeds an isolated
store with old policy and noise, synchronizes the shipped guide before MCP
initialization, and records full requests, responses, rejections, continuations,
UTF-8 bytes and local call latency. The fresh writer does not receive the seed
payload, issue criteria, tests, prior traces or expected answers. Its accepted
receipt must be inspected for literal source fidelity, relation direction and
independent entry/relation clocks. An exact retry must be retained.

After the writer completes, preserve the store under this artifact directory
and prepare ChronoLoom at its exact refs. Human ChronoLoom review must happen
before any independent experimental reader. Until that review occurs, this
audit must report the boundary rather than describe an automated inspection as
human review. Delivery of context is not proof of comprehension, and this
single writer has no comparison arm from which to infer causal improvement.

Sol completed this protocol on final main. The accepted representation has six
entries, five relations and eleven evidence items. It separates J4 at 09:00,
C7 at 09:07, the quantity-only correction at 09:08, unresolved R3 conflict at
09:09, the 10:00 source receipt and the 10:15 association. Its five relations
preserve `transfer -> verified_by -> C7`, `63 -> corrects -> 62`,
`61 -> contradicts -> 63`, and both older constraints. Relation occurrence and
validity remain absent. An exact retry replayed the accepted receipt and its
original ingestion clock.

The first packet was rejected for invalid kinds. Sol then noticed a reversed
`verified_by` direction in its own proposal, consulted the served neighborhood
and guide, corrected it, completed the context pages and resumed the opaque
continuation. This is observed behavior from one writer, not a causal comparison
or proof of comprehension. `source_coverage` remains `not_assessed`.

The source and instructions accidentally omitted the seeded about identifier.
Three guessed Wake calls returned `not_found`; the coordinator then supplied
only the exact seeded about, `project:fresh-final-main-n41`. Those failures and
the correction remain in the trace. They do not become source facts or an
expected semantic answer.

The complete session contains 33 native calls, 20,040 request bytes, 438,931
response bytes and 113,088 visible `o200k_base` tokens. Recorded local call
latency totals 1,221.360 ms, median 16.200 ms, nearest-rank p95 84.829 ms and
maximum 285.749 ms. These are driver-side local timings, not a benchmark. The
driver made no model/API calls.

`CHRONOLOOM-REVIEW.md` names the exact six refs and human checklist on a
byte-identical store copy. Human review is still pending, so #683's ordering
condition is preserved and no independent reader has run.

## Reproduction

```bash
cargo test --locked -p kmp-mcp \
  --test writer_clock_audit \
  --test write_neighborhood_selection \
  --test write_neighborhood_commit \
  --test relation_write_retries
```

The historical Marea source stores and the earlier fresh Luna store were not
modified. Test stores live under repository `tmp/` and are disposable; writer
stores and original traces live under this artifact directory.
