# Kernel Tool Operator Model Plan

> **Canonical pre-v8-divergence design baseline.** This doc defines the intended
> design (anonymized refs, schema-in-prompt, offline teacher) and the V6 result
> (1.000 exact acc). A 2026-05-29 audit found v7/v8 training diverged from it by
> dropping ref anonymization; for the divergence record + corrective plan see the
> operator repo `docs/training/DIVERGENCE_AND_CORRECTIVE_PLAN_2026-05-29.md`. Do not
> alter the V6 result or design content below.

Status: P1.1-P1.10 are implemented: trajectory export, offline evaluation,
deterministic and generalist baselines, ref-safe datasets, SFT LoRA training,
explicit holdout validation, raw-ref de-anonymization, and live MCP replay.
The row-split LoRA run remains only a smoke test. The preferred result is now
the V6 explicit holdout20 run: grouped task split with tasks 80-99 reserved for
eval, synthetic model-facing refs, zero dropped non-visible target refs,
structural writer candidate details, and evaluation against anonymized model
trajectories. It reached 1.000 exact action accuracy, 1.000 primary-ref
accuracy, and zero invalid or unbounded actions on 1,124 held-out decisions.
The same 1,124 predictions were regenerated with the strict
`kernel-operator-action-contract-v1` validator, de-anonymized, and replayed
through the public TLS MCP/gRPC endpoint on 2026-05-14: 976 executed tool
calls, 148 stop actions, 0 MCP failures, 0 invalid actions, 0 unbounded calls,
0 missing expected refs, 424 explicit partial results, and 10m15.9s elapsed.
The P1.11.0 pagination/progress/replay gate is clean for this V6 holdout. The
next publication requirement is a larger fresh MemoryArena run with the same
strict contract and live replay evidence.

Date: 2026-05-12.

Update 2026-05-13: the previous Operator models are retained as baselines, not
discarded. The clean current training base is the MemoryArena P1.11 corpus.
The mixed MemoryArena + LongMemEval run is internal comparison only. The
LongMemEval-S cleaned 500 full-history artifacts are quarantined because the
dataset contains repeated or normalized-colliding `session_id`s inside some
questions, which used to generate duplicate dimensions. The adapter now fails
fast on that unsupported shape. See
[`operator-training-data-audit-2026-05-13.md`](operator-training-data-audit-2026-05-13.md).

Update 2026-05-13, later cut: a LongMemEval-only legacy experiment was run to
stress the Operator before the next MemoryArena-first training pass. It exposed
and fixed a duplicate `step_id` problem in trajectory export/evaluation. The
corrected v7 LoRA run is valid as an internal stress result, but not yet a
model-release candidate: exact action accuracy reached 0.7000 on 60 held-out
decisions, with zero invalid or unbounded tool calls, but 12 strict parse
failures concentrated in smart-writer `read:1` steps. The next Operator gate is
single-action termination/constrained emission, without parser fallback.

Update 2026-05-13, backbone check: Hammer2.0-0.5B was tested as a
function-calling 0.5B alternative on the same corrected LongMemEval v7 slice.
It parsed more actions than Qwen under the strict single-JSON contract, but
policy quality was lower: exact action accuracy reached 0.6500, with 7
prediction failures and 1 unbounded tool call. It remains a useful measured
candidate, not a release candidate. Google FunctionGemma 270M is now the next
sub-0.5B candidate to evaluate because it is explicitly designed for
tool/function calling and has a dedicated tuning workflow for custom function
schemas.

FunctionGemma relaunch note: the Kubernetes secret for Hugging Face exists, but
the first launch was not authorized for the gated `google/functiongemma-270m-it`
repository. After accepting the terms for the token account, the same manifest
trained successfully. The compatibility-cut result does not beat Qwen or
Hammer: exact action accuracy reached 0.5000, with 30 strict prediction
failures and zero unbounded calls. This does not fully evaluate FunctionGemma's
intended mode because the cut used the generic chat/JSON prompt, not
FunctionGemma-native tool schemas.

FunctionGemma-native follow-up: KMP actions were mapped to FunctionGemma tool
declarations and predictions were parsed back into the same policy-eval
contract. The native cut trained cleanly and reached final eval loss 0.05202,
but policy quality remained weak: 39/60 parsed predictions, 0.4667 exact action
accuracy, 0 unbounded tool calls. It overused `kmp_ask` and `kernel_stop`
and did not reliably choose `kmp_near` or `kmp_inspect`. FunctionGemma
270M is therefore not the current Operator backbone.

Llama 3.2 1B control note: `meta-llama/Llama-3.2-1B-Instruct` was added as a
larger control candidate. It is not a sub-0.5B Operator target. The first
cluster launch failed before model load because Hugging Face reports the
access request is awaiting review from the repo authors. Relaunch only after
the token account is approved for the model.

SmolLM2 360M control note: `HuggingFaceTB/SmolLM2-360M-Instruct` is public,
Apache-2.0, not gated, and uses the standard Llama architecture. It trained
quickly on the LongMemEval v7 slice, reaching final eval loss 0.07627, but it
is not a viable Operator backbone under the strict single-JSON contract: only
38/60 predictions parsed, exact action accuracy reached 0.4833, and 3 tool
calls were unbounded. Failures were dominated by incomplete JSON and invalid
stop refs. The issue is structured action reliability, not GPU memory or
training throughput.

Nemotron Flash 3B note: `nvidia/Nemotron-Flash-3B-Instruct` remains
architecturally interesting for NVIDIA alignment, but the current local stack
is blocked. The model requires `flash_attn`, `mamba_ssm`, and `causal_conv1d`
at import time. A CUDA 13 / Torch 2.9.1 image compiled `flash-attn`, but failed
on `causal-conv1d` and `mamba-ssm`: their expected prebuilt wheels are not
available for this combination and the PyPI source packages do not contain the
required C++/CUDA sources. Do not treat this as a model-quality result.

LFM2.5 Nova Function Calling note: `NovachronoAI/LFM2.5-1.2B-Nova-Function-Calling`
was added as the next Hugging Face candidate. It is larger than the preferred
0.5B target, but remains small, public, Apache-2.0, safetensors-based, and is
explicitly tuned for function calling/tool use. It is a useful non-Qwen control
for whether a tool-specialized backbone improves strict KMP/MCP action
emission.

LFM2.5 Nova result: the model trained cleanly in 9m27s on one RTX 3090, using
about 7.8 GiB VRAM and reaching final eval loss 0.0589. Policy quality matched
Hammer but did not beat the Qwen v7 baseline: exact action accuracy reached
0.6500, with 46/60 parsed predictions, 14 strict prediction failures, and zero
unbounded tool calls. Stop accuracy was perfect. The valid action distribution
contained `kmp_ask`, `kmp_near`, and `stop`, but no valid
`kmp_inspect` predictions, which explains the remaining gap on smart-writer
read steps. The failure reasons were 8 unsupported action types, 2 incomplete
JSON responses, 2 extra-content-after-JSON responses, and 2 missing action
types. Keep this as a useful non-Qwen control, not a release candidate.

LFM2.5 Nova mixed-data control: the MemoryArena P1.11 + LongMemEval v7 mixed
training cut was rerun with 4 RTX 3090 GPUs using torchrun/DDP. The hardware
path is valid: the same effective batch size trained in about 29 minutes, with
all 4 GPUs saturated, about 13 GiB VRAM used per GPU, final eval loss 0.0590,
and eval token accuracy 0.9873. The policy result is not valid as an
improvement: only 42/60 predictions parsed, exact action accuracy dropped to
0.5167, and the model still produced zero valid `kmp_inspect` predictions.
All 18 strict failures were `longmemeval.smart_writer` steps. Failure analysis
showed the model copying large `writer_*` visible-state fragments into the
action JSON, producing incomplete or malformed actions. Do not continue this
mixed recipe. The next useful dataset work is to simplify and normalize
writer-mode visible state, keep internal writer labels out of model-facing
action arguments, and train/evaluate read and writer policies as explicit
modes instead of relying on raw mixed traces.

Hammer clean visible-state rerun: after removing exporter-only `sources`,
`visible_state.writer`, and internal `writer_*` provenance from model-facing
SFT data, `MadeAgents/Hammer2.0-0.5b` was rerun on the same LongMemEval v7
holdout using 4 RTX 3090 GPUs. Training completed in 5m12s with final
`eval_loss=0.05494` and `eval_mean_token_accuracy=0.9847`. Policy quality
improved materially versus the earlier Hammer v7 run: exact action accuracy
rose from 0.6500 to 0.7500, missing predictions fell from 7 to 1, and primary
ref accuracy rose from 0.7727 to 0.9545. This confirms the leak cleanup matters.
The model is still not a release candidate: it emits no valid `kmp_inspect`
predictions and 6/60 predictions are unbounded, mostly `kmp_near` calls
missing `limit.tokens`. The next gate is stronger action-shape supervision or
constrained emission for required bounded fields, then rerun Qwen/Hammer/LFM on
the same clean split.

FunctionGemma 270M clean visible-state rerun: `google/functiongemma-270m-it`
was rerun in native function-call mode on the same clean split. DDP support was
added to the FunctionGemma trainer so the run could use 4 RTX 3090 GPUs. The
first 4-GPU attempt with per-device batch size 4 failed with CUDA OOM; the
stable run used per-device batch size 1 and gradient accumulation 4. Training
completed in 4m15s with final `eval_loss=0.05393` and
`eval_mean_token_accuracy=0.9879`, but policy quality remained weak: only 33/60
predictions parsed, exact action accuracy reached 0.5000, and valid predicted
tools were limited to `kmp_ask` and `stop`. The main failure mode is native
function-call corruption: incomplete calls, copied function declarations, and
overlong `kernel_stop` calls with large `final_refs`. FunctionGemma remains a
useful small-model control, but not a viable Operator backbone in this setup.

Qwen 0.5B clean visible-state rerun: `Qwen/Qwen2.5-0.5B-Instruct` was rerun on
the same LongMemEval v8 clean split using 4 RTX 3090 GPUs. Training completed
in 5m03s with final `eval_loss=0.05537` and
`eval_mean_token_accuracy=0.9846`. Strict single-JSON inference produced 56/60
valid predictions: 4 predictions were rejected, with one each of
`incomplete_json`, `invalid_json`, `missing_action_type`, and `missing_tool`.
Policy quality improved versus the earlier Qwen v7 run: exact action accuracy
rose from 0.7000 to 0.7500, strict parse failures fell from 12 to 4, and stop
accuracy stayed at 1.000. The remaining dominant gap is not generic JSON
emission. It is tool semantics in smart-writer read steps: the held-out target
contains 14 `kmp_inspect` actions, but Qwen produced zero valid
`kmp_inspect` predictions, mapping 10 of them to `kmp_near` and missing
the other 4. One valid prediction was unbounded because it mixed a
`kmp_near` tool name with `kmp_ask`-shaped arguments, omitting
`around.ref` and `limit`. Qwen and Hammer now tie on exact action accuracy
(`0.7500`) on this clean split, but neither is a release candidate until the
training data or constrained emission teaches the inspect-vs-near boundary.

Prompt-only v9 negative result: adding a longer textual tool-selection policy
to the SFT system prompt did not solve the inspect-vs-near boundary. It made
Qwen more verbose and less stable under strict JSON generation: exact action
accuracy dropped to 0.5167, strict prediction failures rose to 7/60, and the
model still emitted zero valid `kmp_inspect` predictions. Keep the base
prompt compact. The problem is representation and action-shape learning, not
more prose.

Qwen 0.5B operator-state v10 result: the SFT preparer now adds a compact,
non-gold `visible_state.operator_state` derived only from visible state, and
reference anonymization now covers LongMemEval `question:run:`, `turn:run:`,
`about:`, and `evidence:` refs. This removes long raw refs from model-facing
state and exposes already-visible navigation facts such as
`navigation_phase=start` versus `after_near_with_observed_refs`, candidate
counts, observed-ref counts, and the primary visible candidate. On the same
LongMemEval holdout, Qwen trained in 3m18s with final `eval_loss=0.04027` and
`eval_mean_token_accuracy=0.9890`. The model produced the full expected action
distribution including all 14 `kmp_inspect` actions. A follow-up hardening
pass moved action-shape validation into the shared testkit contract and rejected
extra fields in tool arguments. Under that stricter evaluator, the same
prediction file has 59/60 valid predictions, 1 invalid prediction, zero
unbounded tool calls, 0.9773 tool accuracy, 0.9773 primary-ref accuracy, 0.9773
scope accuracy, 1.000 stop accuracy, and 0.9833 exact action accuracy. The
invalid row was a semantically correct `kmp_ask` that added `final_refs`
inside `arguments`, which is not part of the KMP/MCP tool schema. The 2026-05-14
strict predictor rerun now rejects that shape at parse time, writes 59
predictions and 1 failure, and policy eval reports 1 missing prediction, 0
invalid predictions, and 0 unbounded calls. This is the first strong evidence
that the small Operator path
works when KMP state is represented for tool operation instead of raw
transcript-like copying, while also showing why strict contract validation must
be part of the benchmark gate.

Update 2026-05-14: the predictor/evaluator contract issue is documented in
[`operator-action-contract-audit-2026-05-14.md`](operator-action-contract-audit-2026-05-14.md).
All Operator metrics produced before this hardening are `pre-strict` unless
they are revalidated with the shared action contract. This does not affect KMP
core, MCP/gRPC, persistence, temporal traversal, or multidimensional memory.
It affects Operator measurement and publication claims. The next mandatory
step is to rerun any model result we intend to cite publicly through the strict
predictor, strict policy evaluator, and live MCP replay gate.

Update 2026-05-14, revalidation sweep: MemoryArena V6 holdout20 was revalidated
with `kernel-operator-action-contract-v1` in both anonymized and de-anonymized
forms. Both remain clean: 1,124/1,124 exact actions, zero missing predictions,
zero invalid predictions, zero unbounded calls, and no invalid reasons.
LongMemEval v8 clean was also revalidated and is not release-grade under the
strict contract: 4 missing predictions, 2 invalid predictions, 0 unbounded
calls, and 0.7500 exact action accuracy. The invalid v8 reasons were one extra
`remaining_context_chars` field inside `kmp_ask.arguments` and one
`kmp_near` action missing required `around`, `include`, and `limit` fields.

Update 2026-05-14, benchmark boundary: the current benchmark status and next
steps are summarized in
[`operator-benchmark-status-and-next-steps-2026-05-14.md`](operator-benchmark-status-and-next-steps-2026-05-14.md).
The important distinction is that MemoryArena V6 validates Operator tool
operation over KMP/MCP, while LongMemEval multi-session remains a secondary
system-level regression for retrieval + reader + plugins + task reasoning.

## Goal

Build a small specialist model that operates the Kernel Memory Protocol tools
efficiently. The model is not the kernel, not the reader, and not the memory
reasoner. It is a tool operator that decides which bounded KMP/MCP move to call
next, with which arguments, and when to stop.

The first useful outcome is not model training. The first useful outcome is a
clean trajectory dataset and evaluator that make tool-use quality measurable.

## North Star

Keep the responsibility split simple and explicit:

```text
Operator 0.5B:
  only learns to use KMP.

Strong teacher:
  produces semantics when semantics are needed.

Kernel:
  validates, stores, traverses, proves, and audits memory.
```

The small Operator is not trained to understand memory as an author. It is
trained to operate the memory protocol:

- choose bounded KMP/MCP tools;
- choose scoped arguments;
- inspect, trace, page, move temporally, stop, or escalate;
- execute a prepared write only when the visible state already contains the
  write payload and relation proof.

The semantic decisions belong elsewhere. Rich writer relations, `why`, cited
evidence, derivations, and domain interpretation should be produced by a strong
teacher or another semantic layer. For writer datasets, the preferred teacher is
GPT-5.5, used offline with exact model provenance and strict structured output.

This prevents the project from drifting into a false goal: making a 0.5B model
act like a general memory reasoner. The product goal is narrower and more
measurable: a small model that uses KMP correctly.

## Boundary

The operator model may:

- choose one KMP/MCP tool call at a time;
- choose scoped, bounded arguments;
- decide whether to continue, inspect, trace, move temporally, or stop;
- execute prepared `kmp_write_memory` calls only when the relation, `why`,
  evidence, and read-context proof are already visible.

The operator model must not:

- bypass the public KMP/MCP/gRPC contract;
- become a hidden memory API;
- invent refs, evidence, relations, dimensions, or about scopes;
- author rich writer relations from scratch;
- decide semantic `why` for memory writes;
- request unbounded histories by default;
- own answer semantics for benchmark-specific tasks;
- become a dependency of kernel core.

Kernel responsibilities remain deterministic:

- traversal;
- proof;
- validation;
- fail-fast errors;
- audit;
- persistence;
- projection.

## Tools In Scope

Read/navigation tools:

- `kmp_ask`
- `kmp_near`
- `kmp_trace`
- `kmp_inspect`
- `kmp_goto`
- `kmp_rewind`
- `kmp_forward`

Write tool:

- `kmp_write_memory`

Write mode is a separate policy mode. The first operator slice should focus on
read/navigation trajectories. Writer mode must be added only after the
read loop is measurable, and it must remain scoped to kernel operation:
read-before-write, prepared-write execution, bounded calls, and escalation to a
strong semantic teacher when relation meaning is not already provided.

## Why This Is P1

The current MemoryArena and MCP runs show that the kernel can expose useful
memory, but generalist LLM tool use is inconsistent:

- it may overuse broad traversal;
- it may not choose the cheapest next move;
- it may stop late or stop with the wrong candidate;
- it may need more context than a kernel-native operator should need;
- its reasoning is hard to compare unless tool trajectories are normalized.

A small model can be viable because the task surface is narrow:

- state is structured;
- available actions are finite;
- valid arguments are constrained;
- success is directly measurable from refs, scopes, leaks, and stop decisions.

## Required Contract Before Training

Do not start model training until these are true:

- Top 1: `near`, `trace`, `goto`, `rewind`, `forward`, remote audit, and live
  replay expose bounded pagination/progress/resume behavior consistently
  through gRPC/KMP and MCP.
- MCP tool schemas are stable enough to serialize into the training prompt.
- Tool errors are fail-fast and explicit enough to be useful training labels.
- Raw logs are redacted: no API keys, credentials, private raw memory, or
  unbounded prompt dumps.
- Successful and failed trajectories can be separated deterministically.

Training on broken traces without labels is prohibited. Failed trajectories are
useful as hard negatives only after the failure reason is classified.

## Trajectory Schema

Each training/evaluation item should be one decision step, not one whole
benchmark task.

Minimal shape:

```json
{
  "schema_version": "kernel-operator-trajectory-v1",
  "run_id": "memoryarena-smart-writer-paged-50tasks-20260508-0009",
  "task_family": "memoryarena.progressive_search",
  "mode": "read",
  "about": "memoryarena:run:...",
  "step_index": 4,
  "goal": "Find the next bounded evidence move for the current question.",
  "visible_state": {
    "current_ref": "memoryarena:...:subtask:9:answer",
    "known_refs": ["..."],
    "last_tool": "kmp_near",
    "last_observed_refs": ["..."],
    "remaining_budget": {
      "tool_calls": 4,
      "context_chars": 12000
    }
  },
  "allowed_tools": [
    "kmp_near",
    "kmp_trace",
    "kmp_inspect",
    "kmp_goto",
    "kmp_rewind",
    "kmp_forward",
    "kmp_ask"
  ],
  "target_action": {
    "type": "tool_call",
    "tool": "kmp_inspect",
    "arguments": {
      "about": "memoryarena:run:...",
      "ref": "memoryarena:...:subtask:9:answer",
      "include": {
        "details": true,
        "relationships": true,
        "raw": false
      }
    }
  },
  "observed_outcome": {
    "success": true,
    "observed_refs": ["..."],
    "elapsed_ms": 46
  },
  "quality": {
    "known_at_clean": true,
    "future_answer_leak": false,
    "invalid_tool_call": false,
    "bounded": true,
    "stop_correct": null
  }
}
```

Stop action shape:

```json
{
  "target_action": {
    "type": "stop",
    "answer_policy": "evidence_or_unknown",
    "final_refs": ["..."],
    "reason": "sufficient_evidence"
  }
}
```

The dataset should preserve the raw tool result path separately for audit, but
the model-facing item should be compact and redacted.

## Initial Data Sources

Use already audited runs first:

- MemoryArena 100-task smart-writer run:
  `/tmp/memoryarena-smart-writer-paged-100tasks-20260508-1407-run`
- MemoryArena 100-task stderr/log digest:
  `/tmp/memoryarena-smart-writer-paged-100tasks-20260508-1407.stderr`
- MemoryArena 100-task scorecard:
  `/tmp/memoryarena-smart-writer-paged-100tasks-20260508-1407-scorecard`
- MemoryArena 100-task audit:
  `/tmp/memoryarena-smart-writer-paged-100tasks-20260508-1407-audit.json`
- MemoryArena 50-task smart-writer run:
  `/tmp/memoryarena-smart-writer-paged-50tasks-20260508-0009-run`
- MemoryArena 50-task stderr/log digest:
  `/tmp/memoryarena-smart-writer-paged-50tasks-20260508-0009.stderr`
- MemoryArena 50-task scorecard:
  `/tmp/memoryarena-smart-writer-paged-50tasks-20260508-0009-scorecard`
- MemoryArena prepared 221-task artifacts:
  `/tmp/memoryarena-progressive-221-cost-estimate-artifacts`
- Live MCP story demo docs:
  `archive/docs/research/kernel-memory-story-demo-2026-05-05.md`
- Live mobile incident MCP demo docs:
  `archive/docs/research/mobile-login-resolution-replay-demo-2026-05-05.md`

The 100-task MemoryArena run is the best first corpus because it includes real
MCP navigation, smart-writer read-before-write decisions, relation quality
metrics, known-at checks, bounded trace proofs, and three final reader failures
where evidence recall was complete. Those failures prove the operator must not
be evaluated only by final answer correctness.

## P1 Work Packages

### P1.1 Trajectory Exporter

Add a testkit exporter that converts existing runner logs and artifacts into
`kernel-operator-trajectory-v1`.

Proposed binary:

```text
kernel_operator_trajectory_export
```

Implemented binary:

```text
cargo run -p kmp-testkit --bin kernel_operator_trajectory_export -- \
  --run <memoryarena-run-dir> \
  --output <output-dir> \
  [--include-writer-reads] \
  [--expected-run-id <id>] \
  [--force]
```

Inputs:

- MemoryArena runner output directory;
- MemoryArena scorecard directory;
- optional stderr JSONL log with MCP read/write events;
- optional run id filter;
- optional task id filter.

Outputs:

- `trajectories.jsonl`;
- `summary.json`;
- `failures.jsonl`;
- `redaction_report.json`.

Exit criteria:

- [x] exporter is deterministic;
- [x] exporter fails fast on mixed runs;
- [x] no secret-looking values are emitted;
- [x] every target tool call has bounded arguments;
- [x] failed and successful steps are labelled separately.

Implemented outputs:

- `trajectories.jsonl`
- `summary.json`
- `failures.jsonl`
- `redaction_report.json`

The model-facing trajectory intentionally excludes benchmark gold answers,
`ask_answer`, raw `ask_content`, API keys, credentials, and raw prompt dumps.
Each item is a single decision step:

```text
visible_state -> target_action
```

The first exported action space is read/navigation:

- `kmp_near`
- `kmp_inspect`
- `kmp_trace`
- `stop`

`--include-writer-reads` additionally exports read-before-write tool calls from
the smart writer as `write_context_read` mode, but it does not yet train the
model to author `kmp_write_memory` relations. Writer-side training remains
limited to kernel operation until a strong-teacher semantic writer dataset
exists.

Real export checks:

| Source run | Mode | Trajectories | Tool calls | Stops | Failures | Redaction findings |
| --- | --- | ---: | ---: | ---: | ---: | ---: |
| MemoryArena 50-task smart-writer run | read | 1,426 | 1,057 | 369 | 0 | 0 |
| MemoryArena 50-task smart-writer run | read + writer reads | 2,802 | 2,433 | 369 | 0 | 0 |
| MemoryArena 100-task smart-writer run | read | 2,912 | 2,159 | 753 | 0 | 0 |
| MemoryArena 100-task smart-writer run | read + writer reads with candidates | 5,724 | 4,971 | 753 | 0 | 0 |

The first 100-task read-only export produced:

```text
kmp_near    753
kmp_inspect 753
kmp_trace   653
stop           753
```

This gives enough clean data to implement P1.2 without touching kernel core.

### P1.2 Operator Evaluator

Add an offline evaluator that scores a predicted tool action against the
recorded target action and final trajectory quality.

Implemented binary:

```text
cargo run -p kmp-testkit --bin kernel_operator_policy_eval -- \
  --trajectories <trajectories.jsonl> \
  [--predictions <predictions.jsonl>] \
  [--baseline deterministic|oracle] \
  [--output <summary.json>]
```

Prediction rows may use either:

```json
{"step_id": "...", "action": {"type": "tool_call", "tool": "...", "arguments": {}}}
```

or:

```json
{"step_id": "...", "target_action": {"type": "stop", "reason": "..."}}
```

Metrics:

- tool selection accuracy;
- argument validity;
- scope correctness;
- ref correctness;
- bounded/paginated request correctness;
- invalid tool call rate;
- future leak rate;
- known-at clean rate;
- stop accuracy;
- excess tool calls;
- elapsed latency by tool;
- answer-independent trajectory success.

Exit criteria:

- [x] a generalist LLM baseline and a deterministic baseline can be compared;
- [x] evaluation does not require access to expected benchmark answers;
- [ ] MemoryArena task `11` is classified as a reader/candidate failure, not an
  operator failure, when navigation/proof is clean.

Current evaluator metrics:

- missing predictions;
- invalid predictions;
- unbounded tool calls;
- action type accuracy;
- tool accuracy over target tool calls;
- primary ref accuracy over target tool calls;
- scope accuracy over target tool calls;
- stop accuracy over target stop actions;
- exact action accuracy.

Real deterministic baseline checks:

| Source trajectories | Tool accuracy | Ref accuracy | Scope accuracy | Stop accuracy | Invalid | Unbounded |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| MemoryArena 50-task read trajectories | 1.000 | 1.000 | 1.000 | 1.000 | 0 | 0 |
| MemoryArena 100-task read trajectories | 1.000 | 1.000 | 1.000 | 1.000 | 0 | 0 |

The exact JSON action score is lower, about `0.517`, because the deterministic
baseline intentionally reconstructs equivalent bounded actions and stops rather
than copying auxiliary `goal` text or exact `final_refs` from the recorded
target. For model comparison, tool/ref/scope/stop validity is the primary P1
signal.

### P1.3 Deterministic Baseline

Implement a simple rule baseline before training:

- inspect the current anchor;
- near around the latest known prior ref;
- trace only when there is a target ref and a bounded path need;
- stop when proof refs cover the current ask and no future refs appear.

This baseline is intentionally modest. It gives the operator model something
concrete to beat.

Implemented inside `kernel_operator_policy_eval` as `--baseline deterministic`.
It is not a trained model. It exists to provide a stable floor for future
generalist and small-model comparisons.

### P1.4 Generalist LLM Baseline

Replay the same trajectory items with a generalist model that emits only one
of:

- a JSON tool call;
- a JSON stop action.

The prompt must include:

- compact tool schema;
- explicit bounds;
- current visible state;
- no benchmark gold answer;
- no raw hidden memory.

This baseline measures whether a small operator can reduce tool calls, context
usage, invalid calls, and latency while preserving ref quality.

Implemented binary:

```text
cargo run -p kmp-testkit --bin kernel_operator_llm_baseline -- \
  --trajectories <trajectory-dir>/trajectories.jsonl \
  --output <output-dir> \
  --endpoint <openai-compatible-chat-completions-url> \
  --model <model> \
  [--provider openai|openai-new|anthropic] \
  [--api-key-env LLM_API_KEY] \
  [--limit n] \
  [--offset n] \
  [--max-refs n] \
  [--force]
```

Outputs:

- `predictions.jsonl`: clean evaluator input with only `step_id` and `action`;
- `llm_results.jsonl`: raw model response, token counts, latency, and parsed
  action for audit;
- `failures.jsonl`: LLM call failures or rejected actions;
- `summary.json`: selected trajectory count, prediction count, invalid count,
  boundedness count, token usage, latency, and action distribution.

Guardrails:

- no `target_action`, `observed_outcome`, benchmark gold answer, or hidden raw
  memory is included in the prompt;
- visible refs are capped with `--max-refs`;
- rejected actions do not enter `predictions.jsonl`;
- tool calls must be bounded;
- tool names must be present in the trajectory action space;
- tool refs and stop refs must already be visible in `current_ref`,
  `trace_target_ref`, `known_refs`, or `last_observed_refs`;
- `kmp_inspect.include.raw` must be `false`.

Evaluation command after a run:

```text
cargo run -p kmp-testkit --bin kernel_operator_policy_eval -- \
  --trajectories <trajectory-dir>/trajectories.jsonl \
  --predictions <llm-baseline-dir>/predictions.jsonl \
  --output <llm-baseline-dir>/policy-eval.json \
  [--limit n] \
  [--offset n]
```

Real OpenAI baseline smoke:

| Model | Slice | Prompt | Predictions | Failures | Exact | Tool | Ref | Scope | Stop | Invalid | Unbounded |
| --- | ---: | --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| `gpt-4o-mini` | first 30 trajectories | initial | 23 | 7 | 0.300 | 0.696 | 0.696 | 0.696 | 0.857 | 0 | 0 |
| `gpt-4o-mini` | first 30 trajectories | explicit `about` rule | 30 | 0 | 0.533 | 1.000 | 1.000 | 1.000 | 1.000 | 0 | 0 |

The initial failure mode was useful: the generalist model used `current_ref` as
`kmp_near.arguments.about` instead of the top-level `about`. The prompt and
training data now state explicitly that `about` must equal the top-level scope.

### P1.5 First Small-Model Experiment

Only after P1.1-P1.4:

- choose a small function-calling or instruction model;
- train or fine-tune on exported trajectories;
- evaluate on held-out runs and task families;
- compare to deterministic and generalist baselines.

No model should be added to kernel core. It runs as a sidecar/client.

Implemented local training path:

```text
scripts/operator/prepare_operator_sft_dataset.py
scripts/operator/train_operator_sft_lora.py
scripts/operator/predict_operator_sft.py
scripts/operator/README.md
```

The first SFT dataset has been prepared from the 100-task trajectory export:

```text
python scripts/operator/prepare_operator_sft_dataset.py \
  --trajectories /tmp/kernel-operator-trajectories-100/trajectories.jsonl \
  --output /tmp/kernel-operator-sft-100 \
  --eval-ratio 0.1 \
  --seed 42 \
  --force
```

Result:

| Source | Selected | Train | Eval | Near | Inspect | Trace | Stop |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| MemoryArena 100-task read trajectories | 2,912 | 2,621 | 291 | 753 | 753 | 653 | 753 |

Leak check:

```text
rg -n 'target_action|observed_outcome|quality|gold|answer_session|has_answer' \
  /tmp/kernel-operator-sft-100/train.jsonl \
  /tmp/kernel-operator-sft-100/eval.jsonl
```

returned no matches. The target action exists only in the assistant message,
not in the model-facing user prompt.

Recommended first model:

```text
Qwen/Qwen2.5-0.5B-Instruct
```

It is small enough for fast iteration and good enough to validate whether the
task is learnable before spending time on 1.5B/3B/7B variants.

First local training command:

```text
python scripts/operator/train_operator_sft_lora.py \
  --train-jsonl /tmp/kernel-operator-sft-100/train.jsonl \
  --eval-jsonl /tmp/kernel-operator-sft-100/eval.jsonl \
  --model-id Qwen/Qwen2.5-0.5B-Instruct \
  --output-dir /tmp/kernel-operator-qwen05-lora \
  --epochs 3 \
  --batch-size 2 \
  --grad-accum 8 \
  --max-length 2048 \
  --bf16
```

Use `--fp16` instead of `--bf16` if the active GPU does not support bfloat16.

Current execution blocker in the Codex shell:

- `nvidia-smi` cannot communicate with the NVIDIA driver;
- Python training dependencies are not installed in this environment
  (`torch`, `transformers`, `trl`, `peft`);
- OpenAI accepted file upload, but fine-tuning is unavailable for the current
  organization (`training_not_available`).

These are environment issues, not dataset or code issues.

First local Kubernetes training run:

```text
kubectl apply -f archive/k8s/kernel-operator-qwen05-lora-job.yaml
```

Training environment:

- model: `Qwen/Qwen2.5-0.5B-Instruct`;
- method: LoRA SFT;
- GPU: 1x RTX 3090 requested through `nvidia.com/gpu: 1`;
- train rows: 2,621;
- eval rows: 291;
- epochs: 3;
- wall time: 58 minutes.

Final training metrics:

| Metric | Value |
| --- | ---: |
| train_loss | 0.05115 |
| eval_loss | 0.008344 |
| eval_mean_token_accuracy | 0.9965 |
| eval_samples_per_second | 6.479 |

Prediction run:

```text
kubectl apply -f archive/k8s/kernel-operator-qwen05-predict-job.yaml
```

Prediction output:

```text
/tmp/kernel-operator-qwen05-predictions
```

Policy evaluation:

```text
cargo run -p kmp-testkit --bin kernel_operator_policy_eval -- \
  --trajectories /tmp/kernel-operator-sft-100/eval_trajectories.jsonl \
  --predictions /tmp/kernel-operator-qwen05-predictions/predictions.jsonl \
  --output /tmp/kernel-operator-qwen05-policy-eval.json
```

Held-out eval result:

| Predictor | Total | Exact | Tool | Ref | Scope | Stop | Invalid | Unbounded |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| Qwen 0.5B LoRA operator | 291 | 1.000 | 1.000 | 1.000 | 1.000 | 1.000 | 0 | 0 |
| Deterministic baseline | 291 | 0.515 | 1.000 | 1.000 | 1.000 | 1.000 | 0 | 0 |

Interpretation:

- the first operator model learned the current read-navigation policy exactly
  on the held-out split;
- the task is learnable by a very small model;
- this does not yet prove generalization beyond the MemoryArena
  progressive-search trajectory distribution;
- next held-out cuts should split by task id/run family, not only ratio-based
  trajectory rows.

### P1.5b Ref-Safe Dataset Hardening

The first LoRA result was useful, but the row split was too easy and the raw
refs were not acceptable for a serious training claim.

The grouped V2 training attempt was stopped intentionally after this was found.
V2 should not be used as a reported result.

Raw MemoryArena refs encode more than identity:

```text
...:task:82:subtask:5:question
...:task:82:subtask:5:answer
```

That naming can leak role, temporal position, and benchmark structure. Even if
the model never sees benchmark answers, it could learn to choose refs from the
shape of the string rather than from KMP visible state. That is not the model
we want.

The preparer now supports two hardening flags:

```text
--anonymize-refs
--require-visible-target-refs
```

`--anonymize-refs` rewrites the model-facing trajectory with stable synthetic
refs per decision step:

```text
about/current_ref/known_refs/target_action refs -> ref_0001, ref_0002, ...
```

The original trajectories are still written for audit:

```text
train_trajectories.jsonl
eval_trajectories.jsonl
all_trajectories.jsonl
```

The anonymized trajectories are written for model evaluation:

```text
train_model_trajectories.jsonl
eval_model_trajectories.jsonl
all_model_trajectories.jsonl
```

Predictions from the anonymized dataset must be evaluated against
`eval_model_trajectories.jsonl`, not against the raw audit trajectories.

`--require-visible-target-refs` drops any row whose target action refers to a
ref that is not present in:

```text
current_ref
trace_target_ref
candidate_refs
known_refs
last_observed_refs
```

This matters because the first writer-read export exposed a real issue: some
read-before-write `kmp_near` targets were only recoverable from the raw ref
naming pattern, not from visible state. Those rows are not valid training
examples for an operator model.

Strict dataset command:

```text
python scripts/operator/prepare_operator_sft_dataset.py \
  --trajectories /tmp/kernel-operator-trajectories-100-with-writer/trajectories.jsonl \
  --output /tmp/kernel-operator-sft-100-with-writer-by-task-anon-visible \
  --split-mode group \
  --group-key task_id \
  --eval-ratio 0.1 \
  --seed 42 \
  --anonymize-refs \
  --require-visible-target-refs \
  --force
```

Real output:

| Dataset | Source rows | Kept | Dropped non-visible refs | Train | Eval | Train groups | Eval groups |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| 100-task read + writer reads, grouped by task, anonymized refs | 5,724 | 4,318 | 1,406 | 3,854 | 464 | 89 | 11 |

Kept rows by mode:

| Mode | Rows |
| --- | ---: |
| `read` | 2,912 |
| `write_context_read` | 1,406 |

Kept target actions:

| Target action | Rows |
| --- | ---: |
| `kmp_near` | 753 |
| `kmp_inspect` | 2,159 |
| `kmp_trace` | 653 |
| `stop` | 753 |

The dropped rows are currently the first writer context-read step where the
target ref is not yet visible. That means the next writer exporter improvement
is clear: if a writer is allowed to inspect a candidate, the candidate must be
explicitly present in visible state. Otherwise the row is not trainable without
leaking naming conventions.

Leak audit for model-facing messages:

```text
jq -c 'select((.messages|map(.content)|join("\n")|test("memoryarena:|:question|:answer|:subtask:|:task:|target_action|observed_outcome|quality|gold|answer_session|has_answer")))' \
  /tmp/kernel-operator-sft-100-with-writer-by-task-anon-visible/train.jsonl \
  /tmp/kernel-operator-sft-100-with-writer-by-task-anon-visible/eval.jsonl \
  /tmp/kernel-operator-sft-100-with-writer-by-task-anon-visible/openai_train.jsonl \
  /tmp/kernel-operator-sft-100-with-writer-by-task-anon-visible/openai_eval.jsonl
```

returned no model-facing matches.

The local trainer should use the messages-only files for training:

```text
openai_train.jsonl
openai_eval.jsonl
```

The prediction script still uses `eval.jsonl`, because it needs `step_id` to
write evaluator-compatible predictions. `step_id` is metadata for the runner,
not part of the prompt.

Evaluator sanity checks on the anonymized eval set:

| Predictor | Total | Exact | Tool | Ref | Scope | Stop | Invalid | Unbounded |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| Oracle baseline | 464 | 1.000 | 1.000 | 1.000 | 1.000 | 1.000 | 0 | 0 |
| Deterministic baseline | 464 | 0.349 | 1.000 | 0.606 | 1.000 | 1.000 | 0 | 0 |

The deterministic baseline losing ref accuracy after anonymization is expected
and useful: it shows that simple rules can no longer benefit from raw ref
naming. The next trained operator should beat this baseline by using visible
state, not benchmark-shaped ids.

Next V3 training rule:

- train on `openai_train.jsonl` / `openai_eval.jsonl`;
- predict on `eval.jsonl`;
- evaluate predictions against `eval_model_trajectories.jsonl`;
- keep `eval_trajectories.jsonl` only for audit against the original kernel
  refs.

V3 ref-safe trained operator result:

| Item | Value |
| --- | --- |
| Base model | `Qwen/Qwen2.5-0.5B-Instruct` |
| Adapter | `/tmp/kernel-operator-qwen05-lora-v3` |
| Kubernetes train job | `kernel-operator-qwen05-lora-v3` |
| Kubernetes predict job | `kernel-operator-qwen05-predict-v3` |
| Train duration | 25m Kubernetes job duration; 1,476s trainer runtime |
| Predict duration | 3m24s Kubernetes job duration with `--batch-size 8` |
| Train rows | 3,854 |
| Eval rows | 464 |
| Prediction failures | 0 |

Training metrics:

| Metric | Value |
| --- | ---: |
| `train_loss` | 0.06846 |
| `eval_loss` | 0.01203 |
| `eval_mean_token_accuracy` | 0.995 |
| `train_samples_per_second` | 7.834 |
| `train_steps_per_second` | 0.49 |

Held-out ref-safe eval result:

| Predictor | Total | Exact | Tool | Ref | Scope | Stop | Invalid | Unbounded |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| Oracle baseline | 464 | 1.000 | 1.000 | 1.000 | 1.000 | 1.000 | 0 | 0 |
| Deterministic baseline | 464 | 0.349 | 1.000 | 0.606 | 1.000 | 1.000 | 0 | 0 |
| Qwen 0.5B LoRA V3 | 464 | 0.996 | 1.000 | 0.995 | 1.000 | 1.000 | 0 | 0 |

The two V3 misses are both in `write_context_read`. In both cases the model
selected the correct tool, bounded scope, and action type, but chose a different
visible `kmp_inspect` ref from the same candidate set. There were no missing
predictions, invalid JSON actions, unbounded tool calls, wrong tools, wrong
scopes, or wrong stop actions.

This is the first credible operator-model result because:

- benchmark-shaped raw refs are not visible to the prompt;
- train/eval is grouped by task;
- target refs must be visible to the model;
- evaluation is against anonymized model trajectories, not the raw audit
  trajectories.

Remaining gaps after V3:

- batched prediction is now acceptable for offline replay, but still not a live
  serving path: 464 rows took 3m24s with `--batch-size 8`, including dependency
  installation and model load;
- 1,406 writer-read rows were intentionally dropped because the target ref was
  not visible to the prompt; the exporter must expose valid candidates rather
  than relying on raw ref shape;
- the result is held out by task, but still from one 100-task MemoryArena run;
  the next credible step is a larger run and a family-held-out split;
- live MCP validation is still separate from offline policy replay;
- writer relation proposals should remain disabled until the read operator is
  validated at larger scale.

### P1.6 Candidate-Visible Writer Hardening

The V3 result was credible, but it exposed a dataset construction gap: 1,406
writer-read rows were dropped because their target refs were valid candidates
for the writer but were not present in the model-facing visible state. That is
not acceptable for a training claim. If the operator is allowed to choose a ref,
that ref must be visible without relying on raw naming conventions.

The exporter now adds `visible_state.candidate_refs` for writer context-read
steps. Candidate refs are gathered from:

- writer read context inspected refs;
- writer read context temporal refs;
- planned relation targets;
- relation-quality targets;
- primary refs already present in the recorded pre-read tool calls.

The current entry ref is excluded. The preparer now treats `candidate_refs` as
part of the visible ref set for strict target-ref validation and prompt
compaction.

Candidate-visible export:

```text
cargo run -p kmp-testkit --bin kernel_operator_trajectory_export -- \
  --run /tmp/memoryarena-smart-writer-paged-100tasks-20260508-1407-run \
  --output /tmp/kernel-operator-trajectories-100-with-writer-candidates \
  --include-writer-reads \
  --expected-run-id smart-writer-paged-100tasks-20260508-1407 \
  --force
```

Strict candidate-visible dataset:

```text
python scripts/operator/prepare_operator_sft_dataset.py \
  --trajectories /tmp/kernel-operator-trajectories-100-with-writer-candidates/trajectories.jsonl \
  --output /tmp/kernel-operator-sft-100-with-writer-by-task-anon-visible-candidates \
  --split-mode group \
  --group-key task_id \
  --eval-ratio 0.1 \
  --seed 42 \
  --anonymize-refs \
  --require-visible-target-refs \
  --force
```

Real output:

| Dataset | Source rows | Kept | Dropped non-visible refs | Train | Eval | Train groups | Eval groups |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| 100-task read + writer reads, grouped by task, anonymized refs, candidate-visible | 5,724 | 5,724 | 0 | 5,109 | 615 | 89 | 11 |

Rows by mode:

| Mode | Rows |
| --- | ---: |
| `read` | 2,912 |
| `write_context_read` | 2,812 |

Target actions:

| Target action | Rows |
| --- | ---: |
| `kmp_near` | 2,159 |
| `kmp_inspect` | 2,159 |
| `kmp_trace` | 653 |
| `stop` | 753 |

Eval-set baselines:

| Predictor | Total | Exact | Tool | Ref | Scope | Stop | Invalid | Unbounded |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| Oracle baseline | 615 | 1.000 | 1.000 | 1.000 | 1.000 | 1.000 | 0 | 0 |
| Deterministic baseline | 615 | 0.263 | 1.000 | 0.434 | 1.000 | 1.000 | 0 | 0 |

V4 trained operator result:

| Item | Value |
| --- | --- |
| Base model | `Qwen/Qwen2.5-0.5B-Instruct` |
| Adapter | `/tmp/kernel-operator-qwen05-lora-v4` |
| Kubernetes train job | `kernel-operator-qwen05-lora-v4` |
| Kubernetes predict job | `kernel-operator-qwen05-predict-v4` |
| Train duration | 32m Kubernetes job duration; 1,930s trainer runtime |
| Predict duration | 5m14s Kubernetes job duration with `--batch-size 8` |
| Train rows | 5,109 |
| Eval rows | 615 |
| Prediction failures | 0 |

Training metrics:

| Metric | Value |
| --- | ---: |
| `train_loss` | 0.05677 |
| `eval_loss` | 0.01068 |
| `eval_mean_token_accuracy` | 0.9952 |
| `train_samples_per_second` | 7.942 |
| `train_steps_per_second` | 0.497 |

Held-out candidate-visible eval result:

| Predictor | Total | Exact | Tool | Ref | Scope | Stop | Invalid | Unbounded |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| Oracle baseline | 615 | 1.000 | 1.000 | 1.000 | 1.000 | 1.000 | 0 | 0 |
| Deterministic baseline | 615 | 0.263 | 1.000 | 0.434 | 1.000 | 1.000 | 0 | 0 |
| Qwen 0.5B LoRA V4 | 615 | 0.993 | 1.000 | 0.993 | 1.000 | 1.000 | 0 | 0 |

The four V4 misses are all in `write_context_read` for the same MemoryArena
task family slice. They use the correct action type, tool, bounded arguments,
scope, and stop behavior, but choose a different visible candidate ref. This is
not a leak or boundedness issue. It points to the next dataset/modeling gap:
candidate refs are visible, but still unranked and mostly untyped. A stronger
writer-operator dataset should expose candidate roles, relation intent, or
writer evidence summaries rather than only a flat candidate ref list.

### P1.7 Writer Candidate Details

The next hardening step is to make writer candidates understandable without
leaking the writer's final relation decision.

The exporter now adds `visible_state.candidate_ref_details` beside
`candidate_refs`. Each detail is deliberately structural:

```json
{
  "ref": "ref_0002",
  "role": "same_subtask_question",
  "turn_kind": "question",
  "relative_position": "same_subtask",
  "temporal_distance": 0,
  "priority": 10,
  "relation_hint": "answer_addresses_question"
}
```

What is intentionally not included:

- final `connect_to.rel`;
- final relation `why`;
- final relation evidence;
- raw memory text;
- benchmark answer labels;
- exporter source names such as writer candidate provenance or recorded action
  provenance.

This matters because a writer operator should learn how to choose between
visible candidates, not memorize the post-hoc relation emitted by the writer.
The useful signal is structural: turn kind, relative temporal position,
candidate role, relation hint, and bounded priority.

This is now a cross-benchmark rule. Benchmark exporters may keep provenance
metadata in audit trajectories, but model-facing SFT data must not expose
internal exporter sources, hidden writer labels, final relation decisions, gold
answers, or raw memory that a real KMP/MCP operator would not see at inference
time. The SFT preparer sanitizes legacy trajectories and fails fast if those
fields reach `visible_state`.

Candidate-detail export:

```text
cargo run -p kmp-testkit --bin kernel_operator_trajectory_export -- \
  --run /tmp/memoryarena-smart-writer-paged-100tasks-20260508-1407-run \
  --output /tmp/kernel-operator-trajectories-100-with-writer-candidate-details \
  --include-writer-reads \
  --expected-run-id smart-writer-paged-100tasks-20260508-1407 \
  --force
```

Strict candidate-detail dataset:

```text
python scripts/operator/prepare_operator_sft_dataset.py \
  --trajectories /tmp/kernel-operator-trajectories-100-with-writer-candidate-details/trajectories.jsonl \
  --output /tmp/kernel-operator-sft-100-with-writer-by-task-anon-visible-candidate-details \
  --split-mode group \
  --group-key task_id \
  --eval-ratio 0.1 \
  --seed 42 \
  --anonymize-refs \
  --require-visible-target-refs \
  --force
```

Real output:

| Dataset | Source rows | Kept | Dropped non-visible refs | Train | Eval | Train groups | Eval groups |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| 100-task read + writer reads, grouped by task, anonymized refs, candidate details | 5,724 | 5,724 | 0 | 5,109 | 615 | 89 | 11 |

Leak audit returned no model-facing rows for raw MemoryArena refs, target
actions, observed outcomes, quality fields, benchmark gold labels, answer
session ids, or `has_answer`.

The V5 training run uses the same base model and split as V4, but with
candidate details in the model-facing visible state:

| Item | Value |
| --- | --- |
| Base model | `Qwen/Qwen2.5-0.5B-Instruct` |
| Adapter | `/tmp/kernel-operator-qwen05-lora-v5` |
| Kubernetes train job | `kernel-operator-qwen05-lora-v5` |
| Kubernetes predict job | `kernel-operator-qwen05-predict-v5` |
| Train duration | 35m27s Kubernetes job duration; 2,092s trainer runtime |
| Predict duration | 4m55s Kubernetes job duration with `--batch-size 8` |
| Train rows | 5,109 |
| Eval rows | 615 |
| Prediction failures | 0 |

Training metrics:

| Metric | Value |
| --- | ---: |
| `train_loss` | 0.05612 |
| `eval_loss` | 0.00966 |
| `eval_mean_token_accuracy` | 0.9957 |
| `train_samples_per_second` | 7.326 |
| `train_steps_per_second` | 0.459 |

Held-out candidate-detail eval result:

| Predictor | Total | Exact | Tool | Ref | Scope | Stop | Invalid | Unbounded |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| Oracle baseline | 615 | 1.000 | 1.000 | 1.000 | 1.000 | 1.000 | 0 | 0 |
| Deterministic baseline | 615 | 0.263 | 1.000 | 0.434 | 1.000 | 1.000 | 0 | 0 |
| Qwen 0.5B LoRA V5 | 615 | 1.000 | 1.000 | 1.000 | 1.000 | 1.000 | 0 | 0 |

V5 closed the four V4 writer-context-read misses. The important change is not
more hidden answer signal; it is better visible structure around the candidate
refs. The model sees candidate role, turn kind, relative temporal position,
priority, and a relation hint, while the final relation decision and evidence
remain hidden.

### P1.8 Explicit Holdout Validation

V5 proved that candidate details closed the remaining writer-context-read
misses, but the eval split was still produced by a seeded stable-hash ratio
over task groups. P1.8 makes the holdout explicit and larger so the result is
easier to repeat and audit.

The SFT preparer now supports explicit group holdouts:

```text
--eval-group-values
--eval-group-values-file
```

It also supports additional group keys:

```text
task_id
task_type
task_family
mode
about
run_id
```

The V6 validation reserved task ids `80` through `99` for evaluation:

```text
python scripts/operator/prepare_operator_sft_dataset.py \
  --trajectories /tmp/kernel-operator-trajectories-100-with-writer-candidate-details/trajectories.jsonl \
  --output /tmp/kernel-operator-sft-100-with-writer-by-task-anon-visible-candidate-details-holdout20 \
  --split-mode group \
  --group-key task_id \
  --eval-group-values-file /tmp/kernel-operator-holdout20-groups.txt \
  --anonymize-refs \
  --require-visible-target-refs \
  --force
```

Real output:

| Dataset | Source rows | Train | Eval | Train groups | Eval groups | Dropped non-visible refs | Eval group values |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | --- |
| 100-task read + writer reads, grouped by explicit task holdout, anonymized refs, candidate details | 5,724 | 4,600 | 1,124 | 80 | 20 | 0 | `80`-`99` |

Eval-set composition:

| Mode | Rows |
| --- | ---: |
| `read` | 572 |
| `write_context_read` | 552 |

| Target action | Rows |
| --- | ---: |
| `kmp_near` | 424 |
| `kmp_inspect` | 424 |
| `kmp_trace` | 128 |
| `stop` | 148 |

Leak audit returned no model-facing rows for raw MemoryArena refs, target
actions, observed outcomes, quality fields, benchmark gold labels, answer
session ids, or `has_answer`.

Eval-set baselines:

| Predictor | Total | Exact | Tool | Ref | Scope | Stop | Invalid | Unbounded |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| Oracle baseline | 1,124 | 1.000 | 1.000 | 1.000 | 1.000 | 1.000 | 0 | 0 |
| Deterministic baseline | 1,124 | 0.263 | 1.000 | 0.434 | 1.000 | 1.000 | 0 | 0 |

V6 explicit-holdout trained operator result:

| Item | Value |
| --- | --- |
| Base model | `Qwen/Qwen2.5-0.5B-Instruct` |
| Adapter | `/tmp/kernel-operator-qwen05-lora-v6-holdout20` |
| Kubernetes train job | `kernel-operator-qwen05-lora-v6-holdout20` |
| Kubernetes predict job | `kernel-operator-qwen05-predict-v6-holdout20` |
| Train duration | 33m01s Kubernetes job duration; 1,946s trainer runtime |
| Predict duration | 8m50s Kubernetes job duration with `--batch-size 8` |
| Train rows | 4,600 |
| Eval rows | 1,124 |
| Prediction failures | 0 |

Training metrics:

| Metric | Value |
| --- | ---: |
| `train_loss` | 0.0588 |
| `eval_loss` | 0.01425 |
| `eval_mean_token_accuracy` | 0.9954 |
| `train_samples_per_second` | 7.092 |
| `train_steps_per_second` | 0.444 |

Held-out explicit-task eval result:

| Predictor | Total | Exact | Tool | Ref | Scope | Stop | Invalid | Unbounded |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| Oracle baseline | 1,124 | 1.000 | 1.000 | 1.000 | 1.000 | 1.000 | 0 | 0 |
| Deterministic baseline | 1,124 | 0.263 | 1.000 | 0.434 | 1.000 | 1.000 | 0 | 0 |
| Qwen 0.5B LoRA V6 holdout20 | 1,124 | 1.000 | 1.000 | 1.000 | 1.000 | 1.000 | 0 | 0 |

This does not prove cross-benchmark generalization or live MCP serving
robustness. It does prove that the operator is not only memorizing the smaller
V5 split: with benchmark-shaped refs hidden, it generalizes across a larger
explicit held-out task range inside the same MemoryArena run.

### P1.9 Raw-Ref Replay Bridge

The V6 result is intentionally evaluated on anonymized model trajectories. That
is the right training/eval boundary because the model must not learn from raw
MemoryArena ref shape. But it creates a practical follow-up: a live MCP replay
cannot execute `ref_0001`.

P1.9 adds an explicit de-anonymization bridge outside kernel core:

```text
scripts/operator/deanonymize_operator_predictions.py
```

Inputs:

- raw audit trajectories: `eval_trajectories.jsonl`;
- anonymized model trajectories: `eval_model_trajectories.jsonl`;
- model predictions: `predictions.jsonl`.

Outputs:

- raw-ref `predictions.jsonl` for evaluator and future live replay;
- `audit.jsonl` showing model action, raw action, and mapped refs;
- `failures.jsonl` for unmappable predictions;
- `summary.json`.

The mapping is derived only from paired raw/model trajectory state for the same
`step_id`. If the model predicts a synthetic ref that was not visible in that
trajectory, the row fails fast. There is no fallback and no invented ref.

V6 holdout20 de-anonymization result:

| Item | Value |
| --- | ---: |
| Selected predictions | 1,124 |
| Written raw predictions | 1,124 |
| Failures | 0 |
| Mapped synthetic refs | 5,240 |

Raw-ref policy eval remained exact:

| Predictor | Total | Exact | Tool | Ref | Scope | Stop | Invalid | Unbounded |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| Qwen 0.5B LoRA V6 holdout20, de-anonymized | 1,124 | 1.000 | 1.000 | 1.000 | 1.000 | 1.000 | 0 | 0 |

This is not live MCP validation yet. It is the required bridge before live
validation: the same prediction can now be inspected in model-ref form and raw
kernel-ref form without contaminating the training prompt.

### P1.10 Live MCP Replay

Raw-ref policy evaluation still runs offline: it compares JSON actions against
JSON targets. The next validation step is to execute those predicted actions
through the real MCP adapter against the deployed kernel.

P1.10 adds:

```text
cargo run -p kmp-testkit --bin kernel_operator_mcp_replay
```

The replayer takes:

- raw audit trajectories;
- raw-ref predictions from the de-anonymizer;
- a live kernel endpoint, usually
  `https://kmp.underpassai.com`.

For long runs, `--log-progress-every N` emits compact JSONL progress events to
stderr so the operator can see which trajectory step is being replayed without
parsing Kubernetes logs.

It executes only bounded tool calls. `stop` actions are recorded but do not call
MCP. For tool calls, the replayer verifies:

- MCP returns a successful JSON-RPC response;
- the response contains typed `structuredContent`;
- every ref observed in the audited trajectory is present in the live response.

This is deliberately stricter than "the JSON action looked right". It proves the
operator action can be replayed through the same MCP/gRPC boundary an agent
would use.

Observed 100-step V6 holdout strict smoke on 2026-05-14:

| Item | Value |
| --- | ---: |
| Selected trajectory steps | 100 |
| Executed tool calls | 85 |
| Stop actions | 15 |
| Successful tool calls | 85 |
| Failed tool calls | 0 |
| Missing expected ref rows | 0 |
| Missing predictions | 0 |
| Invalid predictions | 0 |
| Unbounded tool calls | 0 |
| Partial result rows | 36 |
| Elapsed | 1m26.5s |

Observed full V6 holdout20 strict replay on 2026-05-14:

| Item | Value |
| --- | ---: |
| Selected trajectory steps | 1,124 |
| Executed tool calls | 976 |
| Stop actions | 148 |
| Successful tool calls | 976 |
| Failed tool calls | 0 |
| Missing expected ref rows | 0 |
| Missing predictions | 0 |
| Invalid predictions | 0 |
| Unbounded tool calls | 0 |
| Extra observed ref rows | 848 |
| Extra observed refs | 7,216 |
| Partial result rows | 424 |
| Elapsed | 10m15.9s |

Extra observed refs mean the live kernel returned additional valid context
beyond the audited minimum. The correctness gate is missing expected refs, not
the presence of extra inspectable context.

Full strict replay latency by action against the public TLS endpoint:

| Action | Count | Avg ms | Max ms |
| --- | ---: | ---: | ---: |
| `kmp_near` | 424 | 1,302.7 | 2,517 |
| `kmp_inspect` | 424 | 110.0 | 198 |
| `kmp_trace` | 128 | 127.1 | 181 |
| `stop` | 148 | 0.0 | 0 |

The current signal is good: correctness holds through MCP under the strict
action contract, and every partial result is explicit. `kmp_near` dominates
live replay latency, so candidate retrieval performance and pagination/budget
behavior remain the main operational things to watch as the holdout scales.

### P1.11 Scale, Pagination, And Publication Gate

P1.10 proves that the current V6 operator predictions are executable through
the real MCP/gRPC boundary. It does not yet prove that the operator generalizes
across a larger corpus. The next cut should keep kernel core unchanged and
scale the same training/evaluation path.

Top 1 requirement: P1.11.0 pagination/progress/resume gate.

- `kmp_trace` must keep returning bounded paths with explicit page metadata:
  cursor, `has_more`, returned count, and budget consumed.
- `kmp_near`, `goto`, `rewind`, and `forward` must expose the same bounded
  pagination semantics through both gRPC/KMP and MCP.
- Remote audit tools must report progress by about/task and support limit,
  offset or cursor-based resume. A long audit must not look like a hung
  process.
- Operator trajectories must include page metadata in `visible_state` when a
  tool result is partial, so the operator can learn when to continue, narrow
  scope, or stop.
- Publication requires a clean replay/audit where no tool call requests an
  unbounded history and no `trace` silently materializes the whole graph.

P1.11.0 first implementation slice:

- `memoryarena_kmp_run_audit` now supports `--limit`, `--offset`,
  `--log-progress-every`, and `--progress-output`.
- Remote inspect audit summaries now report `total_refs`, `offset`, `limit`,
  `checked_refs`, `next_offset`, and global `completed`.
- Audit progress events include `processed`, `selected`, `total`,
  `next_offset`, `page_completed`, global `completed`, found/missing/error
  counts, and elapsed time.
- Resume is explicit: rerun with `--offset <inspect.next_offset>` or the last
  progress event's `next_offset`.
- Live smoke against the public TLS endpoint validated page 0, page 1, and a
  single-ref page over the 221-task MemoryArena run with found refs and no
  errors.
- Temporal movement responses now carry `PageInfo` in the API contract for
  `Goto`, `Near`, `Rewind`, and `Forward`; the MCP adapter maps the same page
  object into `kmp_goto`, `kmp_near`, `kmp_rewind`, and
  `kmp_forward` structured output.
- `kernel_operator_mcp_replay` already exposes `--limit`, `--offset`, and
  `--log-progress-every`; replay rows now record `partial_result` and `page`,
  and summaries count partial results by action.

Input rules:

- use a fresh audited MemoryArena smart-writer run, not only prepared
  cost-estimate rows;
- keep train/eval split grouped by task id or run family, never by individual
  trajectory row;
- use a fresh `run_id` / `about` namespace for every live kernel run;
- keep synthetic model-facing refs for training and evaluation;
- keep de-anonymized raw refs only for offline policy evaluation and live MCP
  replay;
- do not expose benchmark answers, hidden tool outcomes, final writer
  relations, or future refs in the model-facing prompt.

Live run hygiene:

- do not reuse a `run_id` after a non-smart or partial smoke has written data
  to the deployed kernel;
- if a smoke is repeated, regenerate artifacts with a new `run_id`;
- a repeated run over the same `about` can make early asks observe answer
  feedback written by the previous run, producing false future-leak failures.

Observed P1.11 setup smokes on 2026-05-11:

| Smoke | Scope | Result |
| --- | ---: | --- |
| TLS runner smoke without writer | 1 task / 27 events | 27/27 successful, 9/9 known-at-clean, 0 future leaks |
| Smart-writer smoke with reused `run_id` | 1 task / 27 events | invalid smoke: prior data caused 9/9 future leaks |
| Smart-writer smoke with fresh `run_id` | 1 task / 27 events | 27/27 successful, 9/9 known-at-clean, 0 future leaks, 17/17 valid LLM outputs |

Observed 221-task progressive-search run on 2026-05-12:

| Runner item | Result |
| --- | ---: |
| Abouts / tasks | 221 |
| Subtasks / asks | 1,641 |
| Ingest writes | 3,282 |
| MCP navigation asks | 1,641 |
| `kmp_near` probe calls | 1,641 |
| `kmp_inspect` probe calls | 1,641 |
| `kmp_trace` probe calls | 1,420 |
| Future answer leaks | 0 |
| Unexpected refs | 0 |
| Smart-writer LLM invalid outputs | 0 |
| Suspect relations | 0 |
| Elapsed | 3h12m23s |

Local scorecard over the same run:

| Scorecard item | Result |
| --- | ---: |
| Task successes | 216 / 221 |
| Task success rate | 0.9774 |
| Process score | 0.8325 |
| Micro process score | 0.8422 |
| Full ref recall asks | 1,641 / 1,641 |
| Future answer leaks | 0 |
| Unexpected refs | 0 |
| Failure class | reader/extraction gap only |

Trajectory export and SFT preparation from that run:

| Dataset item | Result |
| --- | ---: |
| Exported operator trajectories | 12,465 |
| Read-mode trajectories | 6,343 |
| Writer context-read trajectories | 6,122 |
| Tool-call trajectories | 10,824 |
| Stop trajectories | 1,641 |
| Export failures | 0 |
| Redaction findings | 0 |
| SFT train rows | 11,177 |
| SFT eval rows | 1,288 |
| Train task groups | 197 |
| Eval task groups | 24 |
| Dropped non-visible target refs | 0 |
| Model-facing leak audit findings | 0 |

Baseline checks on the anonymized eval split:

| Predictor | Total | Exact | Tool | Ref | Scope | Stop | Invalid | Unbounded |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| Oracle | 1,288 | 1.000 | 1.000 | 1.000 | 1.000 | 1.000 | 0 | 0 |
| Deterministic | 1,288 | 0.264 | 1.000 | 0.435 | 1.000 | 1.000 | 0 | 0 |

The deterministic baseline losing ref accuracy after anonymization is expected
and useful: the larger corpus no longer lets a rule recover the right ref from
the raw MemoryArena id shape. The operator must use visible state and tool
observations.

Post-deploy P1.11.0 checks on 2026-05-12:

| Check | Result |
| --- | ---: |
| Deployed image | `ghcr.io/underpass-ai/kmp:dev-78e9a9f` |
| MCP temporal page smoke | `kmp_forward` page 1 `has_more=true`, page 2 `has_more=false` |
| Remote inspect audit smoke | 100 / 100 refs found |
| Remote inspect audit errors | 0 |
| Remote inspect audit total refs | 12,150 |
| Remote inspect audit next offset | 100 |
| Regenerated trajectories from current exporter | 12,465 |
| Regenerated SFT train rows | 11,177 |
| Regenerated SFT eval rows | 1,288 |
| Regenerated eval oracle exact accuracy | 1.000 |
| Regenerated eval deterministic exact accuracy | 0.264 |

The local artifact cache for this cut is outside the repository tree at
`../kmp-artifacts/operator/p111-pageinfo-221-20260512/`. It
contains large benchmark runs, JSONL trajectories, SFT splits, logs, and audit
outputs copied from `/tmp`.

The 221-task run was recorded before temporal `page` metadata existed in stored
MCP navigation rows, so its exported trajectories do not contain historical
`page` or `partial_result` values. The trajectory exporter now preserves that
metadata when source rows carry it. Page metadata is already validated through
live KMP/MCP calls and must be captured by future page-aware replay or fresh
page-aware benchmark runs.

This run is the first useful P1.11 scale corpus, but the remote `--inspect`
audit exposed why P1.11.0 is now top priority: an exhaustive audit over
thousands of remote `inspect` calls is too opaque while it runs. Large audits
and graph traversal must be paged, bounded, resumable, and observable in the
same way normal KMP reads are.

Recommended run order:

1. Implement and validate P1.11.0 pagination/progress/resume for KMP/MCP
   traversal, remote audit, and replay.
2. Generate or select the next larger MemoryArena run. Target the 221-task
   progressive set when cost and time are acceptable; use a smaller stepped
   run only as a smoke.
3. Run local scorecard and remote audit with progress/pagination enabled.
4. Export trajectories with writer candidate details.
5. Prepare a grouped, anonymized, visible-ref-only split.
6. Run deterministic and generalist baselines on the same eval set.
7. Train the same small Qwen 0.5B LoRA recipe from scratch.
8. Predict on the held-out set and evaluate against anonymized trajectories.
9. De-anonymize predictions and run raw-ref policy evaluation.
10. Run live MCP replay first with `--limit 100`, then full replay only if the
   smoke has zero missing predictions, invalid predictions, unbounded calls,
   MCP failures, and missing expected refs.

Metrics to publish for this cut:

- trajectory count, train count, eval count, grouped split rule;
- exact/tool/ref/scope/stop accuracy;
- invalid prediction and unbounded call counts;
- deterministic baseline comparison;
- generalist LLM baseline comparison, if cost allows;
- live MCP replay success, missing expected refs, extra observed refs, and
  latency by action;
- prompt leak audit result.

Exit criteria:

- P1.11.0 pagination/progress/resume reports prove that `trace`, `near`,
  temporal moves, replay, and audit stay bounded at 221-task scale or higher;
- no model-facing leak of raw MemoryArena refs or benchmark answers;
- zero invalid and unbounded operator actions on the held-out set;
- live MCP replay has zero MCP failures and zero missing expected refs;
- latency report shows whether `kmp_near` remains the dominant cost;
- failures, if any, are classified as operator, kernel retrieval, writer,
  reader, or benchmark/domain reasoning.

## Fast Training Path

The goal is fast iteration, not a large first model. The first operator model
should prove that a small specialist can choose bounded kernel tools better
than a generalist baseline on the same replayable trajectories.

### Hardware Assumption

Target local hardware:

- 4x RTX 3090;
- 24 GB VRAM per GPU;
- local fine-tuning with LoRA/QLoRA;
- offline replay evaluation before live-kernel validation.

Expected first useful training durations:

| Model size | Method | Expected iteration time |
| --- | --- | --- |
| 0.5B-1.5B | LoRA or full small-model fine-tune | 30 min - 3 h |
| 3B | LoRA | 2 - 6 h |
| 7B-8B | QLoRA | 6 - 18 h |
| 14B | QLoRA | 1 - 3 days |

The first slice should prefer 0.5B-1.5B or 3B. A 7B/8B model is useful only
after the dataset and evaluator are clean.

### Speed Principles

1. Train on decisions, not whole tasks.

Each item should be:

```text
visible_state -> next_tool_call_or_stop
```

Do not include full conversations, full graph dumps, or hidden benchmark
answers in the model-facing input.

2. Start with a small action space.

Initial read/navigation actions:

- `kmp_near`
- `kmp_inspect`
- `kmp_trace`
- `stop`

Hold back until later:

- `kmp_goto`
- `kmp_rewind`
- `kmp_forward`
- `kmp_write_memory`

This reduces ambiguity and lets the first model learn the highest-frequency
navigation moves before temporal movement and write policy are added.

3. Distill from multiple sources.

Generate candidate trajectories from:

- successful audited MemoryArena MCP runs;
- deterministic navigation baseline;
- generalist LLM baseline;
- classified hard negatives.

Only train on failed trajectories when the failure reason is labelled. Broken
logs without labels should be excluded.

4. Use curriculum stages.

Recommended stages:

- valid JSON action;
- valid tool name;
- bounded arguments;
- valid refs and scopes;
- correct next tool;
- correct stop decision;
- pagination and temporal movement;
- writer relation proposal with cited evidence.

Do not mix all stages on day one. If JSON validity or bounded arguments fail,
later training metrics are not meaningful.

5. Cache evaluation.

The evaluator should run offline from exported trajectories and cached tool
results. Live kernel calls are reserved for final validation, not every training
iteration.

6. Measure cheap metrics first.

Fast-loop metrics:

- JSON validity;
- tool selection accuracy;
- invalid ref rate;
- invalid scope rate;
- unbounded call rate;
- stop accuracy;
- excess tool calls;
- future leak rate;
- known-at clean rate.

Do not wait for an end-to-end benchmark score to decide whether a checkpoint is
usable.

### Five-Day Fast Track

Day 1:

- implement the trajectory exporter;
- export the MemoryArena 50-task run;
- validate redaction and mixed-run rejection.

Day 2:

- implement deterministic baseline;
- implement offline evaluator;
- produce baseline summary.

Day 3:

- train first 0.5B-1.5B or 3B LoRA checkpoint;
- evaluate offline against deterministic baseline.

Day 4:

- run a generalist LLM baseline on the same trajectory items;
- compare tool calls, invalid actions, boundedness, and stop decisions.

Day 5:

- run live-kernel validation on a small held-out slice;
- package dataset card, model card draft, and evaluation summary for Hugging
  Face publication.

### Hugging Face Publication Path

Publish only after redaction, evaluation, and live replay are credible. The
release checklist is tracked in
[`kernel-tool-operator-publication-plan.md`](kernel-tool-operator-publication-plan.md).

Recommended artifacts:

- dataset: `underpass-ai/kernel-operator-trajectories`;
- model: `underpass-ai/kernel-tool-operator-small`;
- optional Space: `underpass-ai/kernel-operator-demo`.

Correct positioning:

> A small specialist model trained to operate KMP memory tools
> through bounded, auditable trajectories.

Avoid claims that it is a general QA model, a memory database, or a replacement
for the kernel.

### LongMemEval Mixed-Trajectory Slice

The Operator dataset should not learn only the MemoryArena graph shape. The
LongMemEval rows are exported by a separate binary,
`longmemeval_operator_trajectory_export`, but use the same
`kernel-operator-trajectory-v1` output contract as MemoryArena.

Current preserved local cache:

```text
../kmp-artifacts/operator/longmemeval-valid-20260512/
```

Exported LongMemEval trajectory counts:

| Source | Trajectories | Notes |
| --- | ---: | --- |
| Balanced60 v6 | 120 | `kmp_ask` + stop rows |
| 100-prefix v6 | 200 | `kmp_ask` + stop rows |
| MS30 smart writer | 268 | `kmp_ask`, stop, `kmp_near`, `kmp_inspect` writer reads |

Mixed MemoryArena P1.11 + LongMemEval SFT output:

```text
../kmp-artifacts/operator/longmemeval-valid-20260512/kernel-operator-sft-mixed-memoryarena-longmemeval-20260512/
```

Summary:

- total model-facing rows: 13,053;
- train rows: 11,729;
- eval rows: 1,324;
- split mode: grouped by `task_id`; LongMemEval maps this to `question_id`;
- dropped non-visible target refs: 0;
- no-gold audit findings: 0.

Action mix:

| Action | Rows |
| --- | ---: |
| `kmp_near` | 4,806 |
| `kmp_inspect` | 4,806 |
| `kmp_trace` | 1,420 |
| `kmp_ask` | 190 |
| `stop` | 1,831 |

The no-gold audit checks the model-facing OpenAI JSONL files for
`target_action`, `observed_outcome`, `expected_answer`,
`expected_answer_turn_refs`, `answer_session_ids`, `answer_session_refs`,
`has_answer`, and gold-label markers.

### LongMemEval 500 Full-History Smoke

The cleaned LongMemEval-S file was restored from Hugging Face and preserved
outside the repo:

```text
../kmp-artifacts/operator/longmemeval-valid-20260512/datasets/longmemeval_s_cleaned.json
```

The 500-item adapter pass completed:

- prepared items: 500;
- sessions: 23,867;
- turns: 246,750;
- expected evidence turns: 896;
- abstention items: 30.

The first live smoke initially exposed an infrastructure regression:

```text
Valkey TLS failed with UnknownIssuer because valkey.tls was enabled for the
server but valkeyTls was disabled for the kernel client.
```

The deployed release was corrected with Helm revision 128 by enabling
`valkeyTls.enabled=true` and mounting `kmp-valkey-tls`.

After redeploy, a one-item full-history smoke against the public TLS endpoint
completed:

| Metric | Value |
| --- | ---: |
| Items | 1 |
| Ingested entries | 551 |
| Full evidence hits | 1 / 1 |
| Elapsed | 130,052 ms |

The smoke used the MCP runner path. A direct Codex MCP check also exercised
`kmp_ingest`, `kmp_ask`, `kmp_near`, and `kmp_inspect` against the
same live kernel boundary on the isolated about
`codex:mcp:operator-longmemeval-smoke:20260512`.

Do not treat this as the publishable LongMemEval-500 score. The full 500 run was
not launched in this slice because the smoke projects to many hours, and the
current LongMemEval adapter still uses `has_answer` to emit `supports_answer`
relations when no generated labels are provided. A publishable official-style
run needs a context-construction path that does not use `has_answer`,
`answer_session_ids`, or gold answer metadata.

Post-audit status on 2026-05-13: the generated 500 full-history artifacts are
also quarantined for Operator training claims. The cleaned dataset contains
questions where a `session_id` appears more than once, or normalizes to the
same KMP dimension id more than once. The adapter now rejects this shape before
writing artifacts or calling the kernel:

```text
question 58bf7951 has duplicate or colliding session_id `07b7a667_1` at session index 47; repeated LongMemEval session ids are not supported by the KMP adapter
```

This is intentional. The adapter must not invent a fallback identity for
benchmark sessions. LongMemEval support needs an explicit model for repeated
session ids before this dataset shape can be used. Until then, the 500
full-history artifacts remain quarantined.

### LongMemEval Legacy Operator Stress Run

After quarantining the cleaned 500 full-history shape, a smaller
LongMemEval-only Operator run was executed from audited legacy slices:

| Source | Decisions |
| --- | ---: |
| Balanced60 v6 | 120 |
| 100-prefix v6 | 200 |
| MS30 smart writer | 268 |
| Total | 588 |

The first version of this combined dataset exposed a trajectory identity bug:
`step_id` was unique inside each source run, but not across combined
LongMemEval slices. Because the policy evaluator keys predictions by `step_id`,
duplicate ids could silently overwrite predictions. The exporter now prefixes
LongMemEval decision ids with the source `run_id`, and both dataset preparation
and policy evaluation reject duplicates fail-fast.

Corrected v7 dataset:

| Metric | Value |
| --- | ---: |
| Total rows | 588 |
| Train rows | 528 |
| Eval rows | 60 |
| Duplicate `step_id`s | 0 |
| No-gold audit findings | 0 |

Corrected v7 LoRA training:

| Metric | Value |
| --- | ---: |
| Base model | `Qwen/Qwen2.5-0.5B-Instruct` |
| Method | LoRA SFT |
| Epochs | 3 |
| Best/final eval loss | 0.06259 |
| Eval mean token accuracy | 0.9830 |
| Runtime | 7m03s |

The earlier five-epoch LongMemEval-only run showed validation loss
deterioration after the best point. Three epochs are the current safe setting
for this small dataset until the corpus is larger.

Corrected v7 policy evaluation:

| Metric | Value |
| --- | ---: |
| Eval decisions | 60 |
| Predictions parsed | 48 |
| Prediction failures | 12 |
| Invalid predictions | 0 |
| Unbounded tool calls | 0 |
| Exact action accuracy | 0.7000 |
| Tool accuracy | 0.6818 |
| Primary ref accuracy | 0.7273 |
| Stop accuracy | 1.0000 |

The 12 prediction failures are all in MS30 smart-writer `read:1` steps. The
target action in those rows is typically `kmp_inspect`, but the model tends
to over-generate: it starts with a plausible JSON-like action and then emits
additional JSON/action fragments until the strict parser rejects the response.

This is a useful failure. It says the next Operator work is not more benchmark
scoring. The next gate is reliable single-action emission:

- exactly one JSON action;
- top-level `type`, `tool`, and `arguments` when calling a tool;
- no invented fields;
- no second action in the same response;
- bounded arguments only;
- fail-fast parsing, not permissive recovery.

## Next Execution Slice

The original first-slice checklist is complete. The next execution slice remains
P1.11, with MemoryArena P1.11 as the clean training base:

1. produce a larger audited MemoryArena smart-writer run;
2. export trajectories with candidate details;
3. prepare a grouped anonymized split;
4. train and evaluate the small operator from scratch;
5. de-anonymize predictions;
6. replay them through live MCP;
7. update this document with the larger-run table before making any public
   model claim.

Do not publish the dataset or model until the leak audit, offline policy eval,
and live MCP replay are all clean.

Do not continue training from the mixed LongMemEval model for publication. Keep
it as an internal baseline only. The next publishable candidate should be
trained from a clean, versioned dataset after the audit gates pass.

## Open Questions

- Should write-mode trajectories be a separate dataset from read-mode
  trajectories? Initial answer: yes.
- Should the operator be allowed to call `kmp_ask`, or should `ask` be the
  reader's terminal move? Initial answer: allow it, but score overuse.
- Should the model learn relation writing directly? Initial answer: only after
  read/navigation is stable, and only with proof-cited relations.
- Should the first public release use only read/navigation actions, or include
  writer relation proposal? Initial answer: keep the first public operator
  read/navigation only.
- Should agentic retrieval/reranking models be added to the Operator path?
  Initial answer: yes, but not inside the first Operator training cut. Models
  such as `lightonai/Agent-ModernColBERT` are retrieval/reranking sidecars, not
  Operator replacements. They can rank candidate refs before the Operator or
  reader chooses the next KMP move.
- Should function-calling specialist SLMs be tested before abandoning the 0.5B
  path? Current answer: yes. `MadeAgents/Hammer2.0-0.5b` was measured and did
  not beat the Qwen v7 baseline on exact policy quality. It still proves that a
  function-calling prior helps parse validity, but it is not enough by itself.
  `google/functiongemma-270m-it` is now the next serious sub-0.5B candidate:
  it is gated and uses the Gemma license, but it is explicitly aligned with
  tool/function calling and has a tuning lab for custom function schemas. Under
  the generic chat/JSON compatibility cut it underperformed; under the native
  tool-schema cut it improved parse count but still underperformed on policy
  accuracy. Do not rerun the same corpus immediately; use this result to drive
  constrained decoding and stronger decision data.
- Should a larger non-0.5B model be measured as a control? Initial answer: yes.
  `meta-llama/Llama-3.2-1B-Instruct` is the first 1B control candidate, but it
  is gated and currently awaiting access approval for the cluster token.

## Success Criteria For P1

P1 is successful when we can say:

- kernel tool-use trajectories are exported reproducibly;
- a baseline can be scored without benchmark gold answers;
- the operator policy is bounded and auditable;
- tool-call efficiency can be compared against a generalist LLM;
- de-anonymized predictions can be replayed through live MCP/gRPC;
- failures are classified by stage: operator, kernel retrieval, reader, writer,
  or benchmark/domain reasoning.

The article angle then becomes stronger:

> The kernel does not only store temporal multidimensional memory. It emits
> auditable tool-use trajectories that can train and evaluate smaller specialist
> operators for memory navigation.
