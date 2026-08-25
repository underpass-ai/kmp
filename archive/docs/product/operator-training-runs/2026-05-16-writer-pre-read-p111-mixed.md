# Operator Training Run: `writer-pre-read-p111-mixed`

## 1. Status

| Field | Value |
| --- | --- |
| Attempt id | `writer-pre-read-p111-mixed` |
| Date | 2026-05-16 |
| Status | `dataset-ready` |
| Scope | MemoryArena P1.11 smart-writer pre-read rows plus `writer-pre-read-v1` conformance |
| Artifact root | `../rehydration-kernel-artifacts/operator/2026-05-16-writer-pre-read-p111-mixed/` |

No model training was launched in this slice.

Historical note: this run was produced before `writer-pre-read-v2` expanded the
profile from 16 to 21 required capabilities. Keep this document as evidence for
the original P1.11 + v1 mixed gate; use the v2 diversity gate for new training
claims.

## 2. Why This Run Exists

The previous `writer-pre-read-gate` proved that the synthetic
`writer-pre-read-v1` suite can cover the writer pre-read contract. It did not
prove that real MemoryArena smart-writer traces cover the same decision space.

This run checks that gap directly:

1. export real writer pre-read trajectories from the MemoryArena P1.11 smart
   writer run;
2. measure them against the `writer-pre-read` contract profile;
3. mix the real rows with `writer-pre-read-v1` conformance rows;
4. prepare a compact SFT cut with train/eval capability coverage at 100%;
5. audit the final OpenAI-format train/eval files for gold leakage.

## 3. Inputs

| Input | Purpose |
| --- | --- |
| `/tmp/memoryarena-p111-smart-221-20260512-000934-run/writer_results.jsonl` | Real MemoryArena P1.11 smart-writer pre-read source |
| `/tmp/kernel-operator-conformance-writer-pre-read-v1-20260516/trajectories.jsonl` | Synthetic writer pre-read conformance source |

The P1.11 run was already available from previous benchmark work. This slice
does not rerun MemoryArena or call the deployed kernel.

## 4. Real P1.11 Writer Export

Command:

```bash
cargo run -p rehydration-testkit --bin kernel_operator_trajectory_export -- \
  --run /tmp/memoryarena-p111-smart-221-20260512-000934-run \
  --output /tmp/kernel-operator-trajectories-p111-smart-writer-pre-read-20260516 \
  --include-writer-reads \
  --force
```

Result:

| Metric | Value |
| --- | ---: |
| trajectories | 12,465 |
| read trajectories | 6,343 |
| `write_context_read` trajectories | 6,122 |
| writer failures | 0 |
| bounded failures | 0 |
| redaction findings | 0 |

Real target actions:

| Target action | Count |
| --- | ---: |
| `kernel_near` | 4,702 |
| `kernel_inspect` | 4,702 |
| `kernel_trace` | 1,420 |
| `stop` | 1,641 |

## 5. Real Corpus Coverage Gap

Command:

```bash
cargo run -p rehydration-testkit --bin kernel_operator_contract_coverage -- \
  --profile writer-pre-read \
  --trajectories /tmp/kernel-operator-trajectories-p111-smart-writer-pre-read-20260516/trajectories.jsonl \
  --fail-under 100 \
  --output /tmp/kernel-operator-trajectories-p111-smart-writer-pre-read-20260516/contract_coverage_writer_pre_read.json
```

The command fails fast, as intended:

| Coverage | Value |
| --- | ---: |
| profile contract coverage | 16 / 16, 100.00% |
| target capability coverage | 13 / 16, 81.25% |

Missing capabilities from the real P1.11 writer pre-read corpus:

| Missing capability | Meaning |
| --- | --- |
| `window:expand` | No target action required increasing the read window |
| `trace.page:first` | Trace calls were not represented as first-page targets under this profile |
| `writer.last_tool:kernel_inspect` | No next decision depended on a previous inspect state |

Interpretation: the real benchmark trace is valuable but not sufficient by
itself to train or claim full writer pre-read coverage. The synthetic
conformance suite is still required to cover rare but important API behavior.

## 6. Mixed Compact SFT Cut

Full mixed preparation without duplicate capping produced 6,130 selected rows
but only 18 unique model-facing prompts. That is too repetitive for a serious
training cut.

The compact cut caps exact duplicate model rows at 20 while preserving 100%
capability coverage:

```bash
python scripts/operator/prepare_operator_sft_dataset.py \
  --trajectories /tmp/kernel-operator-trajectories-p111-smart-writer-pre-read-20260516/trajectories.jsonl \
  --trajectories /tmp/kernel-operator-conformance-writer-pre-read-v1-20260516/trajectories.jsonl \
  --output /tmp/kernel-operator-sft-p111-smart-writer-pre-read-mixed-compact-20260516 \
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
| selected rows | 207 |
| train rows | 165 |
| eval rows | 42 |
| source real rows retained | 199 |
| source conformance rows retained | 8 |
| train capability coverage | 16 / 16, 100.00% |
| eval capability coverage | 16 / 16, 100.00% |
| dropped non-visible target refs | 0 |
| dropped non-visible target cursors | 0 |
| duplicate rows dropped | 5,923 |
| unique model-facing rows | 18 |
| max duplicate count after cap | 20 |

Action distribution:

| Target action | Count |
| --- | ---: |
| `kernel_inspect` | 121 |
| `kernel_near` | 84 |
| `kernel_trace` | 2 |

## 7. Quality Finding

The mixed compact dataset is clean enough for a gate and a small diagnostic
adapter, but not yet a strong training base.

The blocker is diversity:

- 6,130 candidate rows collapse to 18 unique model-facing prompts after
  anonymization;
- capping duplicates reduces the set to 207 rows, but still leaves only 18
  unique prompts;
- forcing `--drop-eval-model-row-overlap` fails because eval then loses required
  writer-pre-read capabilities.

This means the current real writer pre-read traces mostly repeat the same few
decisions. Before a serious writer-pre-read model claim, the dataset needs more
real or synthetic variation in:

- expanded windows;
- trace-first-page decisions;
- decisions made after inspect;
- candidate ambiguity;
- stop/sufficient-context decisions for writer pre-read;
- richer visible candidate details.

## 8. Audits

No-gold audit:

```bash
python scripts/operator/audit_operator_sft_no_gold.py \
  /tmp/kernel-operator-sft-p111-smart-writer-pre-read-mixed-compact-20260516/openai_train.jsonl \
  /tmp/kernel-operator-sft-p111-smart-writer-pre-read-mixed-compact-20260516/openai_eval.jsonl \
  --output /tmp/kernel-operator-sft-p111-smart-writer-pre-read-mixed-compact-20260516/no_gold_audit.json
```

Result:

| Metric | Value |
| --- | ---: |
| rows | 207 |
| findings | 0 |

Final compact coverage:

```bash
cargo run -p rehydration-testkit --bin kernel_operator_contract_coverage -- \
  --profile writer-pre-read \
  --trajectories /tmp/kernel-operator-sft-p111-smart-writer-pre-read-mixed-compact-20260516/all_trajectories.jsonl \
  --fail-under 100 \
  --output /tmp/kernel-operator-sft-p111-smart-writer-pre-read-mixed-compact-20260516/contract_coverage_writer_pre_read.json
```

Result:

| Coverage | Value |
| --- | ---: |
| profile contract coverage | 16 / 16, 100.00% |
| target capability coverage | 16 / 16, 100.00% |

## 9. Artifact Hashes

| Artifact | SHA-256 |
| --- | --- |
| `results/kernel-operator-trajectories-p111-smart-writer-pre-read-20260516/summary.json` | `b2719dc04bc9c656aba8bcb623f513596277d681d7b9aaa543d8bcaf76b38fcb` |
| `results/kernel-operator-trajectories-p111-smart-writer-pre-read-20260516/trajectories.jsonl` | `855b06efb4350ba20001031c0d83372ec5d216fed78a17abcd74c8f449e9e700` |
| `results/kernel-operator-trajectories-p111-smart-writer-pre-read-20260516/contract_coverage_writer_pre_read.json` | `89957b2de71e49924eacc68402a1890ccbf8f99e4c230dba97bd7b9677c6ed88` |
| `results/kernel-operator-sft-p111-smart-writer-pre-read-mixed-compact-20260516/summary.json` | `02bb329d5903a2d961b1102c81bf2bc3ad5449c1b95e27c77333402f61d9ce47` |
| `results/kernel-operator-sft-p111-smart-writer-pre-read-mixed-compact-20260516/openai_train.jsonl` | `3e82e1d0c5f9aeb230b60806c086667ce39a0ac3adfe12a34067cb2f8c0c0a54` |
| `results/kernel-operator-sft-p111-smart-writer-pre-read-mixed-compact-20260516/openai_eval.jsonl` | `2c707b0ec83aaa4f0127eb4642f07bf3b0d58da136a565596651f7663e90212c` |
| `results/kernel-operator-sft-p111-smart-writer-pre-read-mixed-compact-20260516/contract_coverage_writer_pre_read.json` | `7ab90010cbbf9cb3b29c85e8c698f1a6457e2e17cf3a09c1b3f6abc9f271c87f` |
| `results/kernel-operator-sft-p111-smart-writer-pre-read-mixed-compact-20260516/no_gold_audit.json` | `058578719f78771f5b48cdea61ab7d5387327dd775f1984329de28c27dc4d928` |

## 10. Decision

This attempt is `dataset-ready` for diagnostic writer-pre-read experiments.

It should not be treated as a publication-grade writer-pre-read training set.
The main value of the run is the measured gap: the real benchmark trace is
large but structurally repetitive, and conformance rows are required to cover
rare API/MCP behavior.

## 11. Next Steps

1. Add or collect more diverse writer pre-read trajectories before training a
   serious candidate.
2. Add explicit stop/sufficient-context writer pre-read cases to the profile if
   Operator is expected to decide when the writer has enough context.
3. Train only a diagnostic adapter from this compact cut if useful.
4. Do not promote a writer-pre-read model until eval can drop exact train/eval
   prompt overlap while retaining 100% capability coverage.
