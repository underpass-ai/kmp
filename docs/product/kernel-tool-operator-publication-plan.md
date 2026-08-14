# Kernel Tool Operator Publication Plan

Date: 2026-05-11
Status: planned publication gate

> **Gate addendum (2026-05-29):** models trained with **un-anonymized refs** do NOT
> meet these publication gates; their claims are invalid until refs are
> re-anonymized and the model is retrained. All v8.x models (through 2026-05-29
> Tier 4) are affected. See the operator repo
> `docs/training/DIVERGENCE_AND_CORRECTIVE_PLAN_2026-05-29.md`.

This document defines when the kernel tool-operator model can be published and
how to package it so the repository, model, and dataset are credible.

The model should not be published just because a local training run looks good.
It should be published only after the operator proves that it can produce
bounded KMP/MCP actions on held-out trajectories and that those actions replay
against a live kernel through the same public adapter an LLM would use.

## Publication Boundary

The operator model is not:

- the kernel;
- a general QA model;
- a reasoning model;
- a benchmark solver;
- a hidden memory API;
- part of kernel core.

It is:

- a small specialist model;
- trained from audited tool-use trajectories;
- constrained to choose one bounded KMP/MCP action at a time;
- evaluated by tool validity, ref validity, boundedness, stop decisions, and
  live replay;
- useful because it can reduce expensive generalist tool-use decisions around
  memory navigation.

Correct public positioning:

> A small specialist model trained to operate KMP memory tools
> through bounded, auditable trajectories.

## Release Gate

The first public Hugging Face release requires all of these:

| Gate | Required result |
| --- | --- |
| Dataset source | Fresh audited MemoryArena smart-writer run larger than the current 100-task corpus |
| Split | Grouped by task id or run family, never by trajectory row |
| Ref hygiene | Model-facing refs are synthetic; raw MemoryArena refs never appear in prompts |
| Leak audit | No benchmark answer, target action, observed outcome, future ref, or raw tool output leak |
| Action contract | Strict KMP/MCP action validator, no additional properties, validator version recorded |
| Offline eval | Zero invalid predictions and zero unbounded calls under the strict validator |
| Live MCP replay | Zero MCP/gRPC failures and zero missing expected refs |
| Baselines | Deterministic baseline reported; generalist LLM baseline reported if cost allows |
| Scope | First release stays read/navigation only unless writer mode has separate proof |
| Documentation | Model card, dataset card, eval summary, limitations, and reproducible commands |

If any gate fails, publish the result as an internal experiment, not as a public
model release.

Current 2026-05-13 rule: the mixed MemoryArena + LongMemEval model remains an
internal baseline. LongMemEval-S cleaned 500 full-history artifacts are
quarantined for release claims until repeated `session_id` semantics are
explicitly modeled by the adapter. The first public release should be
MemoryArena-first unless a later LongMemEval slice passes the same audit gates.

Current 2026-05-14 rule: all Operator metrics generated before the strict
action-contract hardening are `pre-strict` unless revalidated. A prediction is
not valid merely because it has `type`, `tool`, and `arguments`; it must match
the exact KMP/MCP tool schema. See
[`operator-action-contract-audit-2026-05-14.md`](operator-action-contract-audit-2026-05-14.md).

Current 2026-05-14 evidence: MemoryArena V6 holdout20 was regenerated with the
strict predictor, de-anonymized, evaluated offline, and replayed through the
public TLS MCP/gRPC endpoint. It produced 1,124/1,124 exact actions, 0 invalid
actions, 0 unbounded calls, 976/976 successful live tool calls, and 0 missing
expected refs. This is release-grade evidence for the current small holdout,
not yet a public model release: the release still requires a larger fresh
MemoryArena run with the same gate.

The benchmark boundary and next execution steps are tracked in
[`operator-benchmark-status-and-next-steps-2026-05-14.md`](operator-benchmark-status-and-next-steps-2026-05-14.md).
Do not present the MemoryArena Operator result as a LongMemEval multi-session
QA result.

## Hugging Face Artifacts

Recommended namespace and repos:

| Artifact | Repo id | Type |
| --- | --- | --- |
| Operator model | `underpass-ai/kernel-tool-operator-small` | model |
| Trajectory dataset | `underpass-ai/kernel-operator-trajectories` | dataset |
| Optional interactive demo | `underpass-ai/kernel-operator-demo` | Space |
| Optional collection | `underpass-ai/kernel-memory-protocol` | collection |

The model repo should contain:

- model weights or adapter weights;
- base model id;
- training recipe;
- inference contract;
- allowed action schema;
- exact eval command;
- offline policy metrics;
- live MCP replay metrics;
- limitations and non-goals;
- link back to the kernel repo.

The dataset repo should contain:

- train/eval JSONL files with synthetic refs only;
- raw audit artifacts only if they are redacted and safe to publish;
- data generation command;
- split definition;
- leak audit result;
- license and provenance.

Do not publish raw private memory, API outputs containing secrets, hidden
benchmark gold fields, or unrestricted kernel traces.

Draft publication assets live in
[`product/huggingface/`](huggingface/README.md):

- model card template;
- dataset card template;
- release evaluation summary template;
- repository visibility checklist.

## Model Card Skeleton

Required sections:

- `Model Summary`
- `Intended Use`
- `Not Intended For`
- `Input Contract`
- `Output Contract`
- `Training Data`
- `Evaluation`
- `Live MCP Replay`
- `Safety And Redaction`
- `Limitations`
- `How To Reproduce`
- `Citation / Project Links`

Minimum honest claim:

```text
This model predicts bounded Kernel Memory Protocol tool actions from visible
memory-navigation state. It was evaluated on held-out audited trajectories and
then replayed through live MCP/gRPC against a deployed KMP.
```

Claims to avoid:

- "solves memory";
- "replaces RAG";
- "99% agent memory";
- "general reasoning model";
- "autonomous agent";
- "drop-in benchmark winner".

## Repository Visibility

The kernel repository should be made easy to understand before publishing the
model. The goal is not hype; it is reducing friction for people who arrive from
Hugging Face, Dev.to, LinkedIn, or GitHub search.

Repo visibility checklist:

- top README explains KMP / KMP in the first screen;
- quickstart shows one real KMP write/read/trace flow;
- architecture image links to the multidimensional temporal memory article;
- benchmark table separates official scores, local scorecards, reader checks,
  and live replay checks;
- Hugging Face model and dataset links are present only after release;
- Dev.to article is linked as background, not as proof;
- license and author section are visible;
- repository topics include `ai-agents`, `agent-memory`, `mcp`, `grpc`,
  `knowledge-graph`, `rust`, and `llm-tools`;
- release notes point to the exact model/dataset/eval versions;
- open issues label the next contributor-friendly work.

## Release Sequence

1. Implement and validate P1.11.0: bounded pagination, progress, and resume for
   KMP/MCP traversal, remote audit, and live replay.
2. Finish P1.11 larger MemoryArena run.
3. Run local scorecard plus remote audit with bounded pagination/progress
   visible by about/task.
4. Export trajectories with candidate details, including page metadata when a
   KMP/MCP read returns a partial result.
5. Prepare grouped anonymized split.
6. Run prompt leak audit.
7. Train the small operator release candidate, either as adapter weights or
   full weights.
8. Run offline policy eval.
9. De-anonymize predictions.
10. Run live MCP replay against the public TLS endpoint.
11. Produce model card, dataset card, and eval summary.
12. Publish model and dataset as private Hugging Face repos first.
13. Verify downloads and local inference from the published artifacts.
14. Make Hugging Face repos public.
15. Update kernel README, docs index, and article links.
16. Create a GitHub release pointing to the exact HF artifacts.

## CLI Notes

Use the current Hugging Face CLI, `hf`, not the deprecated
`huggingface-cli`.

Recommended commands when the release gate is clean:

```bash
hf auth whoami
hf repos create underpass-ai/kernel-tool-operator-small --type model --private --exist-ok
hf repos create underpass-ai/kernel-operator-trajectories --type dataset --private --exist-ok
hf upload underpass-ai/kernel-tool-operator-small <model-dir> --type model --private
hf upload underpass-ai/kernel-operator-trajectories <dataset-dir> --type dataset --private
```

Only make the repos public after a clean download/replay check.
