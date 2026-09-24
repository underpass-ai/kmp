# Agent token optimization — paired Wake report (#544 I3)

Generated from `report.json` by `python -m scripts.performance.token_harness render`; edit the harness or rerun it, not this file.

- Evidence scope: `native_replay`; model calls: 0.
- Tokenizer: tiktoken 0.14.0 o200k_base; representations: `mcp_json_compact_legacy_v1`, `json_compact_lexical_v1`, `structured_compact_v1`, `content_blocks_standalone_v1`.
- Tasks: 6; journeys: 11; attempts per journey: 1; paired rows: 44.
- Configuration mismatch: none.
- Harness code SHA-256: `285248e05c8e52d37659eecf4da1b26d621296bc50805f74cba0568ddf4c97ca`.

| variant | run | commit | binary SHA-256 | wake contract | journeys | capture failures | incomplete |
|---|---|---|---|---|---|---|---|
| baseline | baseline | a22b6402c5ac76248fdd78b6e48a9425784c0dc8 | d391f6a96a6163aba6a011e14552fff531eb4f3f7cf0b65e4c8aabe0971485f9 | kmp.wake_claim.v1 | 11 | 0 | 0 |
| candidate | candidate | f171b3a7ec2307d3970db87a756e0393ea9cc8a3 | 9a969e20c6f4d174373dc8ebdcbc2627bf246d2bd58df08491b802e395c0db68 | kmp.wake_claim.v2 | 11 | 0 | 0 |

Limitations:

- L1 native replay of synthetic fixtures; no agent, host context or billing was observed.
- Reference tokens are whole-unit counts under the declared encoder and representation.
- One deterministic capture per variant; ids and clocks differ between fresh stores.
- Scenarios run: W01-W04, A01, E01. T01, G03 and the 512-byte recovery budget of annex A.12 are not implemented yet.
- requested_scope_exhausted is reported, not required: an orientation may end with items excluded by detail.
- support_bookkeeping_in_state_count (supports lines in current_state) is descriptive; the verdict uses support_displacement_count, supports lines present while a required memory is missing from state (annex A.11.2).
- No latency, cache temperature, rehydration or host (H4/H5) measurement.

## Conclusions

`descriptive_only`: 6, `quality_fix_not_equivalent_compression`: 24, `quality_regression_or_failure`: 8, `reference_increase_with_quality_pass`: 6 (rows over every representation).

## Oracle per journey

| journey | variant | oracle | unresolved refs | body as ref | supports lines in state | displaced memories | relation as action | equal bodies merged | contract violations | post-as_of memory in state (figure) | first unit after rpc | task ready after rpc | rpc | failed checks |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
| a01-b10000 | baseline | pass | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 1 | 1 | 1 | — |
| a01-b10000 | candidate | pass | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 1 | 1 | 1 | — |
| a01-b4096 | baseline | pass | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 1 | 1 | 2 | — |
| a01-b4096 | candidate | pass | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 1 | 1 | 2 | — |
| e01 | baseline | pass | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 1 | 1 | 1 | — |
| e01 | candidate | pass | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 1 | 1 | 1 | — |
| w01-b10000 | baseline | FAIL | 6 | 6 | 5 | 3 | 1 | 0 | 0 | 0 | 1 | — | 3 | task_obligations_satisfied, unresolved_evidence_reference_count, evidence_body_disguised_as_ref_count, support_displacement_count, historical_relation_as_action_count |
| w01-b10000 | candidate | pass | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 1 | 1 | 4 | — |
| w01-b4096 | baseline | FAIL | 6 | 6 | 5 | 3 | 1 | 0 | 0 | 0 | 2 | — | 3 | task_obligations_satisfied, unresolved_evidence_reference_count, evidence_body_disguised_as_ref_count, support_displacement_count, historical_relation_as_action_count |
| w01-b4096 | candidate | pass | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 2 | 2 | 3 | — |
| w02-b10000 | baseline | FAIL | 2 | 2 | 2 | 2 | 0 | 0 | 0 | 0 | 1 | — | 1 | task_obligations_satisfied, unresolved_evidence_reference_count, evidence_body_disguised_as_ref_count, support_displacement_count |
| w02-b10000 | candidate | pass | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 1 | 1 | 1 | — |
| w02-b4096 | baseline | FAIL | 2 | 2 | 2 | 2 | 0 | 0 | 0 | 0 | 4 | — | 9 | task_obligations_satisfied, unresolved_evidence_reference_count, evidence_body_disguised_as_ref_count, support_displacement_count |
| w02-b4096 | candidate | pass | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 2 | 2 | 2 | — |
| w03-b10000 | baseline | FAIL | 1 | 1 | 2 | 1 | 0 | 2 | 0 | 0 | 1 | — | 1 | task_obligations_satisfied, unresolved_evidence_reference_count, evidence_body_disguised_as_ref_count, support_displacement_count, identical_body_merged_citation_count |
| w03-b10000 | candidate | pass | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 1 | 1 | 1 | — |
| w03-b4096 | baseline | FAIL | 1 | 1 | 2 | 1 | 0 | 2 | 0 | 0 | 4 | — | 9 | task_obligations_satisfied, unresolved_evidence_reference_count, evidence_body_disguised_as_ref_count, support_displacement_count, identical_body_merged_citation_count |
| w03-b4096 | candidate | pass | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 2 | 2 | 2 | — |
| w04-b10000 | baseline | FAIL | 1 | 1 | 4 | 1 | 0 | 0 | 0 | 4 | 1 | — | 1 | task_obligations_satisfied, unresolved_evidence_reference_count, evidence_body_disguised_as_ref_count, support_displacement_count |
| w04-b10000 | candidate | FAIL | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 2 | 1 | — | 1 | task_obligations_satisfied |
| w04-b4096 | baseline | FAIL | 1 | 1 | 4 | 1 | 0 | 0 | 0 | 4 | 4 | — | 6 | task_obligations_satisfied, unresolved_evidence_reference_count, evidence_body_disguised_as_ref_count, support_displacement_count |
| w04-b4096 | candidate | FAIL | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 2 | 2 | — | 2 | task_obligations_satisfied |

### Unmet obligations

- baseline `w01-b4096`: `state_mentions` "Use blue-green deploys for the ledger service." → FAIL
- baseline `w01-b4096`: `state_mentions` "A second ledger pool is provisioned." → FAIL
- baseline `w01-b4096`: `state_mentions` "Cutover is scheduled for Friday." → FAIL
- baseline `w01-b10000`: `state_mentions` "Use blue-green deploys for the ledger service." → FAIL
- baseline `w01-b10000`: `state_mentions` "A second ledger pool is provisioned." → FAIL
- baseline `w01-b10000`: `state_mentions` "Cutover is scheduled for Friday." → FAIL
- baseline `w02-b4096`: `state_mentions` "Export billing records as parquet files." → FAIL
- baseline `w02-b4096`: `state_mentions` "The parquet exporter passed the schema check." → FAIL
- baseline `w02-b10000`: `state_mentions` "Export billing records as parquet files." → FAIL
- baseline `w02-b10000`: `state_mentions` "The parquet exporter passed the schema check." → FAIL
- baseline `w03-b4096`: `state_mentions` "Disk usage on node-7 reached 91 percent." → FAIL
- baseline `w03-b10000`: `state_mentions` "Disk usage on node-7 reached 91 percent." → FAIL
- baseline `w04-b4096`: `state_mentions` "Freeze schema changes during the audit." → FAIL
- baseline `w04-b4096`: `as_of_excludes` "Lift the schema freeze after the audit closed." → FAIL
- baseline `w04-b10000`: `state_mentions` "Freeze schema changes during the audit." → FAIL
- baseline `w04-b10000`: `as_of_excludes` "Lift the schema freeze after the audit closed." → FAIL
- candidate `w04-b4096`: `as_of_excludes` "Lift the schema freeze after the audit closed." → FAIL
- candidate `w04-b10000`: `as_of_excludes` "Lift the schema freeze after the audit closed." → FAIL

## Paired table (`json_compact_lexical_v1`)

Whole-journey reference tokens: startup (initialize, initialized, tools/list) plus every memory call the server directed.

| case_id | journey | goal_id | encoding | baseline_tokens | candidate_tokens | delta_tokens | reduction_fraction | baseline_rpc | candidate_rpc | first unit (base) | first unit (cand) | quality_baseline | quality_candidate | comparable | conclusion |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
| w01 | w01-b4096 | orient_small_about | o200k_base | 60707 | 61974 | 1267 | -0.0209 | 3 | 3 | 2 | 2 | FAIL | pass | yes | quality_fix_not_equivalent_compression |
| w01 | w01-b10000 | orient_small_about | o200k_base | 60583 | 63172 | 2589 | -0.0427 | 3 | 4 | 1 | 1 | FAIL | pass | yes | quality_fix_not_equivalent_compression |
| w02 | w02-b4096 | resume_explicit_label | o200k_base | 63603 | 55689 | -7914 | 0.1244 | 9 | 2 | 4 | 2 | FAIL | pass | yes | quality_fix_not_equivalent_compression |
| w02 | w02-b10000 | resume_explicit_label | o200k_base | 54535 | 54473 | -62 | 0.0011 | 1 | 1 | 1 | 1 | FAIL | pass | yes | quality_fix_not_equivalent_compression |
| w03 | w03-b4096 | orient_identical_bodies | o200k_base | 63210 | 55749 | -7461 | 0.1180 | 9 | 2 | 4 | 2 | FAIL | pass | yes | quality_fix_not_equivalent_compression |
| w03 | w03-b10000 | orient_identical_bodies | o200k_base | 54614 | 54478 | -136 | 0.0025 | 1 | 1 | 1 | 1 | FAIL | pass | yes | quality_fix_not_equivalent_compression |
| w04 | w04-b4096 | dated_wake | o200k_base | 59689 | 55198 | -4491 | 0.0752 | 6 | 2 | 4 | 2 | FAIL | FAIL | yes | quality_regression_or_failure |
| w04 | w04-b10000 | dated_wake | o200k_base | 54168 | 53920 | -248 | 0.0046 | 1 | 1 | 1 | 1 | FAIL | FAIL | yes | quality_regression_or_failure |
| a01 | a01-b4096 | ask_short_source | o200k_base | 54657 | 54671 | 14 | -0.0003 | 2 | 2 | 1 | 1 | pass | pass | yes | reference_increase_with_quality_pass |
| a01 | a01-b10000 | ask_short_source | o200k_base | 53370 | 53384 | 14 | -0.0003 | 1 | 1 | 1 | 1 | pass | pass | yes | reference_increase_with_quality_pass |
| e01 | e01 | simple_write_receipt | o200k_base | 52711 | 52725 | 14 | -0.0003 | 1 | 1 | 1 | 1 | pass | pass | yes | reference_increase_with_quality_pass |

### Stage split and time to first unit (`json_compact_lexical_v1`)

| journey | startup (base) | startup (cand) | memory (base) | memory (cand) | memory delta | until first unit (base) | until first unit (cand) | until task ready (base) | until task ready (cand) |
|---|---|---|---|---|---|---|---|---|---|
| w01-b4096 | 52075 | 52089 | 8632 | 9885 | 1253 | 57676 | 57828 | — | 57828 |
| w01-b10000 | 52075 | 52089 | 8508 | 11083 | 2575 | 55035 | 55193 | — | 55193 |
| w02-b4096 | 52075 | 52089 | 11528 | 3600 | -7928 | 57284 | 55689 | — | 55689 |
| w02-b10000 | 52075 | 52089 | 2460 | 2384 | -76 | 54535 | 54473 | — | 54473 |
| w03-b4096 | 52075 | 52089 | 11135 | 3660 | -7475 | 57045 | 55749 | — | 55749 |
| w03-b10000 | 52075 | 52089 | 2539 | 2389 | -150 | 54614 | 54478 | — | 54478 |
| w04-b4096 | 52075 | 52089 | 7614 | 3109 | -4505 | 57316 | 55198 | — | — |
| w04-b10000 | 52075 | 52089 | 2093 | 1831 | -262 | 54168 | 53920 | — | — |
| a01-b4096 | 52075 | 52089 | 2582 | 2582 | 0 | 53341 | 53355 | 53341 | 53355 |
| a01-b10000 | 52075 | 52089 | 1295 | 1295 | 0 | 53370 | 53384 | 53370 | 53384 |
| e01 | 52075 | 52089 | 636 | 636 | 0 | 52711 | 52725 | 52711 | 52725 |

## Cohorts

weighted_reduction = 1 - sum(candidate) / sum(baseline) over rows with both counts; failing rows stay in the cohort.

| encoding | representation | group | n | excluded | baseline sum | candidate sum | weighted_reduction | median delta | worst journey | worst delta | conclusions |
|---|---|---|---|---|---|---|---|---|---|---|---|
| o200k_base | content_blocks_standalone_v1 | all | 11 | 0 | 1790 | 653 | 0.6352 | 0.0000 | w02-b10000 | 1 | descriptive_only: 3, quality_fix_not_equivalent_compression: 6, quality_regression_or_failure: 2 |
| o200k_base | content_blocks_standalone_v1 | b10000 | 5 | 0 | 319 | 288 | 0.0972 | 1.0000 | w02-b10000 | 1 | descriptive_only: 1, quality_fix_not_equivalent_compression: 3, quality_regression_or_failure: 1 |
| o200k_base | content_blocks_standalone_v1 | b4096 | 5 | 0 | 1450 | 344 | 0.7628 | -227.0000 | a01-b4096 | 0 | descriptive_only: 1, quality_fix_not_equivalent_compression: 3, quality_regression_or_failure: 1 |
| o200k_base | content_blocks_standalone_v1 | write | 1 | 0 | 21 | 21 | 0.0000 | 0.0000 | e01 | 0 | descriptive_only: 1 |
| o200k_base | json_compact_lexical_v1 | all | 11 | 0 | 631847 | 615433 | 0.0260 | -62.0000 | w01-b10000 | 2589 | quality_fix_not_equivalent_compression: 6, quality_regression_or_failure: 2, reference_increase_with_quality_pass: 3 |
| o200k_base | json_compact_lexical_v1 | b10000 | 5 | 0 | 277270 | 279427 | -0.0078 | -62.0000 | w01-b10000 | 2589 | quality_fix_not_equivalent_compression: 3, quality_regression_or_failure: 1, reference_increase_with_quality_pass: 1 |
| o200k_base | json_compact_lexical_v1 | b4096 | 5 | 0 | 301866 | 283281 | 0.0616 | -4491.0000 | w01-b4096 | 1267 | quality_fix_not_equivalent_compression: 3, quality_regression_or_failure: 1, reference_increase_with_quality_pass: 1 |
| o200k_base | json_compact_lexical_v1 | write | 1 | 0 | 52711 | 52725 | -0.0003 | 14.0000 | e01 | 14 | reference_increase_with_quality_pass: 1 |
| o200k_base | mcp_json_compact_legacy_v1 | all | 11 | 0 | 631847 | 615433 | 0.0260 | -62.0000 | w01-b10000 | 2589 | quality_fix_not_equivalent_compression: 6, quality_regression_or_failure: 2, reference_increase_with_quality_pass: 3 |
| o200k_base | mcp_json_compact_legacy_v1 | b10000 | 5 | 0 | 277270 | 279427 | -0.0078 | -62.0000 | w01-b10000 | 2589 | quality_fix_not_equivalent_compression: 3, quality_regression_or_failure: 1, reference_increase_with_quality_pass: 1 |
| o200k_base | mcp_json_compact_legacy_v1 | b4096 | 5 | 0 | 301866 | 283281 | 0.0616 | -4491.0000 | w01-b4096 | 1267 | quality_fix_not_equivalent_compression: 3, quality_regression_or_failure: 1, reference_increase_with_quality_pass: 1 |
| o200k_base | mcp_json_compact_legacy_v1 | write | 1 | 0 | 52711 | 52725 | -0.0003 | 14.0000 | e01 | 14 | reference_increase_with_quality_pass: 1 |
| o200k_base | structured_compact_v1 | all | 11 | 0 | 52256 | 39677 | 0.2407 | -77.0000 | w01-b10000 | 2479 | descriptive_only: 3, quality_fix_not_equivalent_compression: 6, quality_regression_or_failure: 2 |
| o200k_base | structured_compact_v1 | b10000 | 5 | 0 | 15851 | 17839 | -0.1254 | -77.0000 | w01-b10000 | 2479 | descriptive_only: 1, quality_fix_not_equivalent_compression: 3, quality_regression_or_failure: 1 |
| o200k_base | structured_compact_v1 | b4096 | 5 | 0 | 35951 | 21384 | 0.4052 | -3594.0000 | w01-b4096 | 1369 | descriptive_only: 1, quality_fix_not_equivalent_compression: 3, quality_regression_or_failure: 1 |
| o200k_base | structured_compact_v1 | write | 1 | 0 | 454 | 454 | 0.0000 | 0.0000 | e01 | 0 | descriptive_only: 1 |

## A/A controls

The same verified artifacts compared with themselves must give zero deltas.

| control | label | baseline run | candidate run | rows | non-zero deltas | rows without counts |
|---|---|---|---|---|---|---|
| aa-baseline.json.gz | aa-baseline | baseline | baseline | 44 | 0 | 0 |
| aa-candidate.json.gz | aa-candidate | candidate | candidate | 44 | 0 | 0 |
