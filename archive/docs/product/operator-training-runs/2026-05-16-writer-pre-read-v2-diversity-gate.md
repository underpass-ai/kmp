# Operator Training Run: `writer-pre-read-v2-diversity-gate`

## 1. Status

| Field | Value |
| --- | --- |
| Attempt id | `writer-pre-read-v2-diversity-gate` |
| Date | 2026-05-16 |
| Status | `dataset-ready` |
| Scope | `writer-pre-read-v2` conformance plus MemoryArena P1.11 smart-writer pre-read rows |
| Artifact root | `../rehydration-kernel-artifacts/operator/2026-05-16-writer-pre-read-v2-diversity-gate/` |

No model training was launched in this slice.

## 2. Why This Run Exists

The previous P1.11 mixed cut exposed the right problem: real smart-writer
pre-read traces were large but structurally repetitive. They also did not cover
all decisions the Operator must make before a writer commits memory.

This run upgrades the writer pre-read gate from 16 to 21 required capabilities.
The added capabilities are:

| Capability | Why it matters |
| --- | --- |
| `tool:stop` | The Operator must stop reading when enough evidence is visible. |
| `window:stop_sufficient` | Stop is a window policy, not a missing tool call. |
| `trace.page:continue` | Writer pre-read must not silently ignore paginated traces. |
| `writer.last_tool:kernel_trace` | A writer often decides after tracing a relation path. |
| `writer.candidate_pool:ambiguous` | The Operator must recognize multiple plausible relation targets. |

The goal is not to make a large dataset. The goal is to prevent a model placed
between an LLM and KMP/MCP from narrowing the real API surface.

## 3. Synthetic `writer-pre-read-v2`

Command:

```bash
cargo run -p rehydration-testkit --bin kernel_operator_conformance_trajectory_export -- \
  --suite writer-pre-read-v2 \
  --run-id kmp-operator-writer-pre-read-v2-20260516 \
  --output /tmp/kernel-operator-conformance-writer-pre-read-v2-20260516 \
  --force
```

Result:

| Metric | Value |
| --- | ---: |
| trajectories | 14 |
| modes | `write_context_read`: 14 |
| contract validation failures | 0 |
| `kernel_near` targets | 4 |
| `kernel_inspect` targets | 4 |
| `kernel_trace` targets | 4 |
| `stop` targets | 2 |

## 4. Contract Coverage

Command:

```bash
cargo run -p rehydration-testkit --bin kernel_operator_contract_coverage -- \
  --profile writer-pre-read \
  --trajectories /tmp/kernel-operator-conformance-writer-pre-read-v2-20260516/trajectories.jsonl \
  --fail-under 100 \
  --output /tmp/kernel-operator-conformance-writer-pre-read-v2-20260516/contract_coverage_writer_pre_read.json
```

Result:

| Coverage | Value |
| --- | ---: |
| profile contract coverage | 21 / 21, 100.00% |
| target capability coverage | 21 / 21, 100.00% |
| target trace first pages | 2 |
| target trace continuation pages | 2 |
| missing capabilities | 0 |

The overall MCP coverage remains 8 / 10 because this profile intentionally
excludes the write tools `kernel_ingest` and `kernel_write_memory`. That is not
a failure for `writer-pre-read`; it is a profile boundary.

## 5. Synthetic SFT Gate

Command:

```bash
python scripts/operator/prepare_operator_sft_dataset.py \
  --trajectories /tmp/kernel-operator-conformance-writer-pre-read-v2-20260516/trajectories.jsonl \
  --output /tmp/kernel-operator-sft-writer-pre-read-v2-20260516 \
  --include-mode write_context_read \
  --eval-ratio 0.5 \
  --split-mode group \
  --group-key task_or_step \
  --seed 42 \
  --anonymize-refs \
  --require-visible-target-refs \
  --require-visible-target-cursors \
  --capability-split-profile writer-pre-read \
  --require-eval-capability-coverage \
  --require-train-capability-coverage \
  --force
```

Result:

| Metric | Value |
| --- | ---: |
| selected rows | 14 |
| train rows | 7 |
| eval rows | 7 |
| unique model-facing rows | 14 |
| duplicate model row hashes | 0 |
| train capability coverage | 21 / 21, 100.00% |
| eval capability coverage | 21 / 21, 100.00% |
| dropped non-visible target refs | 0 |
| dropped non-visible target cursors | 0 |

No-gold audit:

| Metric | Value |
| --- | ---: |
| rows | 14 |
| findings | 0 |

## 6. Mixed P1.11 + V2 Compact Cut

Command:

```bash
python scripts/operator/prepare_operator_sft_dataset.py \
  --trajectories /tmp/kernel-operator-trajectories-p111-smart-writer-pre-read-20260516/trajectories.jsonl \
  --trajectories /tmp/kernel-operator-conformance-writer-pre-read-v2-20260516/trajectories.jsonl \
  --output /tmp/kernel-operator-sft-p111-smart-writer-pre-read-mixed-v2-compact-20260516 \
  --include-mode write_context_read \
  --eval-ratio 0.2 \
  --split-mode group \
  --group-key task_or_step \
  --seed 42 \
  --anonymize-refs \
  --require-visible-target-refs \
  --require-visible-target-cursors \
  --capability-split-profile writer-pre-read \
  --require-eval-capability-coverage \
  --require-train-capability-coverage \
  --max-duplicate-model-row-count 20 \
  --force
```

Result:

| Metric | Value |
| --- | ---: |
| total source rows | 12,479 |
| selected rows | 213 |
| train rows | 168 |
| eval rows | 45 |
| real rows retained | 199 |
| conformance rows retained | 14 |
| unique model-facing rows | 24 |
| duplicate rows dropped | 5,923 |
| max duplicate count after cap | 20 |
| train capability coverage | 21 / 21, 100.00% |
| eval capability coverage | 21 / 21, 100.00% |
| dropped non-visible target refs | 0 |
| dropped non-visible target cursors | 0 |

Action distribution:

| Target action | Count |
| --- | ---: |
| `kernel_near` | 84 |
| `kernel_inspect` | 123 |
| `kernel_trace` | 4 |
| `stop` | 2 |

Quality comparison against the previous compact cut:

| Cut | Required capabilities | Rows | Unique model-facing rows |
| --- | ---: | ---: | ---: |
| P1.11 + `writer-pre-read-v1` | 16 | 207 | 18 |
| P1.11 + `writer-pre-read-v2` | 21 | 213 | 24 |

This is an improvement, not a final training corpus. The real P1.11 rows still
dominate the dataset and still repeat a small number of model-facing states.

## 7. Audits

No-gold audit:

```bash
python scripts/operator/audit_operator_sft_no_gold.py \
  /tmp/kernel-operator-sft-p111-smart-writer-pre-read-mixed-v2-compact-20260516/openai_train.jsonl \
  /tmp/kernel-operator-sft-p111-smart-writer-pre-read-mixed-v2-compact-20260516/openai_eval.jsonl \
  --output /tmp/kernel-operator-sft-p111-smart-writer-pre-read-mixed-v2-compact-20260516/no_gold_audit.json
```

Result:

| Metric | Value |
| --- | ---: |
| rows | 213 |
| findings | 0 |

Final compact coverage:

```bash
cargo run -p rehydration-testkit --bin kernel_operator_contract_coverage -- \
  --profile writer-pre-read \
  --trajectories /tmp/kernel-operator-sft-p111-smart-writer-pre-read-mixed-v2-compact-20260516/all_trajectories.jsonl \
  --fail-under 100 \
  --output /tmp/kernel-operator-sft-p111-smart-writer-pre-read-mixed-v2-compact-20260516/contract_coverage_writer_pre_read.json
```

Result:

| Coverage | Value |
| --- | ---: |
| profile contract coverage | 21 / 21, 100.00% |
| target capability coverage | 21 / 21, 100.00% |
| missing capabilities | 0 |

## 8. Artifact Hashes

| Artifact | SHA-256 |
| --- | --- |
| `results/kernel-operator-conformance-writer-pre-read-v2-20260516/summary.json` | `da0256957197cbf7caa54e748a0c6b1967e0dd50cfbe0c78db76b641dbd14887` |
| `results/kernel-operator-conformance-writer-pre-read-v2-20260516/trajectories.jsonl` | `a06f57d455728f7570dc8d4ff0743ba7e4b32413b599df439c220f5f3da1c1c3` |
| `results/kernel-operator-conformance-writer-pre-read-v2-20260516/contract_coverage_writer_pre_read.json` | `3a6d45977eb03db31f71d887f01623cb3b4726db4902db0c7c1d71a19903963f` |
| `results/kernel-operator-sft-writer-pre-read-v2-20260516/summary.json` | `9fe9f70176354975c4f791c4865ba3bf1619d86c7f29f67c7b2ddaf2c09bf85c` |
| `results/kernel-operator-sft-writer-pre-read-v2-20260516/openai_train.jsonl` | `1714ba0edbd5af92fac5ef71217e137f6c16ebbaaade6d7897fb7474b3c7ae0d` |
| `results/kernel-operator-sft-writer-pre-read-v2-20260516/openai_eval.jsonl` | `25b2f81604851f5c6d4fa11d44a1f46094f5de6ae0bc5ca6c3c0576456c27119` |
| `results/kernel-operator-sft-writer-pre-read-v2-20260516/contract_coverage_writer_pre_read.json` | `b622464b103daafd0a7e55eaf51cb2d4dd2678f81283f35813eed3922a514ef7` |
| `results/kernel-operator-sft-writer-pre-read-v2-20260516/no_gold_audit.json` | `a867f9488075b95343fc3d8928c7cc2bcb03e85e931b20b694715201894a668a` |
| `results/kernel-operator-sft-p111-smart-writer-pre-read-mixed-v2-compact-20260516/summary.json` | `b04bd4caa02e1052e96e668aae766b0ba5c82c9843b4885c6530ac77cef306bb` |
| `results/kernel-operator-sft-p111-smart-writer-pre-read-mixed-v2-compact-20260516/openai_train.jsonl` | `d213aa3cda764d233b7876e90ff9953017d3552c337fcab8650aa9484bbd9fe5` |
| `results/kernel-operator-sft-p111-smart-writer-pre-read-mixed-v2-compact-20260516/openai_eval.jsonl` | `cf053ac94e86e39284f121eb8e6891a0237f43827b6bc578149bb10a69f2631f` |
| `results/kernel-operator-sft-p111-smart-writer-pre-read-mixed-v2-compact-20260516/contract_coverage_writer_pre_read.json` | `dc17df7708bcb750e372e04035f12991d3441ea0752298719aa5314387e7fcd8` |
| `results/kernel-operator-sft-p111-smart-writer-pre-read-mixed-v2-compact-20260516/no_gold_audit.json` | `0fd69491cb8393626e49aa126732eb0595950ea616fe5b6fe665d1595ea2b5da` |

## 9. Decision

This attempt is `dataset-ready` for the next diagnostic writer pre-read model
experiment.

It is stronger than the previous mixed cut because the contract now includes
stop, trace continuation, post-trace state, and candidate ambiguity. It is still
not enough for a publication-grade Operator claim because the real rows remain
too repetitive after anonymization.

## 10. Next Steps

1. Use this v2 mixed cut for the next controlled diagnostic training run only.
2. Generate or collect more real writer pre-read states with distinct visible
   context, not only more rows.
3. Keep `writer-pre-read-v2` as the minimum conformance source for any future
   writer pre-read training claim.
4. Do not place a writer pre-read model in front of KMP/MCP unless the exact
   profile it serves reaches 100% train/eval capability coverage.
