# Agent token optimization — consolidated report, main → integration (#544)

Generated from `report.json` by `python -m scripts.performance.token_harness render`; edit the harness or rerun it, not this file.

- Evidence scope: `native_replay`; model calls: 0.
- Tokenizer: tiktoken 0.14.0 o200k_base; representations: `mcp_json_compact_legacy_v1`, `json_compact_lexical_v1`, `structured_compact_v1`, `content_blocks_standalone_v1`.
- Tasks: 6; journeys: 11; attempts per journey: 1; paired rows: 44.
- Configuration mismatch: none.
- Harness code SHA-256: `1b74193ce7b74b14e7e1f018b154e3cd3214f3b8b42a80e28927217e12ebd324`.

| variant | run | commit | binary SHA-256 | wake contract | journeys | capture failures | incomplete |
|---|---|---|---|---|---|---|---|
| baseline | baseline | a22b6402c5ac76248fdd78b6e48a9425784c0dc8 | d391f6a96a6163aba6a011e14552fff531eb4f3f7cf0b65e4c8aabe0971485f9 | kmp.wake_claim.v1 | 11 | 0 | 0 |
| candidate | candidate | 23f95fd320670a351253a872095c3f3276c8ee2c | 2d23064e6687e1fc2372ba72c7e43690c02a308a54eff5a4d33602bf717b2369 | kmp.wake_claim.v2 | 11 | 0 | 0 |

Limitations:

- L1 native replay of synthetic fixtures; no agent, host context or billing was observed.
- Reference tokens are whole-unit counts under the declared encoder and representation.
- One deterministic capture per variant; ids and clocks differ between fresh stores.
- Scenarios run: W01-W04, A01, E01. T01, G03 and the 512-byte recovery budget of annex A.12 are not implemented yet.
- requested_scope_exhausted is reported, not required: an orientation may end with items excluded by detail.
- support_bookkeeping_in_state_count (supports lines in current_state) is descriptive; the verdict uses support_displacement_count, supports lines present while a required memory is missing from state (annex A.11.2).
- No latency, cache temperature, rehydration or host (H4/H5) measurement.

## Headline (`json_compact_lexical_v1`)

### Startup per session

initialize, notifications/initialized and tools/list, requests and responses; every journey below pays it once.

| variant | encoding | initialize | initialized | tools/list | startup tokens | startup bytes | distinct totals across sessions |
|---|---|---|---|---|---|---|---|
| baseline | o200k_base | 407 | 18 | 51650 | 52075 | 250660 | 52075 |
| candidate | o200k_base | 407 | 18 | 24789 | 25214 | 117880 | 25214 |

### Journeys

"memory" excludes startup; "with startup" is the whole journey. A negative change is a reduction.

| journey | budget | calls | memory (base) | memory (cand) | memory delta | memory change | with startup (base) | with startup (cand) | delta | change | oracle (base) | oracle (cand) | conclusion |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
| w01-b4096 | 4096 | 3 → 3 | 8632 | 6759 | -1873 | -21.7 % | 60707 | 31973 | -28734 | -47.3 % | FAIL | pass | quality_fix_not_equivalent_compression |
| w01-b10000 | 10000 | 3 → 2 | 8508 | 5549 | -2959 | -34.8 % | 60583 | 30763 | -29820 | -49.2 % | FAIL | pass | quality_fix_not_equivalent_compression |
| w02-b4096 | 4096 | 9 → 2 | 11480 | 2128 | -9352 | -81.5 % | 63555 | 27342 | -36213 | -57.0 % | FAIL | pass | quality_fix_not_equivalent_compression |
| w02-b10000 | 10000 | 1 → 1 | 2460 | 1764 | -696 | -28.3 % | 54535 | 26978 | -27557 | -50.5 % | FAIL | pass | quality_fix_not_equivalent_compression |
| w03-b4096 | 4096 | 9 → 2 | 11207 | 2111 | -9096 | -81.2 % | 63282 | 27325 | -35957 | -56.8 % | FAIL | pass | quality_fix_not_equivalent_compression |
| w03-b10000 | 10000 | 1 → 1 | 2539 | 1749 | -790 | -31.1 % | 54614 | 26963 | -27651 | -50.6 % | FAIL | pass | quality_fix_not_equivalent_compression |
| w04-b4096 | 4096 | 6 → 2 | 7719 | 1647 | -6072 | -78.7 % | 59794 | 26861 | -32933 | -55.1 % | FAIL | pass | quality_fix_not_equivalent_compression |
| w04-b10000 | 10000 | 1 → 1 | 2093 | 1301 | -792 | -37.8 % | 54168 | 26515 | -27653 | -51.1 % | FAIL | pass | quality_fix_not_equivalent_compression |
| a01-b4096 | 4096 | 2 → 1 | 2582 | 1174 | -1408 | -54.5 % | 54657 | 26388 | -28269 | -51.7 % | pass | pass | reference_reduction_with_quality_pass |
| a01-b10000 | 10000 | 1 → 1 | 1295 | 1174 | -121 | -9.3 % | 53370 | 26388 | -26982 | -50.6 % | pass | pass | reference_reduction_with_quality_pass |
| e01 | write | 1 → 1 | 636 | 636 | 0 | 0.0 % | 52711 | 25850 | -26861 | -51.0 % | pass | pass | reference_reduction_with_quality_pass |

### Weighted change

change = sum(candidate) / sum(baseline) - 1 over rows with both counts; failing rows stay in.

| encoding | group | n | memory (base) | memory (cand) | memory weighted change | with startup (base) | with startup (cand) | weighted change |
|---|---|---|---|---|---|---|---|---|
| o200k_base | all | 11 | 59151 | 25992 | -56.1 % | 631976 | 303346 | -52.0 % |
| o200k_base | b10000 | 5 | 16895 | 11537 | -31.7 % | 277270 | 137607 | -50.4 % |
| o200k_base | b4096 | 5 | 41620 | 13819 | -66.8 % | 301995 | 139889 | -53.7 % |
| o200k_base | write | 1 | 636 | 636 | 0.0 % | 52711 | 25850 | -51.0 % |

## Conclusions

`descriptive_only`: 3, `quality_fix_not_equivalent_compression`: 32, `reference_reduction_with_quality_pass`: 9 (rows over every representation).

## Oracle per journey

| journey | variant | oracle | unresolved refs | body as ref | supports lines in state | displaced memories | relation as action | equal bodies merged | contract violations | post-as_of memory in state (figure) | first unit after rpc | task ready after rpc | rpc | failed checks |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
| a01-b10000 | baseline | pass | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 1 | 1 | 1 | — |
| a01-b10000 | candidate | pass | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 1 | 1 | 1 | — |
| a01-b4096 | baseline | pass | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 1 | 1 | 2 | — |
| a01-b4096 | candidate | pass | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 1 | 1 | 1 | — |
| e01 | baseline | pass | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 1 | 1 | 1 | — |
| e01 | candidate | pass | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 1 | 1 | 1 | — |
| w01-b10000 | baseline | FAIL | 6 | 6 | 5 | 3 | 1 | 0 | 0 | 0 | 1 | — | 3 | task_obligations_satisfied, unresolved_evidence_reference_count, evidence_body_disguised_as_ref_count, support_displacement_count, historical_relation_as_action_count |
| w01-b10000 | candidate | pass | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 1 | 1 | 2 | — |
| w01-b4096 | baseline | FAIL | 6 | 6 | 5 | 3 | 1 | 0 | 0 | 0 | 2 | — | 3 | task_obligations_satisfied, unresolved_evidence_reference_count, evidence_body_disguised_as_ref_count, support_displacement_count, historical_relation_as_action_count |
| w01-b4096 | candidate | pass | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 2 | 2 | 3 | — |
| w02-b10000 | baseline | FAIL | 2 | 2 | 2 | 2 | 0 | 0 | 0 | 0 | 1 | — | 1 | task_obligations_satisfied, unresolved_evidence_reference_count, evidence_body_disguised_as_ref_count, support_displacement_count |
| w02-b10000 | candidate | pass | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 1 | 1 | 1 | — |
| w02-b4096 | baseline | FAIL | 2 | 2 | 2 | 2 | 0 | 0 | 0 | 0 | 4 | — | 9 | task_obligations_satisfied, unresolved_evidence_reference_count, evidence_body_disguised_as_ref_count, support_displacement_count |
| w02-b4096 | candidate | pass | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 1 | 1 | 2 | — |
| w03-b10000 | baseline | FAIL | 1 | 1 | 2 | 1 | 0 | 2 | 0 | 0 | 1 | — | 1 | task_obligations_satisfied, unresolved_evidence_reference_count, evidence_body_disguised_as_ref_count, support_displacement_count, identical_body_merged_citation_count |
| w03-b10000 | candidate | pass | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 1 | 1 | 1 | — |
| w03-b4096 | baseline | FAIL | 1 | 1 | 2 | 1 | 0 | 2 | 0 | 0 | 4 | — | 9 | task_obligations_satisfied, unresolved_evidence_reference_count, evidence_body_disguised_as_ref_count, support_displacement_count, identical_body_merged_citation_count |
| w03-b4096 | candidate | pass | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 1 | 1 | 2 | — |
| w04-b10000 | baseline | FAIL | 1 | 1 | 4 | 1 | 0 | 0 | 0 | 4 | 1 | — | 1 | task_obligations_satisfied, unresolved_evidence_reference_count, evidence_body_disguised_as_ref_count, support_displacement_count |
| w04-b10000 | candidate | pass | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 2 | 1 | 1 | 1 | — |
| w04-b4096 | baseline | FAIL | 1 | 1 | 4 | 1 | 0 | 0 | 0 | 4 | 4 | — | 6 | task_obligations_satisfied, unresolved_evidence_reference_count, evidence_body_disguised_as_ref_count, support_displacement_count |
| w04-b4096 | candidate | pass | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 2 | 1 | 1 | 2 | — |

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

## Paired table (`json_compact_lexical_v1`)

Whole-journey reference tokens: startup (initialize, initialized, tools/list) plus every memory call the server directed.

| case_id | journey | goal_id | encoding | baseline_tokens | candidate_tokens | delta_tokens | reduction_fraction | baseline_rpc | candidate_rpc | first unit (base) | first unit (cand) | quality_baseline | quality_candidate | comparable | conclusion |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
| w01 | w01-b4096 | orient_small_about | o200k_base | 60707 | 31973 | -28734 | 0.4733 | 3 | 3 | 2 | 2 | FAIL | pass | yes | quality_fix_not_equivalent_compression |
| w01 | w01-b10000 | orient_small_about | o200k_base | 60583 | 30763 | -29820 | 0.4922 | 3 | 2 | 1 | 1 | FAIL | pass | yes | quality_fix_not_equivalent_compression |
| w02 | w02-b4096 | resume_explicit_label | o200k_base | 63555 | 27342 | -36213 | 0.5698 | 9 | 2 | 4 | 1 | FAIL | pass | yes | quality_fix_not_equivalent_compression |
| w02 | w02-b10000 | resume_explicit_label | o200k_base | 54535 | 26978 | -27557 | 0.5053 | 1 | 1 | 1 | 1 | FAIL | pass | yes | quality_fix_not_equivalent_compression |
| w03 | w03-b4096 | orient_identical_bodies | o200k_base | 63282 | 27325 | -35957 | 0.5682 | 9 | 2 | 4 | 1 | FAIL | pass | yes | quality_fix_not_equivalent_compression |
| w03 | w03-b10000 | orient_identical_bodies | o200k_base | 54614 | 26963 | -27651 | 0.5063 | 1 | 1 | 1 | 1 | FAIL | pass | yes | quality_fix_not_equivalent_compression |
| w04 | w04-b4096 | dated_wake | o200k_base | 59794 | 26861 | -32933 | 0.5508 | 6 | 2 | 4 | 1 | FAIL | pass | yes | quality_fix_not_equivalent_compression |
| w04 | w04-b10000 | dated_wake | o200k_base | 54168 | 26515 | -27653 | 0.5105 | 1 | 1 | 1 | 1 | FAIL | pass | yes | quality_fix_not_equivalent_compression |
| a01 | a01-b4096 | ask_short_source | o200k_base | 54657 | 26388 | -28269 | 0.5172 | 2 | 1 | 1 | 1 | pass | pass | yes | reference_reduction_with_quality_pass |
| a01 | a01-b10000 | ask_short_source | o200k_base | 53370 | 26388 | -26982 | 0.5056 | 1 | 1 | 1 | 1 | pass | pass | yes | reference_reduction_with_quality_pass |
| e01 | e01 | simple_write_receipt | o200k_base | 52711 | 25850 | -26861 | 0.5096 | 1 | 1 | 1 | 1 | pass | pass | yes | reference_reduction_with_quality_pass |

### Stage split and time to first unit (`json_compact_lexical_v1`)

| journey | startup (base) | startup (cand) | memory (base) | memory (cand) | memory delta | until first unit (base) | until first unit (cand) | until task ready (base) | until task ready (cand) |
|---|---|---|---|---|---|---|---|---|---|
| w01-b4096 | 52075 | 25214 | 8632 | 6759 | -1873 | 57676 | 30826 | — | 30826 |
| w01-b10000 | 52075 | 25214 | 8508 | 5549 | -2959 | 55035 | 28296 | — | 28296 |
| w02-b4096 | 52075 | 25214 | 11480 | 2128 | -9352 | 57262 | 26420 | — | 26420 |
| w02-b10000 | 52075 | 25214 | 2460 | 1764 | -696 | 54535 | 26978 | — | 26978 |
| w03-b4096 | 52075 | 25214 | 11207 | 2111 | -9096 | 57078 | 26371 | — | 26371 |
| w03-b10000 | 52075 | 25214 | 2539 | 1749 | -790 | 54614 | 26963 | — | 26963 |
| w04-b4096 | 52075 | 25214 | 7719 | 1647 | -6072 | 57393 | 26410 | — | 26410 |
| w04-b10000 | 52075 | 25214 | 2093 | 1301 | -792 | 54168 | 26515 | — | 26515 |
| a01-b4096 | 52075 | 25214 | 2582 | 1174 | -1408 | 53341 | 26388 | 53341 | 26388 |
| a01-b10000 | 52075 | 25214 | 1295 | 1174 | -121 | 53370 | 26388 | 53370 | 26388 |
| e01 | 52075 | 25214 | 636 | 636 | 0 | 52711 | 25850 | 52711 | 25850 |

## Cohorts

weighted_reduction = 1 - sum(candidate) / sum(baseline) over rows with both counts; failing rows stay in the cohort.

| encoding | representation | group | n | excluded | baseline sum | candidate sum | weighted_reduction | median delta | worst journey | worst delta | conclusions |
|---|---|---|---|---|---|---|---|---|---|---|---|
| o200k_base | content_blocks_standalone_v1 | all | 11 | 0 | 1790 | 544 | 0.6961 | -42.0000 | w02-b10000 | 1 | descriptive_only: 2, quality_fix_not_equivalent_compression: 8, reference_reduction_with_quality_pass: 1 |
| o200k_base | content_blocks_standalone_v1 | b10000 | 5 | 0 | 319 | 174 | 0.4545 | 1.0000 | w02-b10000 | 1 | descriptive_only: 1, quality_fix_not_equivalent_compression: 4 |
| o200k_base | content_blocks_standalone_v1 | b4096 | 5 | 0 | 1450 | 349 | 0.7593 | -214.0000 | a01-b4096 | -42 | quality_fix_not_equivalent_compression: 4, reference_reduction_with_quality_pass: 1 |
| o200k_base | content_blocks_standalone_v1 | write | 1 | 0 | 21 | 21 | 0.0000 | 0.0000 | e01 | 0 | descriptive_only: 1 |
| o200k_base | json_compact_lexical_v1 | all | 11 | 0 | 631976 | 303346 | 0.5200 | -28269.0000 | e01 | -26861 | quality_fix_not_equivalent_compression: 8, reference_reduction_with_quality_pass: 3 |
| o200k_base | json_compact_lexical_v1 | b10000 | 5 | 0 | 277270 | 137607 | 0.5037 | -27651.0000 | a01-b10000 | -26982 | quality_fix_not_equivalent_compression: 4, reference_reduction_with_quality_pass: 1 |
| o200k_base | json_compact_lexical_v1 | b4096 | 5 | 0 | 301995 | 139889 | 0.5368 | -32933.0000 | a01-b4096 | -28269 | quality_fix_not_equivalent_compression: 4, reference_reduction_with_quality_pass: 1 |
| o200k_base | json_compact_lexical_v1 | write | 1 | 0 | 52711 | 25850 | 0.5096 | -26861.0000 | e01 | -26861 | reference_reduction_with_quality_pass: 1 |
| o200k_base | mcp_json_compact_legacy_v1 | all | 11 | 0 | 631976 | 303346 | 0.5200 | -28269.0000 | e01 | -26861 | quality_fix_not_equivalent_compression: 8, reference_reduction_with_quality_pass: 3 |
| o200k_base | mcp_json_compact_legacy_v1 | b10000 | 5 | 0 | 277270 | 137607 | 0.5037 | -27651.0000 | a01-b10000 | -26982 | quality_fix_not_equivalent_compression: 4, reference_reduction_with_quality_pass: 1 |
| o200k_base | mcp_json_compact_legacy_v1 | b4096 | 5 | 0 | 301995 | 139889 | 0.5368 | -32933.0000 | a01-b4096 | -28269 | quality_fix_not_equivalent_compression: 4, reference_reduction_with_quality_pass: 1 |
| o200k_base | mcp_json_compact_legacy_v1 | write | 1 | 0 | 52711 | 25850 | 0.5096 | -26861.0000 | e01 | -26861 | reference_reduction_with_quality_pass: 1 |
| o200k_base | structured_compact_v1 | all | 11 | 0 | 52342 | 23864 | 0.5441 | -1254.0000 | e01 | 0 | descriptive_only: 1, quality_fix_not_equivalent_compression: 8, reference_reduction_with_quality_pass: 2 |
| o200k_base | structured_compact_v1 | b10000 | 5 | 0 | 15851 | 10828 | 0.3169 | -791.0000 | a01-b10000 | -121 | quality_fix_not_equivalent_compression: 4, reference_reduction_with_quality_pass: 1 |
| o200k_base | structured_compact_v1 | b4096 | 5 | 0 | 36037 | 12582 | 0.6509 | -5116.0000 | a01-b4096 | -1254 | quality_fix_not_equivalent_compression: 4, reference_reduction_with_quality_pass: 1 |
| o200k_base | structured_compact_v1 | write | 1 | 0 | 454 | 454 | 0.0000 | 0.0000 | e01 | 0 | descriptive_only: 1 |

## A/A controls

The same verified artifacts compared with themselves must give zero deltas.

| control | label | baseline run | candidate run | rows | non-zero deltas | rows without counts |
|---|---|---|---|---|---|---|
| aa-baseline.json | aa-baseline | baseline | baseline | 44 | 0 | 0 |
| aa-candidate.json | aa-candidate | candidate | candidate | 44 | 0 | 0 |

## Per-increment history

Earlier paired runs, each against the integration head before the increment (details in
`agent-token-optimization.md`). Figures are whole-journey weighted changes, `o200k_base`,
`json_compact_lexical_v1`; A/A controls were zero in every run.

| Increment | PR | Baseline | 4096 B | 10000 B | Oracle |
|---|---|---|---:|---:|---|
| I1–I2b + I3 (Wake evidence, state, scope) | #835 #837 #840 #839 | main a22b6402 | −6.9 % | −1.0 % | candidate 11/11, baseline 3/11 |
| C1 catalogue without output schemas | #841 | — (fixture) | default `tools/list` 51,629 → 29,460 tokens | | unchanged |
| PR 3 incremental continuation pages | #842 | d339c02f | −3.4 % | −1.2 % | 11/11 both |
| C2 one time-navigation verb | #843 | — (fixture) | default `tools/list` 29,506 → 24,899 tokens | | unchanged |
| C3 minimal continuation actions, lean progress | #845 | dcb8b5d8 | −1.3 % | −0.85 % | 11/11 both |

The I3 run of 2026-09-24 remains in `artifacts/544-wake-oracle-20260924/`.
