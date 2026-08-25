# Operator Training Run: `writer-pre-read-gate`

Status: `dataset-ready`

Date opened: 2026-05-16
Owner: Tirso / Codex

## 1. Scope

| Field | Value |
| --- | --- |
| Attempt id | `writer-pre-read-gate` |
| Profile | `operator-write-context-read` / smart-writer pre-read |
| Base model | not trained in this slice |
| Artifact root | `../rehydration-kernel-artifacts/operator/2026-05-16-writer-pre-read-gate/` |
| Branch | `codex/operator-write-context-read-gate` |
| Starting point | `main` after PR `#106` |

Historical note: this run was produced before `writer-pre-read-v2` expanded the
profile from 16 to 21 required capabilities. Keep this document as evidence for
the original v1 gate; use the v2 gate for new training claims.

## 2. Why This Exists

The P111 scale gate proved `operator-read`, but left `write_context_read` at
`4/8` exact actions. That mode is not the same product surface as normal read:
it is the bounded read phase a writer performs before deciding how to connect a
new memory node.

This slice creates a separate contract and dataset gate for writer pre-read so
read results cannot hide writer failures, and writer failures cannot block a
read-profile claim.

## 3. Implemented Contract

New conformance suite:

```text
writer-pre-read-v1
```

New dataset capability profile:

```text
writer-pre-read
```

Required capabilities:

| Group | Capabilities |
| --- | --- |
| mode | `mode:write_context_read` |
| tools | `kernel_near`, `kernel_inspect`, `kernel_trace` |
| cursor | `cursor:ref` |
| dimensions | `dimensions.mode:all`, `dimensions.scope:current_about` |
| window policy | `window:expand`, `window:shrink` |
| security | `inspect.raw:false` |
| pagination | `trace.page:first` |
| writer state | `writer.last_tool:none`, `writer.last_tool:kernel_near`, `writer.last_tool:kernel_inspect` |
| candidates | `writer.candidate_role:previous_subtask_answer`, `writer.candidate_role:same_subtask_question` |

The profile deliberately does not include `kernel_wake`, `kernel_ask`,
temporal moves, `kernel_ingest`, or `kernel_write_memory`. This is the read
phase before writing memory, not the write action itself.

## 4. Commands

Generate conformance trajectories:

```bash
cargo run -p rehydration-testkit --bin kernel_operator_conformance_trajectory_export -- \
  --suite writer-pre-read-v1 \
  --run-id kmp-operator-writer-pre-read-v1-20260516 \
  --output /tmp/kernel-operator-conformance-writer-pre-read-v1-20260516 \
  --force
```

Contract coverage:

```bash
cargo run -p rehydration-testkit --bin kernel_operator_contract_coverage -- \
  --profile writer-pre-read \
  --trajectories /tmp/kernel-operator-conformance-writer-pre-read-v1-20260516/trajectories.jsonl \
  --fail-under 100 \
  --output /tmp/kernel-operator-conformance-writer-pre-read-v1-20260516/contract_coverage_writer_pre_read.json
```

Prepare SFT:

```bash
python scripts/operator/prepare_operator_sft_dataset.py \
  --trajectories /tmp/kernel-operator-conformance-writer-pre-read-v1-20260516/trajectories.jsonl \
  --output /tmp/kernel-operator-sft-writer-pre-read-v1-20260516 \
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

Leak audit:

```bash
python scripts/operator/audit_operator_sft_no_gold.py \
  /tmp/kernel-operator-sft-writer-pre-read-v1-20260516/openai_train.jsonl \
  /tmp/kernel-operator-sft-writer-pre-read-v1-20260516/openai_eval.jsonl \
  --output /tmp/kernel-operator-sft-writer-pre-read-v1-20260516/no_gold_audit.json
```

## 5. Result

Conformance export:

| Metric | Value |
| --- | ---: |
| trajectories | 8 |
| mode | `write_context_read` |
| `kernel_near` targets | 4 |
| `kernel_inspect` targets | 2 |
| `kernel_trace` targets | 2 |
| contract validation failures | 0 |

Contract coverage:

| Metric | Value |
| --- | ---: |
| profile contract coverage | 16 / 16 |
| profile contract percent | 100% |
| training target capability coverage | 16 / 16 |
| training target capability percent | 100% |

SFT split:

| Metric | Value |
| --- | ---: |
| selected rows | 8 |
| train rows | 4 |
| eval rows | 4 |
| train capability coverage | 16 / 16 |
| eval capability coverage | 16 / 16 |
| duplicate model rows | 0 |
| non-visible target refs dropped | 0 |
| non-visible target cursors dropped | 0 |

Leak audit:

| Metric | Value |
| --- | ---: |
| rows | 8 |
| findings | 0 |

## 6. Evidence

| Artifact | sha256 |
| --- | --- |
| `results/kernel-operator-conformance-writer-pre-read-v1-20260516/summary.json` | `0c5db143b8526a18b038c82c6b08b166b247aa341bd42944018c720b42a921f8` |
| `results/kernel-operator-conformance-writer-pre-read-v1-20260516/trajectories.jsonl` | `7e0ed2ee16970688f4ee3feca70849e1bc78cd9a15e861aaa28b6056009996cc` |
| `results/kernel-operator-conformance-writer-pre-read-v1-20260516/contract_coverage_writer_pre_read.json` | `69cf62c9290790ea39095c4cfc276d2ce20a71e0b76ac91a80fa5e17de4311b4` |
| `results/kernel-operator-sft-writer-pre-read-v1-20260516/summary.json` | `e611fbf9dedca86a610924c5d73fed5598906145c035513c164591918f2e6fdf` |
| `results/kernel-operator-sft-writer-pre-read-v1-20260516/openai_train.jsonl` | `846d391ab50d178aacb50329bea43fe00e06217bdd7ebcf79390387726415e1d` |
| `results/kernel-operator-sft-writer-pre-read-v1-20260516/openai_eval.jsonl` | `8c9916dfff032dc88fbda2f8509cdf310f68289dc2083e8a49c74195e2afaea5` |
| `results/kernel-operator-sft-writer-pre-read-v1-20260516/no_gold_audit.json` | `f4a68613bb543b34d30ca7a579f759817ff54bae15193e0bbecf8484e78e266b` |

## 7. Decision

```text
dataset-ready
```

This is not a trained model result. It is the first clean writer pre-read
contract slice. The next step is to mix this conformance suite with real
writer pre-read traces, train a dedicated candidate, and evaluate with
`by_mode_eval.write_context_read`.

## 8. Next Steps

1. Build a mixed writer-pre-read SFT cut from real MemoryArena writer traces
   plus `writer-pre-read-v1`.
2. Train a small candidate adapter using only `write_context_read` rows.
3. Evaluate exact action accuracy for `by_mode_eval.write_context_read`.
4. Replay only real writer pre-read rows against deployed KMP/MCP after
   de-anonymization.
