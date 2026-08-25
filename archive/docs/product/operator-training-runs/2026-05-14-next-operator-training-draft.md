# Operator Training Run: `2026-05-14-next-operator-training`

Status: `running`

Date opened: `2026-05-14`
Date closed: `pending`
Owner: `Tirso / Codex`

This run was opened as the next serious Operator training attempt. The first
preflight paused on a real measurement gap: the mixed dataset had 100% read
target coverage overall, but eval covered only 75% of the read profile. That
gap is now closed by a capability-aware split plus expanded conformance data.
Training started with Qwen 0.5B as the current strongest baseline. Live MCP
replay remains gated until predictions exist.

## 1. Scope

| Field | Value |
| --- | --- |
| Attempt id | `2026-05-14-next-operator-training` |
| Profile | `operator-read` first; writer semantic training excluded |
| Base model | `Qwen/Qwen2.5-0.5B-Instruct` |
| Adapter output | `/tmp/kernel-operator-qwen05-lora-read-v7-4gpu-20260514` |
| Artifact root | `../rehydration-kernel-artifacts/operator/2026-05-14-next-operator-training/` |
| Branch | `main` |
| Commit | `c5aca703286c00f113d1e6291bf713a7df926d6f` |
| Dirty worktree at start | yes; Operator/conformance code, docs, and scripts have local changes |

## 2. North Star Check

```text
Operator 0.5B:
  only learns to use KMP.

Strong teacher:
  produces semantics when semantics are needed.

Kernel:
  validates, stores, traverses, proves, and audits memory.
```

This run must not train the 0.5B model to author rich writer relations.

## 3. Hypothesis

Main hypothesis:

```text
A small Operator can learn broader KMP read/navigation policy when trained on a
clean, audited, page-aware, grouped, anonymized dataset and evaluated through
strict policy eval plus live MCP replay.
```

Success means:

- zero invalid predictions;
- zero unbounded tool calls;
- zero MCP replay failures;
- zero missing expected refs;
- better or equal exact/tool/ref/scope metrics than the current V6 holdout20
  baseline on comparable scope.

Failure means:

- dataset audit fails;
- model emits non-strict JSON repeatedly;
- policy eval accepts less than the baseline on core read/navigation;
- live replay exposes a contract gap not caught offline.

## 4. Candidate Dataset Inputs

| Dataset | Source | Label source | Teacher model | Rows | Train | Eval | Status |
| --- | --- | --- | --- | ---: | ---: | ---: | --- |
| KMP conformance synthetic v5 full | `../rehydration-kernel-artifacts/operator/2026-05-14-next-operator-training/kernel-operator-conformance-full-v5/trajectories.jsonl` | deterministic contract | none | 58 | 44 | 14 | preserved; superseded for read split coverage |
| KMP conformance synthetic v5 read-only | `../rehydration-kernel-artifacts/operator/2026-05-14-next-operator-training/kernel-operator-conformance-full-v5-read-sft/` | deterministic contract | none | 42 | 32 | 10 | generated; read coverage seed |
| KMP conformance synthetic v7 full | `../rehydration-kernel-artifacts/operator/2026-05-14-next-operator-training/kernel-operator-conformance-full-v7/trajectories.jsonl` | deterministic contract | none | 61 | n/a | n/a | generated; rare read capabilities duplicated for train/eval |
| MemoryArena V6 holdout20 | existing audited SFT | benchmark-derived trajectory | run-dependent | 5,724 source rows before holdout split | 4,600 | 1,124 | baseline |
| MemoryArena P1.11 221-task corpus | `../rehydration-kernel-artifacts/operator/p111-pageinfo-221-20260512/` | benchmark-derived trajectory | run-dependent | 12,465 | 11,177 | 1,288 | candidate scale corpus |
| P1.11 + conformance read mixed candidate v5 | `../rehydration-kernel-artifacts/operator/2026-05-14-next-operator-training/kernel-operator-sft-p111-plus-conformance-read-20260514/` | benchmark-derived + deterministic contract | none | 12,507 | 11,227 | 1,280 | superseded: eval coverage too low |
| P1.11 + conformance read capability split v7 | `../rehydration-kernel-artifacts/operator/2026-05-14-next-operator-training/kernel-operator-sft-capability-split-read-v7-20260514/` | benchmark-derived + deterministic contract | none | 12,510 | 11,238 | 1,272 | ready for training preflight |

Decision before run:

```text
Dataset gate is now clean.

P1.11 alone is too narrow for a full operator-read claim:
target capability coverage on eval_trajectories = 41.67%.

P1.11 + conformance read fixes full-dataset target coverage:
target capability coverage on all_trajectories = 100%.

The first mixed eval split was not sufficient:
target capability coverage on eval_trajectories = 75.00%.

The new capability-aware split with conformance v7 guarantees read profile
coverage in both train and eval:
target capability coverage on train_trajectories = 100%.
target capability coverage on eval_trajectories = 100%.
```

## 5. Required Preflight

- [x] update branch/commit/dirty status;
- [x] confirm dataset root exists outside repo;
- [x] preserve `/tmp` conformance artifacts under attempt artifact root;
- [x] compute hashes for candidate train/eval files;
- [x] run no-gold/no-leak audit;
- [x] run contract coverage;
- [x] confirm grouped split for P1.11 and mixed candidate;
- [x] confirm no unsupported writer semantic labels enter this run;
- [x] confirm live MCP replay requires predictions and therefore runs after
  training/prediction;
- [x] confirm live MCP endpoint is available for replay smoke.

Hashes:

| File | SHA-256 |
| --- | --- |
| conformance full `trajectories.jsonl` | `056c9d424f6ebfdd1d51fc66ab166c27c9c178a89375bf0b762fff3c941f042b` |
| conformance full `openai_train.jsonl` | `523cca2fc5c018152b2bdef336652e878408e99b70b0671817b58c0c04bd5834` |
| conformance full `openai_eval.jsonl` | `4bf7defef12b1f4f1bb544d2040279bf7ce1b098e90a1605aa2f8f120c4cbc8c` |
| conformance read `openai_train.jsonl` | `e5db2dab978e546bae5ff4dbbb477868068f3dce0e75b8265aa609ff7ebe59f1` |
| conformance read `openai_eval.jsonl` | `42df73a1e34222bb3ca86d95ed442d6fe3e1fbe2749577016a90763f01f0deae` |
| P1.11 `openai_train.jsonl` | `0be135b2edd5f7fcbafe7866955cecc59c637f13eb5db8579325a5820a6d9741` |
| P1.11 `openai_eval.jsonl` | `216b3dcce5c629fde46993a17bcd0d9620a75151722134b265eb2fdc1fd1dffd` |
| mixed candidate `openai_train.jsonl` | `13d655984dbdfcf00cd3d793eb5b3423f0ecbfb0a71b6dfddc34395afc40be22` |
| mixed candidate `openai_eval.jsonl` | `a457745636ecd489b0155f4188b2882ae7e796c20d04c055487ef67332758d62` |
| conformance v7 `trajectories.jsonl` | `241d549708af8e5ffa245207f1a4f240bfa0d1ed6407980b8f7ea001314de01c` |
| mixed capability v7 `openai_train.jsonl` | `8cc5bbe0bdffe5c5bef5d256bd7e8abac298b1ee998f9f031a73c689e0984343` |
| mixed capability v7 `openai_eval.jsonl` | `e70b53970941674a947db8ea4d15473ecaedaf74aca3a7e3e95061cc684a0a6a` |
| mixed capability v7 `all_trajectories.jsonl` | `c9b024b5d2b447d05f5acca2e6ac0f6422abd7a50abffeb89774e0860ce194f0` |

## 6. Stop Gates

Stop immediately if:

- any model-facing row leaks target action, benchmark answer, hidden writer
  labels, raw prompts, credentials, or post-hoc outputs;
- dropped non-visible target refs is non-zero without explicit acceptance;
- any action fails the strict KMP/MCP action contract;
- the wrong base model or wrong dataset is selected;
- predictions have invalid actions or unbounded calls;
- live replay has MCP failures or missing expected refs.

## 7. Execution Plan

1. Finalize dataset choice.
2. Prepare SFT with grouped split and anonymized refs.
3. Run no-gold audit.
4. Run contract coverage.
5. Train LoRA from scratch.
6. Predict eval set with strict parser.
7. Run strict policy eval with details output.
8. Compare against current V6 holdout20 baseline.
9. De-anonymize predictions.
10. Replay first 100 rows through live MCP.
11. Replay full eval only if smoke is clean.
12. Close this document with final status.

## 8. Results

Preflight results:

| Check | Result |
| --- | ---: |
| P1.11 no-gold findings | 0 |
| conformance v5 no-gold findings | 0 |
| conformance read-only no-gold findings | 0 |
| mixed candidate no-gold findings | 0 |
| conformance full target capability coverage | 100% full profile |
| conformance read-only target capability coverage | 100% read profile |
| P1.11 eval target capability coverage | 41.67% read profile |
| mixed candidate full-dataset target capability coverage | 100% read profile |
| mixed candidate eval target capability coverage | 75% read profile |
| mixed candidate dropped non-visible target refs | 0 |
| mixed candidate selected rows | 12,507 |
| mixed candidate train rows | 11,227 |
| mixed candidate eval rows | 1,280 |
| conformance v7 contract validation failures | 0 |
| mixed capability v7 no-gold findings | 0 |
| mixed capability v7 dropped non-visible target refs | 0 |
| mixed capability v7 selected rows | 12,510 |
| mixed capability v7 train rows | 11,238 |
| mixed capability v7 eval rows | 1,272 |
| mixed capability v7 train target capability coverage | 100% read profile |
| mixed capability v7 eval target capability coverage | 100% read profile |
| mixed capability v7 all target capability coverage | 100% read profile |

Script changes made during preflight:

- `prepare_operator_sft_dataset.py` now supports `--include-mode` and
  `--exclude-mode`;
- `prepare_operator_sft_dataset.py` now supports `--group-key task_or_step` so
  benchmark task groups stay grouped while synthetic conformance rows can split
  by `step_id`.
- `prepare_operator_sft_dataset.py` now supports `--capability-split-profile`,
  `--require-eval-capability-coverage`, and
  `--require-train-capability-coverage`;
- capability-aware splitting preserves train coverage while seeding eval with
  required profile capabilities;
- Python capability extraction was aligned with the Rust contract: trace
  pagination capabilities only count when `kernel_trace.arguments.page` exists.

Capability-aware dataset command:

```bash
python scripts/operator/prepare_operator_sft_dataset.py \
  --trajectories ../rehydration-kernel-artifacts/operator/p111-pageinfo-221-20260512/kernel-operator-trajectories-p111-pageinfo-221-20260512/trajectories.jsonl \
  --trajectories /tmp/kernel-operator-conformance-full-v7/trajectories.jsonl \
  --output /tmp/kernel-operator-sft-capability-split-read-v7-20260514 \
  --include-mode read \
  --include-mode write_context_read \
  --split-mode group \
  --group-key task_or_step \
  --eval-ratio 0.1 \
  --seed 42 \
  --anonymize-refs \
  --require-visible-target-refs \
  --capability-split-profile read \
  --require-eval-capability-coverage \
  --require-train-capability-coverage \
  --force
```

## 9. Current Decision

The run is complete and classified as `internal-only`.

Reason:

```text
The model learned the MemoryArena P1.11 trajectory surface extremely well, but
it did not learn the rare synthetic conformance API/MCP rows well enough. The
strict predictor rejected 3/1,272 outputs and the policy evaluator found
13/1,272 non-exact rows. All non-exact rows are in conformance synthetic v7.

This is useful evidence, but it is not a public Operator candidate because the
candidate gate requires zero missing predictions, zero strict contract
failures, and zero non-executable actions before live MCP replay.
```

Next required step:

```text
Expand the conformance training set with multiple variants per rare KMP/MCP
capability, or add schema-aware/constrained decoding for tool arguments, then
rerun from a fresh attempt. Do not run live MCP replay for this attempt because
the offline gate did not pass.
```

## 10. Training Launch

Training was launched after the dataset gate passed.

| Field | Value |
| --- | --- |
| Kubernetes job | `underpass-runtime/kop-qwen05-lora-read-v7-4gpu-20260514` |
| Manifest | `archive/k8s/kernel-operator-qwen05-lora-read-v7-4gpu-20260514-job.yaml` |
| Base model | `Qwen/Qwen2.5-0.5B-Instruct` |
| Train file | `/tmp/kernel-operator-sft-capability-split-read-v7-20260514/openai_train.jsonl` |
| Eval file | `/tmp/kernel-operator-sft-capability-split-read-v7-20260514/openai_eval.jsonl` |
| Output adapter | `/tmp/kernel-operator-qwen05-lora-read-v7-4gpu-20260514` |
| Epochs | `3` |
| Hardware | `4x RTX 3090 via Kubernetes nvidia.com/gpu=4` |
| Batch config | per-device batch `4`, grad accumulation `1` |
| Max length | `2048` |
| Precision | `fp16` |

Initial observed log state:

```text
Dataset loaded: train=11,238 eval=1,272.
Model weights loaded.
Tokenization started.
```

## 11. Live Training Journal

| Time | Event | Evidence | Decision |
| --- | --- | --- | --- |
| `19:32` | Training active on all 4 GPUs | `kop-qwen05-lora-read-v7-4gpu-20260514-5ptwv`, `nvidia-smi`: 4x RTX 3090 at 100%, about 19 GiB VRAM each | continue |
| `19:32` | Early loss fell quickly | step ~110/2109, loss around `0.0065`, mean token accuracy around `0.9977` | continue, but do not treat loss as quality until strict policy eval and MCP replay pass |
| `19:36` | Public replay endpoint reachable | `https://rehydration-kernel.underpassai.com` returned HTTP/2 `200`, `content-type: application/grpc`, `grpc-status: 12` on HEAD; kernel pod `Ready` | continue |
| `20:03` | Epoch 1 completed | `eval_loss=0.005514`, `eval_mean_token_accuracy=0.9979`, eval runtime `79.66s`, checkpoint `checkpoint-703` written | continue; strict prediction/eval remains the quality gate |
| `20:44` | Epoch 2 completed | `eval_loss=0.005230`, `eval_mean_token_accuracy=0.9979`, eval runtime `80.07s`, checkpoint `checkpoint-1406` written | continue; eval loss improved but strict prediction/eval remains the quality gate |
| `21:16` | Training completed | job duration `109m`, train runtime `6522s`, `train_loss=0.02849`, final `eval_loss=0.0052`, final `eval_mean_token_accuracy=0.998`, final adapter files written | continue to strict prediction |
| `21:22` | Strict prediction completed | `1269/1272` predictions accepted, `3` strict failures | stop public candidate path; run offline policy eval |
| `21:28` | Offline policy eval completed | `1259/1272` exact, `3` missing, `0` invalid accepted, `0` unbounded | classify as internal-only; no live MCP replay |

## 12. Capability And Data Contribution

| Data block | Intended capability | Added rows | Coverage delta | Strict metric delta | Classification |
| --- | --- | ---: | --- | --- | --- |
| MemoryArena P1.11 221-task corpus | scale read/navigation and writer context-read operation | `12,465` source trajectories | eval read coverage by itself: `41.67%` | `1259/1259` exact on MemoryArena eval rows | `improves` |
| KMP conformance synthetic v7 | missing read API/MCP capabilities, rare scopes, pagination, cursor modes, stop and fail-fast decisions | `61` source trajectories | mixed eval read coverage: `75% -> 100%`; train read coverage: `100%` | `0/13` exact on conformance eval rows; 3 strict failures | `unproven` |

Interpretation:

```text
The capability-aware split fixed the measurement gap before training. It does
not yet prove the model learned the extra API/MCP surface. The strict policy
evaluator shows that the MemoryArena surface is solved in this split, while
the rare conformance rows are still under-taught.

## 13. Prediction And Policy Eval

Prediction artifacts:

| Artifact | Path |
| --- | --- |
| predictions directory | `../rehydration-kernel-artifacts/operator/2026-05-14-next-operator-training/results/kernel-operator-qwen05-predictions-read-v7-strict-20260514/` |
| policy summary | `../rehydration-kernel-artifacts/operator/2026-05-14-next-operator-training/results/kernel-operator-qwen05-predictions-read-v7-strict-20260514-policy-eval.json` |
| policy details | `../rehydration-kernel-artifacts/operator/2026-05-14-next-operator-training/results/kernel-operator-qwen05-predictions-read-v7-strict-20260514-policy-details.jsonl` |
| adapter | `../rehydration-kernel-artifacts/operator/2026-05-14-next-operator-training/results/kernel-operator-qwen05-lora-read-v7-4gpu-20260514/` |

Strict prediction summary:

| Metric | Value |
| --- | ---: |
| selected eval rows | `1,272` |
| accepted predictions | `1,269` |
| strict failures | `3` |
| invalid predictions accepted by policy eval | `0` |
| unbounded tool calls | `0` |

Strict failure reasons:

| Reason | Count |
| --- | ---: |
| `action.arguments_missing_required:ref` | `1` |
| `action.arguments_missing_required:to` | `1` |
| `action.arguments_unexpected:from,include,limit,window` | `1` |

Offline policy summary:

| Metric | Value |
| --- | ---: |
| total eval rows | `1,272` |
| exact actions | `1,259` |
| exact action accuracy | `0.9897798742` |
| missing predictions | `3` |
| invalid predictions | `0` |
| unbounded tool calls | `0` |
| tool accuracy | `0.9963833635` |
| primary ref accuracy | `0.9927667269` |
| scope accuracy | `0.9972875226` |
| stop accuracy | `1.0` |

Result by task family:

| Task family | Rows | Exact | Missing |
| --- | ---: | ---: | ---: |
| `memoryarena.progressive_search` | `641` | `641` | `0` |
| `memoryarena.smart_writer` | `618` | `618` | `0` |
| `conformance.read.ask` | `3` | `0` | `0` |
| `conformance.read.inspect` | `1` | `0` | `1` |
| `conformance.read.near` | `2` | `0` | `0` |
| `conformance.read.temporal` | `4` | `0` | `0` |
| `conformance.read.trace` | `2` | `0` | `1` |
| `conformance.read.wake` | `1` | `0` | `1` |

## 14. Failure Analysis

The 3 strict failures are schema-adherence failures, not raw JSON failures.
The predictor emitted parseable JSON, and the strict KMP/MCP action contract
correctly rejected actions that were close but not valid.

| Step | Expected | Model behavior | Rejection |
| --- | --- | --- | --- |
| `kmp-operator-conformance-v7:wake-current-about` | `kernel_wake` with wake arguments | Chose `kernel_wake`, but mixed in temporal/navigation fields: `from`, `include`, `limit`, `window` | `action.arguments_unexpected:from,include,limit,window` |
| `kmp-operator-conformance-v7:trace-first-page-after-new-target` | `kernel_trace` first page with `from` and `to` | Chose `kernel_trace`, but omitted required `to` | `action.arguments_missing_required:to` |
| `kmp-operator-conformance-v7:inspect-typed-raw-false` | `kernel_inspect` with `ref` and typed include flags | Chose `kernel_inspect`, but omitted required `ref` and mixed in navigation fields | `action.arguments_missing_required:ref` |

The remaining 10 non-exact rows were valid tool calls but did not match the
target action exactly. They are all synthetic conformance rows. The main gap is
therefore not MemoryArena operation; it is rare API/MCP capability coverage.

Interpretation:

```text
Qwen did not fail by producing malformed free text. It failed by mixing tool
argument schemas on rare tools/capabilities. This run validates the strict
action contract and shows that the Operator dataset needs more examples per
rare capability, or schema-aware decoding so a selected tool cannot emit fields
from another tool.
```

## 15. Final Decision

Final status:

```text
internal-only
```

Reason:

```text
Strong internal result for MemoryArena P1.11 operation: 1259/1259 exact on
MemoryArena eval rows, with zero invalid and zero unbounded calls.

Not a public Operator candidate: conformance v7 rare API/MCP rows are not
learned, with 13/13 non-exact rows and 3 strict prediction failures.
```

Follow-up:

- expand conformance rows with multiple variants per read capability;
- add specific hard cases for `kernel_wake`, `kernel_inspect`,
  `kernel_trace` first/continue page, temporal direction, and multi-about
  dimension scopes;
- consider two-stage prediction: choose tool first, then emit arguments under
  the selected tool schema;
- consider constrained/schema-aware decoding before claiming 100% API/MCP
  coverage;
- rerun from a fresh attempt document and do not replay against live MCP until
  offline strict prediction has zero missing, zero invalid, and zero unbounded
  calls.
