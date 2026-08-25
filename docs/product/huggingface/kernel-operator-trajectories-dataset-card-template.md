---
license: apache-2.0
task_categories:
  - text-generation
language:
  - en
tags:
  - tool-use
  - agent-memory
  - mcp
  - llm-tools
  - knowledge-graph
  - temporal-memory
  - kernel-memory-protocol
  - underpass-kernel
pretty_name: Kernel Operator Trajectories
---

# Kernel Operator Trajectories

> **CRITICAL publication gate (2026-05-29): refs MUST be anonymized.** Model-facing
> refs must be opaque ids (`ref_0001`/`about_0001`); no domain topic may appear. The
> v8.1.8 Tier-4 corpus (2026-05-29) was built with anonymization OFF and contains
> literal domain refs — it does **NOT** meet this gate and must NOT be published.
> Require the corpus manifest to confirm `anonymize_refs: true`. See the operator
> repo `docs/training/DIVERGENCE_AND_CORRECTIVE_PLAN_2026-05-29.md`.

This is a draft dataset card for
`underpass-ai/kernel-operator-trajectories`. Do not publish it as final until
the release gate in
[`kernel-tool-operator-publication-plan.md`](../kernel-tool-operator-publication-plan.md)
is complete.

## Dataset Summary

Kernel Operator Trajectories contains audited decision steps for operating
Kernel Memory Protocol tools.

Each row represents one bounded decision:

```text
visible memory state -> one KMP/MCP action
```

The dataset is designed to train and evaluate small specialist models that
operate memory tools. It is not a QA dataset and does not train a model to
answer benchmark questions directly.

## Why This Dataset Exists

KMP exposes memory as navigable state: about scopes, dimensions,
temporal movement, graph relations, evidence, and inspection. A general LLM can
operate those tools, but its tool-use policy can be expensive and inconsistent.

This dataset isolates the narrower problem:

```text
Given the visible memory state, what bounded memory tool action should happen next?
```

## Data Sources

Planned release source:

| Source | Status |
| --- | --- |
| Fresh MemoryArena smart-writer P1.11 run | `<fill after run>` |
| Existing V6 explicit holdout20 run | internal validation only |

Do not use hidden benchmark fields such as gold answers, answer sessions,
future refs, or has-answer flags to build model-facing context.

## Dataset Structure

Each raw trajectory row follows `kernel-operator-trajectory-v1`:

```json
{
  "schema_version": "kernel-operator-trajectory-v1",
  "mode": "read",
  "task_family": "memoryarena.progressive_search",
  "visible_state": {
    "current_ref": "ref_0001",
    "known_refs": ["ref_0001"],
    "last_tool": "kmp_near",
    "last_observed_refs": ["ref_0002"],
    "remaining_budget": {
      "tool_calls": 4,
      "context_chars": 12000
    }
  },
  "allowed_tools": ["kmp_near", "kmp_trace", "kmp_inspect"],
  "target_action": {
    "type": "tool_call",
    "tool": "kmp_inspect",
    "arguments": {
      "ref": "ref_0002"
    }
  }
}
```

The public model-facing files should contain synthetic refs only.

Recommended release files:

| File | Purpose |
| --- | --- |
| `train_model_trajectories.jsonl` | Train split with synthetic refs |
| `eval_model_trajectories.jsonl` | Held-out eval split with synthetic refs |
| `openai_train.jsonl` | Messages-only train data for compatible SFT APIs |
| `openai_eval.jsonl` | Messages-only eval data for compatible SFT APIs |
| `summary.json` | Split and leak-audit summary |

Raw audit files may be kept private if they contain raw refs or tool output.

## Splits

Splits must be grouped by task id or run family. Row-level splits are not
valid for public claims because adjacent tool decisions from the same task can
leak between train and eval.

Fill after the release run:

| Split | Rows | Groups | Notes |
| --- | ---: | ---: | --- |
| Train | `<fill>` | `<fill>` | grouped |
| Eval | `<fill>` | `<fill>` | held out |

## Redaction And Leak Audit

Required checks before publication:

| Check | Required result |
| --- | --- |
| Synthetic model-facing refs | yes |
| Raw benchmark refs in prompts | 0 |
| Target actions in prompts | 0 |
| Observed outcomes in prompts | 0 |
| Gold answers in prompts | 0 |
| Future refs in prompts | 0 |
| Secrets or credentials | 0 |

Any row that cannot be proven safe should be dropped rather than redacted
manually.

## Intended Use

This dataset is intended for:

- training a small KMP/MCP tool operator;
- benchmarking memory tool-use policies;
- comparing deterministic, generalist LLM, and specialist model operators;
- reproducing live replay checks against an KMP deployment.

## Not Intended For

This dataset is not intended for:

- final answer generation;
- benchmark QA training;
- training a general assistant;
- training a model to bypass kernel validation;
- exposing private memory traces.

## Evaluation

A model trained on this dataset should be evaluated with:

- offline policy evaluation;
- invalid JSON/action checks;
- boundedness checks;
- visible-ref checks;
- de-anonymized raw-ref policy eval;
- live MCP/gRPC replay.

The release should include the exact commands and artifact checksums used to
produce the published result.

## License

Apache License 2.0, subject to the same redaction and provenance constraints
documented in the repository.

## Project Links

- Kernel repository: https://github.com/underpass-ai/kmp
- Model: https://huggingface.co/underpass-ai/kernel-tool-operator-small
- Article: https://dev.to/tirsogarcia/building-kernel-memory-protocol-navigable-memory-for-ai-agents-315j
