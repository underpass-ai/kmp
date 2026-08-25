# Operator Training Data Audit - 2026-05-13

This note records the current Operator training-data state after the
LongMemEval `session_id` collision investigation.

Follow-up on 2026-05-14: a separate action-contract audit found that the
Operator predictor/evaluator accepted actions that were structurally close to
the KMP/MCP contract but not exact schema matches. That issue affects Operator
measurement and publication claims, not kernel core or this training-data
classification. See
[`operator-action-contract-audit-2026-05-14.md`](operator-action-contract-audit-2026-05-14.md).

## Decision

Do not discard the previous Operator runs.

Keep them as baselines, but do not continue training on top of them until the
input data is explicitly classified. The next serious Operator run should be
trained from a clean, versioned dataset produced after this audit.

## Current Classification

| Source | Status | Reason |
| --- | --- | --- |
| MemoryArena P1.11 221-task corpus | Clean baseline | 12,465 trajectories, grouped split, anonymized refs, zero dropped non-visible targets, zero redaction findings. |
| Qwen 0.5B LoRA V6 holdout20 | Clean baseline | MemoryArena-only training/eval/replay baseline. Keep as the strongest pre-audit model checkpoint. |
| Mixed MemoryArena + LongMemEval SFT 20260512 | Internal baseline only | No-gold audit is clean, but it mixes older LongMemEval slices. Useful for comparison, not a publication claim. |
| LongMemEval Balanced60 v6 artifacts | Usable legacy slice | No duplicate dimension ids observed in generated ingest artifacts. Still secondary because it is LongMemEval-specific. |
| LongMemEval 100-prefix v6 artifacts | Usable legacy slice | No duplicate dimension ids observed in generated ingest artifacts. Still secondary because it is LongMemEval-specific. |
| LongMemEval MS30 smart-writer artifacts | Usable legacy slice | No duplicate dimension ids observed in generated ingest artifacts and includes writer reads. Still secondary. |
| LongMemEval-S cleaned 500 full-history artifacts | Quarantined | The dataset contains repeated or normalized-colliding `session_id`s inside individual questions. The old adapter generated duplicate dimensions. |

## Concrete LongMemEval Collision

The cleaned 500-item LongMemEval-S file contains at least these colliding
questions in the generated full-history artifacts:

| Question id | Colliding dimension |
| --- | --- |
| `58bf7951` | `longmemeval:session:run:lme-s500-mini-20260512-a:question:58bf7951:07b7a667_1` |
| `1e043500` | `longmemeval:session:run:lme-s500-mini-20260512-a:question:1e043500:d5d1f9c4` |
| `001be529` | `longmemeval:session:run:lme-s500-mini-20260512-a:question:001be529:sharegpt_syblhtk_0` |
| `d23cf73b` | `longmemeval:session:run:lme-s500-mini-20260512-a:question:d23cf73b:334ab2f1` |
| `caf03d32` | `longmemeval:session:run:lme-s500-mini-20260512-a:question:caf03d32:sharegpt_ycfhenu_0` |
| `91b15a6e` | `longmemeval:session:run:lme-s500-mini-20260512-a:question:91b15a6e:sharegpt_fgnzlte_0` |
| `078150f1` | `longmemeval:session:run:lme-s500-mini-20260512-a:question:078150f1:sharegpt_z7ogy29_0` |
| `gpt4_4929293b` | `longmemeval:session:run:lme-s500-mini-20260512-a:question:gpt4_4929293b:sharegpt_81riysf_0` |
| `gpt4_76048e76` | `longmemeval:session:run:lme-s500-mini-20260512-a:question:gpt4_76048e76:8fcaf3a9_2` |
| `gpt4_c27434e8_abs` | `longmemeval:session:run:lme-s500-mini-20260512-a:question:gpt4_c27434e8_abs:8c64ce26_3` |
| `18bc8abd` | `longmemeval:session:run:lme-s500-mini-20260512-a:question:18bc8abd:sharegpt_vyhqfrx_0` |
| `c7dc5443` | `longmemeval:session:run:lme-s500-mini-20260512-a:question:c7dc5443:6b8e8bb3_1` |
| `1d4da289` | `longmemeval:session:run:lme-s500-mini-20260512-a:question:1d4da289:sharegpt_qktlfws_21` |

The adapter now fails fast on this shape instead of inventing a fallback id.
The repeated check fails before writing artifacts or calling the kernel:

```text
question 58bf7951 has duplicate or colliding session_id `07b7a667_1` at session index 47; repeated LongMemEval session ids are not supported by the KMP adapter
```

## Why This Does Not Invalidate The Kernel

The collision is in the benchmark adapter boundary, not in kernel core.

The kernel correctly rejects duplicate dimensions. The previous adapter used a
LongMemEval session identity that was too weak for the cleaned full-history
dataset. This affects generated artifacts from that adapter shape; it does not
change the KMP model, the gRPC/MCP contract, or the MemoryArena Operator
trajectory corpus.

## Training Rule Going Forward

Operator training data must satisfy all of these before it can be used for a
new model claim:

- no duplicate dimension ids inside an ingest request;
- no unsupported benchmark shape accepted silently;
- no fallback identity generation for unsupported LongMemEval sessions;
- grouped split, never row split;
- model-facing refs anonymized;
- target refs visible in the prompt state;
- prompt leak audit with zero findings;
- no benchmark answer fields, `has_answer`, `answer_session_ids`, or gold labels
  in model-facing prompts;
- live MCP replay for de-anonymized predictions before publication claims.

## What To Train Next

Recommended next run:

1. Use the MemoryArena P1.11 221-task corpus as the clean base.
2. Keep the previous V6 holdout20 model as the baseline to beat.
3. Treat the mixed MemoryArena + LongMemEval model as an internal comparison
   only.
4. Do not add LongMemEval-S cleaned 500 full-history data until the benchmark
   adapter has an explicit, documented model for repeated session ids.
5. If LongMemEval is used before that, use only explicitly audited legacy
   slices and mark the result as secondary.

## LongMemEval Legacy Operator Experiment

A LongMemEval-only experiment was run after the collision audit to stress the
Operator on a chat-style memory shape before continuing with the public
MemoryArena-first path.

Input slices:

| Source | Decisions |
| --- | ---: |
| Balanced60 v6 | 120 |
| 100-prefix v6 | 200 |
| MS30 smart writer | 268 |
| Total | 588 |

The first export exposed a second identity issue: `step_id` was unique only
inside a source run, not across combined LongMemEval slices. The evaluator used
`step_id` as the prediction key, so duplicate ids could silently overwrite
predictions. This does not affect kernel core. It affects Operator dataset and
policy-evaluation hygiene.

The fix is now fail-fast at three boundaries:

- LongMemEval trajectory export namespaces every `step_id` with `run_id`;
- the SFT dataset preparer rejects duplicate `step_id`s;
- the policy evaluator rejects duplicate trajectory or prediction `step_id`s.

The corrected v7 dataset has:

- total model-facing rows: 588;
- train rows: 528;
- eval rows: 60;
- grouped split by LongMemEval task/question id;
- duplicate `step_id`s: 0;
- no-gold audit findings: 0.

The v7 LoRA run used Qwen2.5-0.5B-Instruct for three epochs. Three epochs were
chosen because the earlier five-epoch run showed validation loss deterioration
after the best point.

Corrected v7 policy-eval result:

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

All 12 prediction failures are concentrated in MS30 smart-writer `read:1`
steps. The model usually starts emitting a plausible action, then continues
generating extra JSON/action fragments until the response is rejected by the
strict parser. This is a real Operator robustness gap, not a parser fallback
case. The runtime contract remains: exactly one bounded JSON action or fail.

Next action for this line is to improve termination and constrained action
emission before using LongMemEval as training data for a publication candidate.

### Function-Calling Backbone Check

The same corrected LongMemEval v7 split was used to test whether a
function-calling 0.5B backbone improves the Operator contract.

Zero-shot `MadeAgents/Hammer2.0-0.5b` was not usable for KMP directly:

| Metric | Value |
| --- | ---: |
| Eval decisions | 60 |
| Predictions parsed | 0 |
| Prediction failures | 60 |
| Main failure reason | `incomplete_json` |

After LoRA training for three epochs on the v7 train split, Hammer improved
structured emission but did not beat the Qwen v7 policy baseline:

| Metric | Value |
| --- | ---: |
| Eval decisions | 60 |
| Predictions parsed | 53 |
| Prediction failures | 7 |
| Invalid predictions | 0 |
| Unbounded tool calls | 1 |
| Exact action accuracy | 0.6500 |
| Tool accuracy | 0.6591 |
| Primary ref accuracy | 0.7727 |
| Stop accuracy | 1.0000 |

Failure reasons:

| Reason | Count |
| --- | ---: |
| `incomplete_json` | 6 |
| `unsupported_action_type` | 1 |

Interpretation:

- Hammer's function-calling prior helps single-action JSON validity compared
  with Qwen v7: 53 parsed predictions instead of 48.
- The policy is weaker: exact action accuracy drops from 0.7000 to 0.6500.
- The single unbounded tool call keeps it out of release-candidate territory.
- Hammer remains a measured candidate, not the current Operator backbone.

The next sub-0.5B candidate should be `google/functiongemma-270m-it`, evaluated
in a separate cut. FunctionGemma is explicitly designed around tool/function
calling and has a Google-provided tuning workflow for custom function schemas,
which matches the Operator task more directly than generic instruction tuning.

FunctionGemma access gate, 2026-05-13:

- Kubernetes secret `underpass-runtime/huggingface-token` exists and exposes
  `HF_TOKEN`.
- The first FunctionGemma LoRA job was launched as
  `kop-functiongemma270m-lora-lme-v7-20260513`.
- The first launch failed before loading tokenizer/model with Hugging Face `403
  Forbidden` because the token account was not authorized for gated repo
  `google/functiongemma-270m-it`.
- After accepting Google's Gemma/FunctionGemma terms for the token account, the
  same manifest loaded the model and trained successfully.

FunctionGemma compatibility-cut result:

| Metric | Value |
| --- | ---: |
| Base model | `google/functiongemma-270m-it` |
| Method | LoRA SFT |
| Epochs | 3 |
| Final eval loss | 0.07252 |
| Eval mean token accuracy | 0.9829 |
| Train runtime | 404s |
| Eval decisions | 60 |
| Predictions parsed | 30 |
| Prediction failures | 30 |
| Invalid predictions | 0 |
| Unbounded tool calls | 0 |
| Exact action accuracy | 0.5000 |
| Tool accuracy | 0.3409 |
| Primary ref accuracy | 0.3409 |
| Stop accuracy | 0.9375 |

Failure reasons:

| Reason | Count |
| --- | ---: |
| `incomplete_json` | 26 |
| `extra_content_after_json` | 2 |
| `missing_or_extra_top_level_fields` | 2 |

Interpretation:

- This cut used the same generic chat/JSON Operator prompt as Qwen and Hammer.
- Under that compatibility contract, FunctionGemma does not beat the existing
  baselines.
- The result does not fully evaluate FunctionGemma's intended mode. Google
  documents it as a function-calling model that should receive tool schemas
  through its chat template/processor.
- The next fair FunctionGemma test should use a FunctionGemma-native exporter
  and predictor that map KMP actions to tool schemas, then convert the emitted
  function call back into the same policy-eval contract.

FunctionGemma-native cut, 2026-05-13:

This cut implemented the fairer test:

- KMP actions are exposed as FunctionGemma tool declarations:
  `kernel_ask`, `kernel_near`, `kernel_inspect`, `kernel_trace`, and
  `kernel_stop`.
- Training renders prompts through FunctionGemma's chat template with tool
  declarations.
- Targets are native FunctionGemma function-call strings.
- Prediction parses FunctionGemma function calls back into the same
  `predictions.jsonl` contract used by `kernel_operator_policy_eval`.

Training result:

| Metric | Value |
| --- | ---: |
| Base model | `google/functiongemma-270m-it` |
| Method | FunctionGemma-native LoRA SFT |
| Epochs | 3 |
| Final eval loss | 0.05202 |
| Eval mean token accuracy | 0.9891 |
| Train runtime | 491.8s |

Prediction result with `max_new_tokens=500`:

| Metric | Value |
| --- | ---: |
| Eval decisions | 60 |
| Predictions parsed | 39 |
| Prediction failures | 21 |
| Invalid predictions | 0 |
| Unbounded tool calls | 0 |
| Exact action accuracy | 0.4667 |
| Tool accuracy | 0.3636 |
| Primary ref accuracy | 0.3636 |
| Stop accuracy | 0.9375 |

Failure reasons:

| Reason | Count |
| --- | ---: |
| `incomplete_function_call` | 16 |
| `invalid_array_separator` | 2 |
| `trailing_argument_content` | 2 |
| `missing_function_call_start` | 1 |

Observed behavior:

- Native FunctionGemma improves parse count over the generic chat/JSON cut
  (`39/60` vs `30/60` parsed predictions).
- Policy quality is worse than Qwen v7 and Hammer v7.
- The model strongly overuses `kernel_ask` and `kernel_stop`.
- It does not reliably choose `kernel_near` or `kernel_inspect` on the held-out
  writer/navigation rows.
- Raising generation length from 220 to 500 only improved parsed predictions
  from 38 to 39, so the main issue is not max token budget.

Decision:

- FunctionGemma 270M is not the current Operator backbone.
- The experiment is still useful: it proves the native tool-schema path is
  implementable and measurable against the same policy contract.
- The next Operator work should focus on constrained decoding and stronger
  decision data, not another immediate FunctionGemma rerun on the same corpus.

## 1B Control Candidate: Llama 3.2 1B

`meta-llama/Llama-3.2-1B-Instruct` was added as a larger control candidate, not
as a sub-0.5B Operator candidate.

Rationale:

- 1.23B parameters, materially larger than the target Operator footprint;
- strong general instruction model;
- useful control to check whether the current failures are mostly model size,
  model family, or output-contract related.

Access status, 2026-05-13:

- Kubernetes manifests were prepared for LoRA training and prediction.
- Training job `kop-llama32-1b-lora-lme-v7-20260513` was launched.
- It failed before model load with Hugging Face `403 Forbidden`.
- Hugging Face response: access request is awaiting review from the repo
  authors.
- Relaunch only after Meta approves the token account for
  `meta-llama/Llama-3.2-1B-Instruct`.

## Future Retrieval/Reranking Sidecars

`lightonai/Agent-ModernColBERT` is recorded as a future candidate for agentic
retrieval/reranking, not as an Operator replacement.

It is a small ColBERT-style retrieval model trained on AgentIR data. Its role
would be to rank candidate KMP refs from reasoning-aware queries such as:

```text
Reasoning: what the agent is trying to verify and why
Query: the evidence needed from KMP memory
```

Potential uses:

- rerank `candidate_refs`;
- improve `kernel_ask` candidate ordering;
- choose which refs should be inspected first;
- reduce unnecessary `near`/`trace` calls;
- provide a retrieval sidecar behind a kernel port without contaminating
  kernel core.

Do not use it in the current Operator cut. The current cut is still about
single-action KMP/MCP operation and strict structured output.

## Future Function-Calling SLM Candidates

`MadeAgents/Hammer2.0-0.5b` was the first 0.5B function-calling candidate
tested as an Operator backbone. It improves parse validity but does not beat
the Qwen v7 policy baseline.

`google/functiongemma-270m-it` is also recorded as a serious future candidate.
It is exposed through Google's FunctionGemma Tuning Lab, a Gradio workflow for
fine-tuning custom tool/function schemas from CSV examples. The model is gated
and uses the Gemma license, but it is materially aligned with the Operator
problem: emit tool calls and arguments for a provided function schema.

Do not mix FunctionGemma with the Hammer cut. Evaluate it separately with the
same strict parser, `--stop-after-json`, policy eval, and MCP replay gates.

## Publication Position

The first public Operator claim should be MemoryArena-first:

> Operator is trained to choose bounded KMP/MCP navigation actions from visible
> memory state and is replayed against live KMP.

LongMemEval remains useful as a secondary stress test for chat-style memory,
but it should not drive the first public model release until its adapter
semantics are clean.
