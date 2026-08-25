# Operator Training Run: `opread-golden-v1`

Status: `v5-real-requested-replay-passed`

Date opened: 2026-05-15
Date closed: 2026-05-15
Owner: Tirso / Codex

## 1. Scope

| Field | Value |
| --- | --- |
| Attempt id | `opread-golden-v1` |
| Profile | `operator-read` |
| Base model | `Qwen/Qwen2.5-0.5B-Instruct` |
| Adapter output | `/tmp/kernel-operator-qwen05-lora-opread-goldenv4-p111req200-v5-20260515` |
| Artifact root | `/tmp/kernel-operator-conformance-golden-v1..v4`, `/tmp/kernel-operator-golden-v1..v4-read-sft`, `/tmp/kernel-operator-p111-pageinfo200-requested-sft`, `/tmp/kernel-operator-mixed-goldenv4-p111req200-read-sft`, prediction outputs under `/tmp/kernel-operator-qwen05-predictions-*`; v1/v2/v4/v5 preserved under `../rehydration-kernel-artifacts/operator/2026-05-15-opread-golden-v1/` |
| Branch | `main` dirty worktree during experiment |
| Commit | pending |
| Dirty worktree at start | yes; continuing Operator dataset/process work from the previous attempt |

## 2. North Star Check

```text
Operator 0.5B:
  only learns to use KMP.

Strong teacher:
  produces semantics when semantics are needed.

Kernel:
  validates, stores, traverses, proves, and audits memory.
```

This run is not meant to train a general reasoning model. It is meant to test
whether a small Operator can reliably choose bounded KMP/MCP read actions when
the state contains enough visible evidence to make the decision.

The previous mixed run showed that a small model can learn the dominant
MemoryArena navigation lane extremely well, but that does not prove full KMP
operation. This attempt deliberately shifts from volume to sample quality.

## 3. Hypothesis

Main hypothesis:

```text
A small, high-quality, contrastive Operator-read dataset teaches KMP/MCP
contract behavior better than a large benchmark-dominated dataset.
```

Success means:

- zero invalid predictions on the golden eval split;
- zero missing predictions on the golden eval split;
- zero unbounded tool calls;
- 100% exact action accuracy on KMP conformance read capabilities;
- live MCP replay passes for a small raw-ref smoke;
- no capability regresses against the previous MemoryArena surface.

Failure means:

- the model mixes argument schemas across tools;
- the model passes MemoryArena-like rows but fails held-out conformance rows;
- any eval row requires hidden information not present in `visible_state`;
- any target action is only learnable by memorizing a benchmark pattern.

## 4. Why This Version Exists

The previous `2026-05-14` attempt produced a useful negative result:

| Area | Result |
| --- | ---: |
| MemoryArena exact actions | `1259 / 1259` |
| KMP conformance exact actions | `0 / 13` |
| strict prediction rejections | `3 / 1272` |

The three hard rejections were not broken JSON. They were contract violations:

- `kernel_wake` received navigation fields such as `from`, `include`, `limit`,
  and `window`;
- `kernel_trace` omitted a required `to`;
- `kernel_inspect` omitted `ref` and mixed in navigation fields.

The important lesson is that the model learned the benchmark lane:

```text
kernel_near -> kernel_inspect -> kernel_trace -> stop
```

It did not yet learn the API/MCP contract as a full operating surface.

## 5. Dataset Policy

This version uses a small curated dataset before any large benchmark mixing.

Good rows must satisfy all of the following:

- the `visible_state` contains every value needed to choose the target action;
- the row has exactly one intended decision;
- the target action passes the shared KMP/MCP action contract;
- the prompt does not expose `target_action`, gold answers, hidden benchmark
  labels, source-only fields, raw prompts, or post-hoc tool outputs;
- the row teaches one or two explicit capabilities, not a vague behavior;
- the row is useful in a contrastive pair or a short contrastive family.

Bad rows are quarantined, not patched silently:

- target uses a ref, time, sequence, page cursor, about, or dimension that is
  not visible to the model;
- target can only be inferred from an exporter-only `decision` label;
- target depends on benchmark answer metadata;
- target action is valid but semantically arbitrary;
- row repeats a common benchmark lane without adding a new capability.

## 6. Golden Dataset Shape

The first cut should be intentionally small:

| Block | Purpose | Target size |
| --- | --- | ---: |
| `contract.core` | one clean schema per read tool | 40-80 rows |
| `contract.contrastive` | near-identical states with different correct tools | 80-160 rows |
| `contract.cursor` | `ref`, `time`, and `sequence` cursor choice | 60-120 rows |
| `contract.scope` | `current_about`, explicit `abouts`, `all_abouts` | 40-80 rows |
| `contract.trace_page` | first page vs continue page vs stop after complete page | 40-80 rows |
| `contract.window` | expand, shrink, and stop-sufficient decisions | 40-80 rows |
| `memoryarena.thin` | small real trajectory sanity slice | 100-300 rows |

The target is not maximum row count. The target is maximum decision density.

## 7. Contrastive Families

Each family should contain rows that differ by one meaningful factor.

Examples:

| Family | Row A | Row B | What it teaches |
| --- | --- | --- | --- |
| temporal direction | `rewind` from a visible later point | `forward` from a visible earlier point | do not infer direction from common pattern |
| trace paging | first trace page with `from`/`to` | continue trace with `page.cursor` | page mode changes schema |
| inspect vs near | candidate ref already visible | no useful ref visible | inspect only when ref exists |
| wake vs near | no refs after reset | current ref and prior refs visible | wake is for context bootstrapping |
| scope | current about only | explicit sibling about present | scope is intentional and auditable |
| stop | evidence sufficient | evidence partial | stop is a decision, not a default |

## 8. Required Dataset Audits Before Training

This attempt must not train until these gates are implemented or checked:

| Gate | Required |
| --- | --- |
| duplicate `step_id` | `0` |
| target action contract failures | `0` |
| model-facing leak findings | `0` |
| debug audit export | `debug_audit.jsonl` present |
| missing visible target refs | `0` |
| missing visible target time cursors | `0` |
| missing visible target sequence cursors | `0` |
| missing visible trace page cursor | `0` |
| train capability coverage | `100% operator-read` |
| eval capability coverage | `100% operator-read` |
| eval contrastive family coverage | every family represented |
| per-source dominance | no source can hide conformance failures |

The previous audit only checked visible refs. This version must also check
non-ref cursor values such as temporal `time`, `sequence`, and trace page
`cursor`, because those values are part of the action contract.

## 9. Dataset Generation Plan

Planned steps:

1. Add or use a deterministic golden conformance export cut.
2. Make every cursor target explicitly visible in `visible_state`.
3. Remove or rename exporter-only `operator_state.decision` fields if they
   leak the answer.
4. Keep useful state hints that a real Operator could observe, such as
   `requested_movement`, `requested_scope`, `trace_target_ref`,
   `last_result_page`, `known_refs`, and `remaining_budget`.
5. Build a tiny MemoryArena slice after the contract rows are clean.
6. Prepare SFT with grouped split, anonymized refs, and strict visibility
   requirements.
7. Evaluate first on golden conformance, then on MemoryArena thin sanity.

## 10. Training Configuration

Pending. The first dataset cut is ready; training has not started.

Candidate models to compare after the dataset is clean:

| Model | Reason |
| --- | --- |
| Qwen2.5-0.5B-Instruct | previous baseline, fast, known failure mode |
| Llama-3.2-1B-Instruct | slightly larger, potentially stronger schema adherence |
| small NVIDIA/Nemotron candidate | aligned with GPU-backed Operator direction if runtime is practical |

## 11. Evaluation Order

Evaluation must be staged:

1. offline strict parser;
2. offline action contract validator;
3. exact action policy eval by capability;
4. contrastive-family eval;
5. raw-ref de-anonymized smoke;
6. live MCP replay;
7. MemoryArena thin sanity;
8. only then larger benchmark mixing.

No live replay is needed if the offline golden gate fails.

## 12. Stop Gates

| Gate | Required | Observed | Pass |
| --- | --- | --- | --- |
| golden dataset generated | yes | `62` source trajectories, `46` read SFT rows | yes |
| target cursors visible | 100% | `0` dropped non-visible cursors | yes |
| no-gold audit findings | 0 | `0 / 46` | yes |
| train read coverage | 100% | `24 / 24` capabilities | yes |
| eval read coverage | 100% | `24 / 24` capabilities | yes |
| invalid predictions | 0 | pending | pending |
| missing predictions | 0 | pending | pending |
| unbounded tool calls | 0 | pending | pending |
| conformance exact accuracy | 100% | pending | pending |
| MCP replay failures | 0 | pending | pending |

## 13. Current Decision

Do not continue training from the previous mixed dataset as a publication
candidate.

Next action:

```text
Build and audit the golden Operator-read dataset before launching another GPU
training job.
```

The goal of the next run is not a better global accuracy number. The goal is
to prove the Operator can use the KMP/MCP contract without relying on the
dominant MemoryArena pattern.

## 14. Initial Cursor-Visibility Audit

Before generating the new golden cut, the existing conformance v7 read rows
were passed through the new cursor-visibility gate:

```bash
python scripts/operator/prepare_operator_sft_dataset.py \
  --trajectories /tmp/kernel-operator-conformance-full-v7/trajectories.jsonl \
  --output /tmp/kernel-operator-conformance-full-v7-cursor-audit \
  --include-mode read \
  --eval-ratio 0.25 \
  --split-mode group \
  --group-key task_or_step \
  --seed 42 \
  --anonymize-refs \
  --require-visible-target-refs \
  --require-visible-target-cursors \
  --force
```

Result:

| Metric | Value |
| --- | ---: |
| source rows | `61` |
| selected read rows after cursor gate | `29` |
| dropped non-visible target refs | `0` |
| dropped non-visible target cursors | `16` |

Dropped cursor rows by tool:

| Tool | Dropped rows |
| --- | ---: |
| `kernel_near` | `4` |
| `kernel_goto` | `4` |
| `kernel_rewind` | `4` |
| `kernel_forward` | `4` |

Examples:

| Step | Missing visible cursor |
| --- | --- |
| `near-by-time-expand-window-about-scope` | `time:2026-05-06T10:04:00Z` |
| `goto-by-sequence-final-remediation` | `sequence:11` |
| `rewind-from-sequence-before-remediation` | `sequence:11` |
| `forward-from-time-find-confirmation` | `time:2026-05-06T10:03:00Z` |

Interpretation:

```text
Conformance v7 was contract-valid, but not fully model-honest for time and
sequence cursor rows. Some targets were valid KMP actions that the model could
not infer from visible state alone.
```

This does not invalidate the previous result. It explains why the result was
diagnostic rather than release-quality: the model mixed schemas, and the eval
also contained rows whose non-ref cursor targets were not fully visible.

Golden v1 must fix this before training. The first golden cut below does.

The SFT preparer now exports a structured debug audit file:

```text
<sft-output>/debug_audit.jsonl
```

Each row records:

- whether the sample was kept or dropped;
- train/eval split for kept rows;
- target tool and argument keys;
- target capabilities;
- target refs and cursor values;
- visible refs and visible cursor values;
- exact drop reasons.

The cursor-audit smoke produced:

| Debug artifact | Value |
| --- | ---: |
| `debug_audit.jsonl` rows | `45` |
| kept rows | `29` |
| dropped rows | `16` |

This file is required for golden v1 so the dataset can be audited after the
generation or training job finishes without reconstructing state from stdout.

## 15. Golden V1 First Cut

Generation:

```bash
cargo run -p rehydration-testkit --bin kernel_operator_conformance_trajectory_export -- \
  --output /tmp/kernel-operator-conformance-golden-v1 \
  --run-id kmp-operator-golden-v1 \
  --force
```

Preparation:

```bash
python scripts/operator/prepare_operator_sft_dataset.py \
  --trajectories /tmp/kernel-operator-conformance-golden-v1/trajectories.jsonl \
  --output /tmp/kernel-operator-golden-v1-read-sft \
  --include-mode read \
  --eval-ratio 0.25 \
  --split-mode group \
  --group-key task_or_step \
  --seed 42 \
  --anonymize-refs \
  --require-visible-target-refs \
  --require-visible-target-cursors \
  --capability-split-profile read \
  --require-eval-capability-coverage \
  --require-train-capability-coverage \
  --force
```

Audit commands:

```bash
python scripts/operator/audit_operator_sft_no_gold.py \
  /tmp/kernel-operator-golden-v1-read-sft/openai_train.jsonl \
  /tmp/kernel-operator-golden-v1-read-sft/openai_eval.jsonl \
  --output /tmp/kernel-operator-golden-v1-read-sft/no_gold_audit.json

cargo run -p rehydration-testkit --bin kernel_operator_contract_coverage -- \
  --profile read \
  --trajectories /tmp/kernel-operator-golden-v1-read-sft/all_model_trajectories.jsonl \
  --fail-under 100 \
  --output /tmp/kernel-operator-golden-v1-read-sft/contract_coverage_read.json
```

Dataset evidence:

| Metric | Value |
| --- | ---: |
| source trajectories | `62` |
| source read trajectories | `46` |
| SFT train rows | `34` |
| SFT eval rows | `12` |
| dropped non-visible target refs | `0` |
| dropped non-visible target cursors | `0` |
| no-gold findings | `0 / 46` |
| train read capability coverage | `24 / 24` |
| eval read capability coverage | `24 / 24` |
| read profile contract coverage | `100%` |

Target actions:

| Action | Rows |
| --- | ---: |
| `kernel_ask` | `7` |
| `kernel_forward` | `6` |
| `kernel_goto` | `6` |
| `kernel_inspect` | `2` |
| `kernel_near` | `7` |
| `kernel_rewind` | `6` |
| `kernel_trace` | `4` |
| `kernel_wake` | `2` |
| `stop` | `6` |

Hashes:

| Artifact | SHA-256 |
| --- | --- |
| `/tmp/kernel-operator-conformance-golden-v1/trajectories.jsonl` | `7335ba45e74bea0df1cfefa1ea6c7fca628667617c7aaf3be7edfb4582d8a06c` |
| `/tmp/kernel-operator-golden-v1-read-sft/openai_train.jsonl` | `afaff93edcba0b060adfab6470ba4af0d157ce158750adeeee4ea194e62fe81a` |
| `/tmp/kernel-operator-golden-v1-read-sft/openai_eval.jsonl` | `72d553b1ba4a231ae7d07aa9bfcc29ab81b3bc1e08ff096232e5fec9adff2734` |
| `/tmp/kernel-operator-golden-v1-read-sft/debug_audit.jsonl` | `1d73503966086d0e8ba5286bfa0a9aad4e1625c2a7632b1f12ee7e24dda44f26` |
| `/tmp/kernel-operator-golden-v1-read-sft/no_gold_audit.json` | `942d3f95775a4ae4b55f060e2a9dd8c34b062c26c830c15fb68d0ca6aa1413b5` |
| `/tmp/kernel-operator-golden-v1-read-sft/contract_coverage_read.json` | `35e96b1e72310bb804a6cf323269331e7bdf7a3856036c7c3ad0a69d6604e77f` |

Decision:

```text
dataset-ready
reason: the first golden read cut passes refs, cursors, no-gold, contract, and
train/eval capability gates. It is ready for a small training run, not yet a
model claim.
```

## 16. Golden V1 Training Result

Golden v1 was trained once on Qwen2.5-0.5B-Instruct with a 12 epoch LoRA run.
The training job completed, but the strict policy result was not acceptable as
a model claim.

Training job:

```text
kubernetes job: underpass-runtime/kop-qwen05-lora-opread-golden-v1-20260515
adapter: /tmp/kernel-operator-qwen05-lora-opread-golden-v1-20260515
epochs: 12
train rows: 34
eval rows: 12
```

Training metrics:

| Metric | Value |
| --- | ---: |
| runtime | `114.6s` |
| train loss | `0.1325` |
| final eval loss | `~0.0668` |
| final eval mean token accuracy | `~0.9892` |

Strict prediction result:

| Metric | Value |
| --- | ---: |
| eval rows | `12` |
| predictions | `10` |
| missing / rejected predictions | `2` |
| invalid predictions after parser | `0` |
| exact action accuracy | `1 / 12` |
| tool accuracy | `~81.8%` |
| primary ref accuracy | `~72.7%` |
| cursor mode accuracy | `100%` |
| window shape accuracy | `20%` |
| limit policy accuracy | `40%` |

The two hard failures were useful:

| Step | Failure | Interpretation |
| --- | --- | --- |
| `ask-conflicts-only-dimensions` | predicted unsupported `answer_policy=evidence_or_conflicts` and malformed `abouts` nesting | prompt did not enumerate `show_conflicts`; scope list shape needed stronger instruction |
| `inspect-typed-raw-false` | emitted `an` instead of required `ref` | too few inspect rows and weak inspect-specific instruction |

The broader exact-action miss was also useful. Golden v1 made cursor refs
visible, but still left exact scope, bounds, trace direction and stop reasons
too implicit for a 0.5B model while the evaluator required exact output.

Decision:

```text
v1-model-diagnostic-only
reason: dataset was honest on hidden refs/cursors, but not explicit enough for
strict exact-action training. Do not use the v1 adapter as a release candidate.
```

## 17. Golden V2 Cut

Golden v2 keeps the same intent but makes the operator request explicit in the
model-visible state.

The added visible fields are not benchmark gold labels. They represent the
bounded request that a real LLM/controller can hand to Operator:

| Field | Purpose |
| --- | --- |
| `requested_wake` | intent, role, dimensions, and wake budget |
| `requested_ask` | question, answer policy, dimensions, budget, and depth |
| `requested_move` | temporal tool intent, cursor key, and cursor value |
| `requested_scope` | explicit dimension selection |
| `requested_bounds` | include flags, limit, window, and budget |
| `requested_trace` | trace direction, goal, role, budget, and page cursor |
| `inspection_request` | exact typed inspect request with `raw=false` |
| `requested_stop` | answer policy, final refs, and fail-fast reason |

This changes the experiment from “can 0.5B infer every policy value from sparse
state?” to “can 0.5B reliably translate an explicit KMP operation request into
the strict MCP/API contract without corrupting it?”

Generation:

```bash
cargo run -p rehydration-testkit --bin kernel_operator_conformance_trajectory_export -- \
  --output /tmp/kernel-operator-conformance-golden-v2 \
  --run-id kmp-operator-golden-v2 \
  --force
```

Preparation:

```bash
python scripts/operator/prepare_operator_sft_dataset.py \
  --trajectories /tmp/kernel-operator-conformance-golden-v2/trajectories.jsonl \
  --output /tmp/kernel-operator-golden-v2-read-sft \
  --include-mode read \
  --eval-ratio 0.25 \
  --split-mode group \
  --group-key task_or_step \
  --seed 42 \
  --anonymize-refs \
  --require-visible-target-refs \
  --require-visible-target-cursors \
  --capability-split-profile read \
  --require-eval-capability-coverage \
  --require-train-capability-coverage \
  --force
```

Dataset evidence:

| Metric | Value |
| --- | ---: |
| source trajectories | `64` |
| source read trajectories | `48` |
| SFT train rows | `36` |
| SFT eval rows | `12` |
| dropped non-visible target refs | `0` |
| dropped non-visible target cursors | `0` |
| no-gold findings | `0 / 48` |
| train read capability coverage | `24 / 24` |
| eval read capability coverage | `24 / 24` |
| read profile contract coverage | `100%` |

Target actions:

| Action | Rows |
| --- | ---: |
| `kernel_ask` | `7` |
| `kernel_forward` | `6` |
| `kernel_goto` | `6` |
| `kernel_inspect` | `4` |
| `kernel_near` | `7` |
| `kernel_rewind` | `6` |
| `kernel_trace` | `4` |
| `kernel_wake` | `2` |
| `stop` | `6` |

Hashes:

| Artifact | SHA-256 |
| --- | --- |
| `/tmp/kernel-operator-conformance-golden-v2/trajectories.jsonl` | `02eb5a136bb3472c3c20e01cabbcc8944e49a28f94c39c7f51a4aa7f66194d8f` |
| `/tmp/kernel-operator-golden-v2-read-sft/openai_train.jsonl` | `ef6377d09011bcef04e2ea3d04bfd0e9c54fd75ae3178fe036d6f6f6caaee7f0` |
| `/tmp/kernel-operator-golden-v2-read-sft/openai_eval.jsonl` | `0817213c4b4a332c4a9c3d3abe633f5de286116d4ce5dd62041c152190f202d9` |
| `/tmp/kernel-operator-golden-v2-read-sft/debug_audit.jsonl` | `7c5152419a68ff8a469137b600fee6cf55db6e56b6b3d1c54bff4f6e8cf92046` |
| `/tmp/kernel-operator-golden-v2-read-sft/no_gold_audit.json` | `7d044fbd255d163891114846106ba083c1404abc55850ec5416bab5ce8b35b6c` |
| `/tmp/kernel-operator-golden-v2-read-sft/contract_coverage_read.json` | `e64737e1d8c7351f36b0d0af9cdf24ee6cfae85e15c2724d2b85fffaa42233e4` |

Training job:

```text
kubernetes job: underpass-runtime/kop-qwen05-lora-opread-golden-v2-20260515
adapter: /tmp/kernel-operator-qwen05-lora-opread-golden-v2-20260515
base model: Qwen/Qwen2.5-0.5B-Instruct
epochs: 8
status: complete
```

Training result:

| Metric | Value |
| --- | ---: |
| runtime | `94.2s` |
| train rows | `36` |
| eval rows | `12` |
| train steps | `144` |
| train loss | `0.1705` |
| final eval loss | `0.04926` |
| final eval mean token accuracy | `0.9919` |

Strict prediction job:

```text
kubernetes job: underpass-runtime/kop-qwen05-predict-opread-golden-v2-20260515
predictions: /tmp/kernel-operator-qwen05-predictions-opread-golden-v2-20260515
status: complete
```

Prediction summary:

| Metric | Value |
| --- | ---: |
| selected eval rows | `12` |
| valid predictions | `12` |
| strict parser failures | `0` |
| action contract failures | `0` |
| schema mode | `strict-no-additional-properties` |

Policy evaluation:

| Metric | Value |
| --- | ---: |
| exact action accuracy | `12 / 12` |
| action type accuracy | `100%` |
| tool accuracy | `100%` |
| primary ref accuracy | `100%` |
| scope accuracy | `100%` |
| cursor mode accuracy | `100%` |
| window shape accuracy | `100%` |
| limit policy accuracy | `100%` |
| trace continue page accuracy | `100%` |
| stop accuracy | `100%` |
| invalid prediction rate | `0%` |
| unbounded tool call rate | `0%` |

Evaluation artifacts:

| Artifact | SHA-256 |
| --- | --- |
| `/tmp/kernel-operator-qwen05-predictions-opread-golden-v2-20260515/predictions.jsonl` | `d0dbbd5a39f1cb2b99d78160b0c58b23ff2a9a8a631764b67cc1b43f6d1ada6d` |
| `/tmp/kernel-operator-qwen05-predictions-opread-golden-v2-20260515/summary.json` | `87e76e4ba7c406c2a476b6369929f19ee2c2c40415987f701e2db5cba8f41557` |
| `/tmp/kernel-operator-qwen05-predictions-opread-golden-v2-20260515-policy-eval.json` | `e5ede23678dbb30c4f7fc75d0547e4ab3f4132d8bbb571c9b40498d74ebc9518` |
| `/tmp/kernel-operator-qwen05-predictions-opread-golden-v2-20260515-policy-details.jsonl` | `8bddf2ca837907464da4e9c226ce0e0e780a543c5f509dead4ee8b7e54af4892` |
| `/tmp/kernel-operator-qwen05-lora-opread-golden-v2-20260515/adapter_model.safetensors` | `28e637592900b546bb52f01e1db4901428b32bb3a5b06f2f4fb2afca6b7e72a5` |
| `/tmp/kernel-operator-qwen05-lora-opread-golden-v2-20260515/adapter_config.json` | `6f08501815946994499c929a4d829842e961aac77a530e1f8b9d819624d55551` |

Decision:

```text
v2-golden-eval-passed
reason: Qwen2.5-0.5B can learn the explicit Operator-read KMP/MCP contract
mapping on this golden split with zero strict parser failures, zero contract
failures, zero unbounded tool calls, and 100% exact action accuracy.
```

This is still not a production model claim. It proves the training lane is now
capable of teaching the read contract when the request is explicit and the
dataset gates are strict. The next validation must test generalization against
new conformance rows and a thin real benchmark slice.

## 18. Generalization Holdout V1

Golden v2 was then tested against a new synthetic read holdout with a different
about and sibling about:

```text
suite: read-generalization
run id: kmp-operator-read-generalization-v1
source trajectories: 24
mode: read only
read capability coverage: 24 / 24
```

The holdout was not used to train the v2 adapter.

Result with the v2 adapter:

| Metric | Value |
| --- | ---: |
| selected rows | `24` |
| valid predictions | `23` |
| strict/action failures | `1` |
| exact action accuracy | `23 / 24` |

The single failure was important:

| Step | Expected | Predicted | Why It Matters |
| --- | --- | --- | --- |
| `holdout-wake-after-empty-near` | `kernel_wake` | invalid `kernel_near` with wake-shaped args | the model over-weighted `current_ref` and previous `kernel_near` state instead of obeying the explicit `requested_wake` field |

Decision:

```text
v2-generalization-diagnostic
reason: v2 passed the fixed golden split, but one realistic state transition
showed a tool-selection bias. The dataset needed a contrastive row, not a
looser evaluator or a fallback.
```

## 19. Golden V3 Diagnostic

Golden v3 added one contrastive read row:

```text
wake-after-empty-near-current-ref-visible
```

Purpose: teach that `requested_wake` wins even when `current_ref` is visible
and the previous tool was `kernel_near`.

Training result:

| Metric | Value |
| --- | ---: |
| source trajectories | `65` |
| read rows | `49` |
| train rows | `37` |
| eval rows | `12` |
| train/eval read capability coverage | `24 / 24` |
| runtime | `101.4s` |
| final eval loss | `0.04654` |
| final eval mean token accuracy | `0.9915` |

Holdout result with the v3 adapter:

| Metric | Value |
| --- | ---: |
| selected rows | `24` |
| valid predictions | `24` |
| exact action accuracy | `24 / 24` |
| invalid prediction rate | `0%` |

Golden eval result with the v3 adapter:

| Metric | Value |
| --- | ---: |
| selected rows | `12` |
| valid predictions | `11` |
| strict/action failures | `1` |

The regression was on:

```text
near-by-ref-shrink-window-except-discarded
```

Two variants of the failure were observed while tightening the prompt:

- malformed JSON with `include`, `limit`, and `window` nested under a top-level
  `exclude` object;
- valid JSON shape, but `exclude` emitted as a top-level field instead of
  `arguments.dimensions`.

Root cause:

```text
The training set still had too little contrast for dimensions.mode=except.
Prompt rules helped, but one 0.5B adapter example was not enough to make the
argument boundary robust.
```

Decision:

```text
v3-diagnostic-only
reason: v3 fixed the wake-after-empty-near holdout but regressed a golden
dimension-filter row. Keep it as evidence for contrastive-data design; do not
promote the v3 adapter.
```

## 20. Golden V4 Result

Golden v4 adds a second `dimensions.mode=except` training contrast. The target
is not to make the model infer semantics; it is to make the model reliably
translate an explicit model-visible request into the strict KMP/MCP action
contract.

Generation:

```bash
cargo run -p rehydration-testkit --bin kernel_operator_conformance_trajectory_export -- \
  --output /tmp/kernel-operator-conformance-golden-v4 \
  --run-id kmp-operator-golden-v4 \
  --suite golden-v4 \
  --force
```

Preparation:

```bash
python scripts/operator/prepare_operator_sft_dataset.py \
  --trajectories /tmp/kernel-operator-conformance-golden-v4/trajectories.jsonl \
  --output /tmp/kernel-operator-golden-v4-read-sft \
  --include-mode read \
  --eval-ratio 0.25 \
  --split-mode group \
  --group-key task_or_step \
  --eval-group-values-file /tmp/kernel-operator-golden-v4-eval-groups.txt \
  --seed 42 \
  --anonymize-refs \
  --require-visible-target-refs \
  --require-visible-target-cursors \
  --capability-split-profile read \
  --require-eval-capability-coverage \
  --require-train-capability-coverage \
  --force
```

Dataset evidence:

| Metric | Value |
| --- | ---: |
| source trajectories | `66` |
| source read trajectories | `50` |
| SFT train rows | `38` |
| SFT eval rows | `12` |
| dropped non-visible target refs | `0` |
| dropped non-visible target cursors | `0` |
| no-gold findings | `0 / 50` |
| train read capability coverage | `24 / 24` |
| eval read capability coverage | `24 / 24` |
| read profile contract coverage | `100%` |
| `dimensions.mode=except` rows | `3` total, `2` train, `1` eval |

Training job:

```text
kubernetes job: underpass-runtime/kop-qwen05-lora-opread-golden-v4-20260515
adapter: /tmp/kernel-operator-qwen05-lora-opread-golden-v4-20260515
base model: Qwen/Qwen2.5-0.5B-Instruct
epochs: 8
status: complete
```

Training result:

| Metric | Value |
| --- | ---: |
| runtime | `255.8s` |
| train rows | `38` |
| eval rows | `12` |
| train steps | `152` |
| train loss | `0.165` |
| final eval loss | `0.03906` |
| final eval mean token accuracy | `0.9930` |

Golden v4 eval:

| Metric | Value |
| --- | ---: |
| selected rows | `12` |
| valid predictions | `12` |
| strict parser failures | `0` |
| action contract failures | `0` |
| exact action accuracy | `12 / 12` |
| action type accuracy | `100%` |
| tool accuracy | `100%` |
| primary ref accuracy | `100%` |
| scope accuracy | `100%` |
| cursor mode accuracy | `100%` |
| window shape accuracy | `100%` |
| limit policy accuracy | `100%` |
| trace continue page accuracy | `100%` |
| stop accuracy | `100%` |
| invalid prediction rate | `0%` |
| unbounded tool call rate | `0%` |

Generalization holdout with the v4 adapter:

| Metric | Value |
| --- | ---: |
| selected rows | `24` |
| valid predictions | `24` |
| strict parser failures | `0` |
| action contract failures | `0` |
| exact action accuracy | `24 / 24` |
| action type accuracy | `100%` |
| tool accuracy | `100%` |
| primary ref accuracy | `100%` |
| scope accuracy | `100%` |
| cursor mode accuracy | `100%` |
| window shape accuracy | `100%` |
| limit policy accuracy | `100%` |
| trace continue page accuracy | `100%` |
| stop accuracy | `100%` |
| invalid prediction rate | `0%` |
| unbounded tool call rate | `0%` |

The two previously failing behaviors are now correct:

| Behavior | v2 | v3 | v4 |
| --- | --- | --- | --- |
| `requested_wake` after empty `kernel_near` | fail | pass | pass |
| `kernel_near` with `dimensions.mode=except` | pass on golden, not stressed enough | fail | pass |

Important evaluation note:

```text
When the SFT dataset uses --anonymize-refs, policy evaluation must compare
predictions against *_model_trajectories.jsonl. Comparing anonymized
predictions against raw audit trajectories creates false ref/scope failures.
```

Evaluation artifacts:

| Artifact | SHA-256 |
| --- | --- |
| `/tmp/kernel-operator-conformance-golden-v4/trajectories.jsonl` | `96be5d2a5e1ddd140650f42af1903e9cbced6809a75f67c31cd530494c03a02b` |
| `/tmp/kernel-operator-golden-v4-read-sft/openai_train.jsonl` | `e1472087dff1f1e051486f7a91e6e154238c6c5909c87342d1e623c17e284215` |
| `/tmp/kernel-operator-golden-v4-read-sft/openai_eval.jsonl` | `98558c55dd598f6ac09b03e24c371708103ab4561f7d838b84e233a320d76908` |
| `/tmp/kernel-operator-golden-v4-read-sft/debug_audit.jsonl` | `4237b91f00c738d22883cdaa68ebcc566910f2030616f5aff5c440b19ea300ac` |
| `/tmp/kernel-operator-golden-v4-read-sft/no_gold_audit.json` | `555f3a3c0b215881c64ad0a75794114f35b0a1d577214b10e18cf44100b3143b` |
| `/tmp/kernel-operator-golden-v4-read-sft/contract_coverage.json` | `7baf25253efbaf1e14e2bbb6c56b4ddc529163840ecfb9fc611bd70f0803da92` |
| `/tmp/kernel-operator-qwen05-predictions-opread-golden-v4-20260515/predictions.jsonl` | `570eaf8fa6ad13281f9fd06306ce2437c1abfc638cc6ee18d412d3fc74b9ede1` |
| `/tmp/kernel-operator-qwen05-predictions-opread-golden-v4-20260515/summary.json` | `19715499da15f2cc68c8469731e699a1e6b0b35380ba3c48d791162eb8c85dda` |
| `/tmp/kernel-operator-qwen05-predictions-opread-golden-v4-20260515-policy-eval.json` | `3cc4a36266042712219d14ac45508612682bab806939b8d22b80def62cfde891` |
| `/tmp/kernel-operator-qwen05-predictions-opread-golden-v4-20260515-policy-details.jsonl` | `b3af4ea842551b6fd94d1d5ba097b2ef9dc238d73d18e1dc0ef4d806800a89ac` |
| `/tmp/kernel-operator-qwen05-predictions-read-generalization-v1-v4adapter-20260515/predictions.jsonl` | `caa049bc536081af13c162c398a6a8b9f3323f681adb99d05f90da2a54eb023d` |
| `/tmp/kernel-operator-qwen05-predictions-read-generalization-v1-v4adapter-20260515/summary.json` | `5570e22d767efdc09581b771e017bd34ec485b26253d1df15721ae24cd42f2d9` |
| `/tmp/kernel-operator-qwen05-predictions-read-generalization-v1-v4adapter-20260515-policy-eval.json` | `eaed7074233aec9237efa2d802ff0b959c6c6a23ae238b6d0a1b861903d77817` |
| `/tmp/kernel-operator-qwen05-predictions-read-generalization-v1-v4adapter-20260515-policy-details.jsonl` | `5b221268c863365666f10ee7c577e53221dc938d626289f5ad3d7865b0fd5d18` |
| `/tmp/kernel-operator-qwen05-lora-opread-golden-v4-20260515/adapter_model.safetensors` | `96a2ac6f528b12bc0b75501679fd6a193afdf4003b4bdf6cce5f661d03beecb9` |
| `/tmp/kernel-operator-qwen05-lora-opread-golden-v4-20260515/adapter_config.json` | `d78e00f6536eb9fb008aaa336c8d738c6ea813bbb27bc5a462f10a9b26909ece` |

Decision:

```text
v4-golden-plus-holdout-passed
reason: the v4 adapter passes the fixed golden read split and the independent
read-generalization holdout with zero strict parser failures, zero action
contract failures, zero unbounded tool calls, and 100% exact action accuracy.
```

This closes the first high-quality Operator-read conformance lane. It is still
not a production release claim: the next gate is real KMP/MCP replay against a
thin benchmark slice, then broader benchmark trajectories, then write/smart
write coverage.

## 21. Real KMP/MCP Replay Gate

The v4 adapter passed synthetic conformance and an independent synthetic
holdout, but that was not enough. The next gate was to execute predicted
actions against the real deployed kernel through the public TLS endpoint:

```text
https://rehydration-kernel.underpassai.com
```

The first attempt de-anonymized the v4 synthetic golden predictions and tried a
small live replay. This correctly reached KMP/MCP, but failed with `NotFound`:

```text
Node not found: incident:mobile-login:hypothesis:network-timeout
```

Decision:

```text
do not use synthetic conformance refs for live replay
```

Reason: the golden conformance dataset is an API/MCP contract corpus. Its refs
are not populated in the deployed benchmark kernel. Live replay must use refs
that exist in the deployed persistence, such as the populated MemoryArena P1.11
run.

While doing this, the de-anonymizer was tightened so it maps both synthetic
refs and synthetic about ids:

```text
ref_0001   -> raw kernel ref
about_0001 -> raw about id
```

This matters because scoped actions can be structurally correct offline and
still fail raw replay if the synthetic `about` value is not restored.

## 22. P111 Compatibility Diagnostics

The first real benchmark slice used the MemoryArena P1.11 trajectory corpus
already present in the deployed kernel:

```text
/tmp/kernel-operator-trajectories-p111-pageinfo-221-20260512/trajectories.jsonl
```

Three diagnostic cuts were run before training v5:

| Cut | Rows | Adapter | Result | Interpretation |
| --- | ---: | --- | --- | --- |
| Old P111 prompt | `20` | v4 | `0 / 20` valid | Old benchmark-shaped prompts do not match the explicit v4 request contract. |
| Current prompt, no requested fields | `20` | v4 | `0 / 20` valid | The model was being asked to plan from state, not translate an explicit KMP request. |
| Current prompt, target projected to requested fields | `20` | v4 | `17 / 20` valid, `14 / 20` exact | The contract direction works, but MemoryArena-shaped `kernel_near` rows needed real examples. |

The failures in the third cut were useful. They showed a narrow schema mistake:
`kernel_near` sometimes omitted the required `around` argument. The model had
learned synthetic `requested_move` rows, but had not seen enough real P111 rows
with the requested move projected into visible state.

This led to a new dataset preparation option:

```bash
python scripts/operator/prepare_operator_sft_dataset.py \
  --trajectories <trajectories.jsonl> \
  --output <operator-sft-dir> \
  --include-mode read \
  --anonymize-refs \
  --require-visible-target-refs \
  --inject-target-request-fields \
  --force
```

Important boundary:

```text
--inject-target-request-fields is not a benchmark decision claim.
```

It projects the audited target action into model-visible `requested_*` fields
so the small Operator can be evaluated as a contract translator. It must not be
used to claim the Operator independently discovered the correct benchmark
decision.

The product interpretation is deliberate:

```text
LLM or controller: decides the memory operation it wants.
Operator 0.5B: translates that explicit request into the strict KMP/MCP action.
Kernel: validates and executes the action through the typed API.
```

Training the Operator as a planner is a separate capability and requires a
different dataset.

## 23. P111 Oracle Replay Sanity

Before trusting model predictions, the replay path was validated with oracle
target actions on the same requested P111 slice.

Result:

| Metric | Value |
| --- | ---: |
| selected rows | `20` |
| executed tool calls | `15` |
| successful tool calls | `15` |
| failed tool calls | `0` |
| missing expected ref rows | `0` |
| partial result rows | `6` |

This proved that:

- the deployed kernel contains the P111 benchmark data;
- the public TLS endpoint is usable for this replay path;
- `kernel_operator_mcp_replay` can execute raw predictions against real KMP/MCP;
- partial `kernel_near` results are normal when paging or limits are active.

## 24. Golden V4 + P111 Requested V5

V5 mixes the fixed golden v4 contract corpus with a small real P111 requested
slice. The goal is still not autonomous planning. The goal is to keep 100%
contract coverage while adding real benchmark-shaped requests.

P111 requested dataset:

```bash
python scripts/operator/prepare_operator_sft_dataset.py \
  --trajectories /tmp/kernel-operator-trajectories-p111-pageinfo-221-20260512/trajectories.jsonl \
  --output /tmp/kernel-operator-p111-pageinfo200-requested-sft \
  --include-mode read \
  --limit 200 \
  --eval-ratio 0.2 \
  --split-mode group \
  --group-key task_id \
  --seed 42 \
  --anonymize-refs \
  --require-visible-target-refs \
  --inject-target-request-fields \
  --force
```

P111 requested dataset evidence:

| Metric | Value |
| --- | ---: |
| selected read rows | `200` |
| train rows | `157` |
| eval rows | `43` |
| eval task groups | `1` |
| `stop` rows | `51` |
| `kernel_inspect` rows | `52` |
| `kernel_near` rows | `52` |
| `kernel_trace` rows | `45` |

Mixed v5 dataset:

| Source | Train | Eval |
| --- | ---: | ---: |
| Golden v4 conformance | `38` | `12` |
| P111 requested | `157` | `43` |
| Total | `195` | `55` |

Contract coverage:

| Check | Value |
| --- | ---: |
| eval `operator-read` coverage | `24 / 24` |
| eval profile coverage | `100%` |

Training:

```text
kubernetes job: underpass-runtime/kop-qwen05-lora-opread-v5-20260515
adapter: /tmp/kernel-operator-qwen05-lora-opread-goldenv4-p111req200-v5-20260515
base model: Qwen/Qwen2.5-0.5B-Instruct
epochs: 4
status: complete
```

Training result:

| Metric | Value |
| --- | ---: |
| runtime | `256.1s` |
| train rows | `195` |
| eval rows | `55` |
| train loss | `0.08309` |
| final eval loss | `0.01326` |
| final eval mean token accuracy | `0.9969` |

Mixed policy eval:

| Metric | Value |
| --- | ---: |
| selected rows | `55` |
| valid predictions | `55` |
| strict parser failures | `0` |
| action contract failures | `0` |
| exact action accuracy | `55 / 55` |
| tool accuracy | `100%` |
| primary ref accuracy | `100%` |
| scope accuracy | `100%` |
| cursor mode accuracy | `100%` |
| window shape accuracy | `100%` |
| limit policy accuracy | `100%` |
| trace continue page accuracy | `100%` |
| stop accuracy | `100%` |
| invalid prediction rate | `0%` |
| unbounded tool call rate | `0%` |

Raw P111 policy eval after de-anonymization:

| Metric | Value |
| --- | ---: |
| selected rows | `43` |
| written raw predictions | `43` |
| de-anonymization failures | `0` |
| mapped synthetic refs | `159` |
| mapped synthetic about ids | `43` |
| exact action accuracy | `43 / 43` |
| invalid prediction rate | `0%` |
| unbounded tool call rate | `0%` |

Live MCP replay against the deployed kernel:

| Metric | Value |
| --- | ---: |
| endpoint | `https://rehydration-kernel.underpassai.com` |
| selected rows | `43` |
| tool calls | `32` |
| stop actions | `11` |
| executed tool calls | `32` |
| successful tool calls | `32` |
| failed tool calls | `0` |
| missing predictions | `0` |
| invalid predictions | `0` |
| unbounded tool calls | `0` |
| missing expected ref rows | `0` |
| missing expected ref total | `0` |
| partial result rows | `11` |

Replay action mix:

| Action | Count |
| --- | ---: |
| `stop` | `11` |
| `kernel_inspect` | `11` |
| `kernel_near` | `11` |
| `kernel_trace` | `10` |

Replay latency:

| Action | Avg ms | Max ms |
| --- | ---: | ---: |
| `kernel_inspect` | `131.9` | `163` |
| `kernel_near` | `2282.9` | `3760` |
| `kernel_trace` | `142.0` | `166` |

The `kernel_near` rows were all marked partial, which is expected for bounded
navigation. This is not a failure: the replay still found all expected refs.

Evaluation artifacts:

| Artifact | SHA-256 |
| --- | --- |
| `/tmp/kernel-operator-p111-pageinfo200-requested-sft/openai_train.jsonl` | `239db31e8e30c9231492fa28265b944f0c5e474a949579f733bd54fa75ed8d08` |
| `/tmp/kernel-operator-p111-pageinfo200-requested-sft/openai_eval.jsonl` | `b41e70d6de8ef102690616c351611e42b00c975bc0af0caffce238ae25db77c0` |
| `/tmp/kernel-operator-mixed-goldenv4-p111req200-read-sft/openai_train.jsonl` | `3f016014ea5aa35a9b1ccc408c18ae86075b23993f8afd7cefbdada0137b034a` |
| `/tmp/kernel-operator-mixed-goldenv4-p111req200-read-sft/openai_eval.jsonl` | `da42025af2646a10417ec194c1be08e0beed29331db99599a99b86561f98419e` |
| `/tmp/kernel-operator-qwen05-lora-opread-goldenv4-p111req200-v5-20260515/adapter_model.safetensors` | `b9c977257a6895cacdbddf0e63d3e16bf6da1969da32e5f6c9c8dc97944a72a7` |
| `/tmp/kernel-operator-qwen05-lora-opread-goldenv4-p111req200-v5-20260515/adapter_config.json` | `dbffd78eed3f475b8bdcd8bc842d0c79cdb4e5bef9fff4b99b8c04967d9939f1` |
| `/tmp/kernel-operator-qwen05-predictions-opread-goldenv4-p111req200-v5-20260515/predictions.jsonl` | `d2ec2e6810437cc9233d55b406c9d45ab194ed91c120455ebe104d24121c50db` |
| `/tmp/kernel-operator-qwen05-predictions-opread-goldenv4-p111req200-v5-20260515/summary.json` | `7b7565d939bc65ce8f7c28b82a95ab887a71a8645409c0b7bf0ca8aebba6ba7a` |
| `/tmp/kernel-operator-qwen05-predictions-opread-goldenv4-p111req200-v5-20260515-policy-eval.json` | `278b497158ac9a2a8085bc6e882f4e00acdca70c989ae3e628a90745b795b648` |
| `/tmp/kernel-operator-qwen05-predictions-p111-pageinfo200-requested-v5-20260515-raw/predictions.jsonl` | `9e56f7eda5a7f344b7624b2fe4bbc4927745f4aa52379e25c963c1af969a02b9` |
| `/tmp/kernel-operator-qwen05-predictions-p111-pageinfo200-requested-v5-20260515-raw/summary.json` | `8f97941e8989d6b47b348783c007b57142247f36a2ee6ce89000bf7face4e0dc` |
| `/tmp/kernel-operator-qwen05-predictions-p111-pageinfo200-requested-v5-20260515-raw-policy-eval.json` | `60486304e3065cfcbfb3e5745b0c35620aa367d7b8fa891c8e9227e6385d4f9c` |
| `/tmp/kernel-operator-qwen05-predictions-p111-pageinfo200-requested-v5-20260515-mcp-replay/summary.json` | `39623a5cc1a0395de54f34ce13ce296fae295382da55bf0091668772f8a83d5a` |
| `/tmp/kernel-operator-qwen05-predictions-p111-pageinfo200-requested-v5-20260515-mcp-replay/results.jsonl` | `8ea4a17bd9127150ee3e6333ada02b4b379a5ed30ba6214821cbaf13a378fc02` |

Decision:

```text
v5-real-requested-replay-passed
reason: the v5 adapter keeps 100% golden read contract coverage, passes the
mixed golden+P111 requested eval with 55/55 exact actions, de-anonymizes P111
predictions with zero failures, and executes 32/32 real tool calls successfully
against the deployed KMP/MCP endpoint with zero missing expected refs.
```

This is the first useful Operator-read baseline for real KMP/MCP replay.

It is not yet the final Operator product. The next work is:

- scale requested real replay beyond the 43-row P111 eval slice;
- add LongMemEval requested trajectories;
- add explicit planner datasets if Operator should choose operations from raw
  task state instead of translating upstream requests;
- add write/smart-write contract training only after the write boundary is
  audited with the same strictness;
- keep pagination and bounded navigation as first-class training requirements.
