---
license: apache-2.0
base_model: Qwen/Qwen2.5-0.5B-Instruct
library_name: peft
pipeline_tag: text-generation
tags:
  - tool-use
  - agent-memory
  - mcp
  - llm-tools
  - knowledge-graph
  - temporal-memory
  - kernel-memory-protocol
  - underpass-kernel
datasets:
  - underpass-ai/kernel-operator-trajectories
language:
  - en
---

# KMP Tool Operator Small

> **Publication note (2026-05-29):** v8.x training (through the 2026-05-29 Tier-4
> run) was done **without ref anonymization** (a design divergence vs the opaque-ref
> requirement). Separately, an evaluation bug (eval rows missing the MCP/API schema
> in their system prompt) initially made the model look broken on read-nav; a
> diagnostic showed the model is actually strong (99.7% base, 92.9% all) once given
> the schema. Do NOT use v8.x for publication until (a) every row + the runtime
> carry the full-schema prompt and (b) refs are anonymized. The V6 result (1.000
> exact accuracy) and this template remain valid guides. See the operator repo
> `docs/training/DIVERGENCE_AND_CORRECTIVE_PLAN_2026-05-29.md`.

This is a draft model card for `underpass-ai/kernel-tool-operator-small`.
Do not publish it as final until the release gate in
[`kernel-tool-operator-publication-plan.md`](../kernel-tool-operator-publication-plan.md)
is complete.

## Model Summary

KMP Tool Operator Small is a specialist model for operating Kernel
Memory Protocol tools. It predicts one bounded memory-navigation action at a
time from visible tool state.

The model is not a general assistant, a QA model, a benchmark solver, or part
of kernel core. It is intended to reduce the cost and variance of memory tool
operation around an existing KMP deployment.

Minimum public claim after the release gate:

```text
This model predicts bounded Kernel Memory Protocol tool actions from visible
memory-navigation state. It was evaluated on held-out audited trajectories and
then replayed through live MCP/gRPC against a deployed KMP.
```

## Intended Use

Use this model when an application already has:

- an KMP or KMP-compatible memory backend;
- bounded MCP/gRPC memory tools;
- visible tool state;
- a policy that checks predicted actions before execution.

The expected loop is:

```text
visible memory state -> operator model -> one KMP/MCP action -> validated tool call
```

## Not Intended For

This model should not be used as:

- a general chat model;
- a final answer generator;
- an autonomous agent;
- a hidden memory API;
- a substitute for kernel validation;
- a writer of memory relations without separate writer-mode proof.

## Input Contract

The input is a compact tool-state prompt generated from
`kernel-operator-trajectory-v1`.

Model-facing references must be synthetic, such as `ref_0001`. Raw benchmark
refs and private memory refs must not appear in prompts.

The prompt may include:

- current synthetic ref;
- known synthetic refs;
- last tool and last observed refs;
- bounded remaining budget;
- allowed tools;
- visible candidate details.

The prompt must not include:

- target action;
- observed outcome;
- benchmark gold answer;
- future refs;
- hidden raw tool output;
- private raw memory;
- secrets or credentials.

## Output Contract

The output must be a single JSON object with either a tool call:

```json
{
  "type": "tool_call",
  "tool": "kmp_inspect",
  "arguments": {
    "ref": "ref_0003",
    "include": {
      "details": true,
      "relationships": true,
      "raw": false
    }
  }
}
```

or a stop action:

```json
{
  "type": "stop",
  "answer_policy": "evidence_or_unknown",
  "final_refs": ["ref_0003"],
  "reason": "sufficient_evidence"
}
```

Any output that is not valid JSON, not one action, unbounded, or refers to a
non-visible ref must be rejected by the caller.

## Allowed Tools

Initial read/navigation release:

- `kmp_ask`
- `kmp_near`
- `kmp_trace`
- `kmp_inspect`
- `kmp_goto`
- `kmp_rewind`
- `kmp_forward`

Writer mode is out of scope for the first public release unless it has its own
separate evaluation.

## Training Data

Training data comes from audited KMP tool trajectories.

Planned public dataset:

- Dataset: `underpass-ai/kernel-operator-trajectories`
- Split: grouped by task id or run family
- Refs: synthetic model-facing refs only
- Raw audit artifacts: redacted or not published
- Forbidden fields: target leakage, benchmark gold answers, future refs, raw
  tool output, private memory

Fill this table after the release run:

| Item | Value |
| --- | --- |
| Dataset version | `<fill>` |
| Source benchmark/run | `<fill>` |
| Train rows | `<fill>` |
| Eval rows | `<fill>` |
| Dropped non-visible target refs | `<fill>` |
| Leak audit result | `<fill>` |

## Evaluation

The model must be evaluated on held-out grouped trajectories, not a row split.

Required metrics:

| Metric | Why it matters |
| --- | --- |
| Exact action accuracy | Same tool and same bounded arguments |
| Tool accuracy | Correct memory operation |
| Ref accuracy | Correct visible reference selection |
| Scope accuracy | Correct about/dimension scope |
| Stop accuracy | Stops only when evidence is sufficient |
| Invalid predictions | Must be zero for release |
| Unbounded calls | Must be zero for release |

Fill this table after the release run:

| Predictor | Total | Exact | Tool | Ref | Scope | Stop | Invalid | Unbounded |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| Deterministic baseline | `<fill>` | `<fill>` | `<fill>` | `<fill>` | `<fill>` | `<fill>` | `<fill>` | `<fill>` |
| Generalist LLM baseline | `<fill>` | `<fill>` | `<fill>` | `<fill>` | `<fill>` | `<fill>` | `<fill>` | `<fill>` |
| Operator model | `<fill>` | `<fill>` | `<fill>` | `<fill>` | `<fill>` | `<fill>` | `<fill>` | `<fill>` |

Current internal validation before the larger release gate:

| Run | Total | Exact | Tool | Ref | Scope | Stop | Invalid | Unbounded |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| V6 explicit holdout20 | 1,124 | 1.000 | 1.000 | 1.000 | 1.000 | 1.000 | 0 | 0 |

This internal result is not the final public release claim. The release claim
must use the fresh larger P1.11 run.

## Live MCP Replay

Offline policy evaluation is not enough. Predictions must be de-anonymized and
replayed against live MCP/gRPC using the same public adapter path an LLM would
use.

Required release result:

| Replay metric | Required result |
| --- | ---: |
| MCP/gRPC failures | 0 |
| Missing expected refs | 0 |
| Missing predictions | 0 |
| Invalid predictions | 0 |
| Unbounded calls | 0 |

Current internal validation before the larger release gate:

| Item | Value |
| --- | ---: |
| Selected trajectory steps | 1,124 |
| Executed tool calls | 976 |
| Stop actions | 148 |
| Failed tool calls | 0 |
| Missing expected ref rows | 0 |
| Unbounded tool calls | 0 |
| Elapsed | 10m15.9s |

## Safety And Redaction

The publication dataset and prompts must be checked for:

- raw MemoryArena refs;
- raw private refs;
- benchmark answers or gold fields;
- future refs;
- raw tool output;
- secrets, tokens, credentials, and prompts containing secrets.

Fail closed. If a row cannot be proven safe, remove it from the public dataset.

## Limitations

- The model predicts memory tool actions; it does not answer user questions.
- It depends on KMP/MCP tool schemas and validation.
- It has not been proven as a writer of new memory relations unless writer mode
  is released separately.
- It should be used behind a validator that rejects invalid, unbounded, or
  non-visible refs.
- It is evaluated on audited trajectories, not arbitrary agent behavior.

## How To Reproduce

Use the commands documented in
[`scripts/operator/README.md`](../../../scripts/operator/README.md) and the
release-specific artifact paths published with the dataset.

Minimum sequence:

```bash
python scripts/operator/prepare_operator_sft_dataset.py ...
python scripts/operator/train_operator_sft_lora.py ...
python scripts/operator/predict_operator_sft.py ...
cargo run -p kmp-testkit --bin kernel_operator_policy_eval -- ...
python scripts/operator/deanonymize_operator_predictions.py ...
cargo run -p kmp-testkit --bin kernel_operator_mcp_replay -- ...
```

## Project Links

- Kernel repository: https://github.com/underpass-ai/kmp
- Dataset: https://huggingface.co/datasets/underpass-ai/kernel-operator-trajectories
- Article: https://dev.to/tirsogarcia/building-kernel-memory-protocol-navigable-memory-for-ai-agents-315j

## Citation

```bibtex
@software{underpass_kernel_2026,
  author = {Tirso Garcia Ibanez},
  title = {KMP: Kernel Memory Protocol for navigable agent memory},
  year = {2026},
  url = {https://github.com/underpass-ai/kmp},
  license = {Apache-2.0}
}
```
