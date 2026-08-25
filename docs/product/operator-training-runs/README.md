# Operator Training Runs

> **Divergence timeline note (2026-05-29):** pre-split runs (2026-05-11..05-15)
> used anonymized refs (V6 validated at 1.000 exact acc). The divergence
> normalized at the 2026-05-20 realistic-v7 plan, hardened 2026-05-22 (v7.3
> teacher corpus), and shipped un-anonymized through v8.1.8 Tier 4 (2026-05-29).
> Anonymization is now mandatory. See
> `operator-experiments/audits/training_divergence_audit_20260529.json` and the
> operator repo `docs/training/DIVERGENCE_AND_CORRECTIVE_PLAN_2026-05-29.md`.

This directory contains one document per serious Operator training attempt.

Use the [operator training run template](../operator-training-run-template.md).

as the source template.

## Rules

- Create the attempt document before training starts.
- Keep failed attempts. Do not rewrite them into success stories.
- Store large artifacts outside the repository, normally under
  `../rehydration-kernel-artifacts/operator/<attempt-id>/`.
- Keep this repo focused on versioned evidence: commands, paths, hashes,
  summaries, decisions, and failure analysis.
- Do not publish or promote a model without a closed attempt document.

## Status Values

| Status | Meaning |
| --- | --- |
| `planned` | Run document exists, execution has not started. |
| `dataset-ready` | Dataset and audits are clean; training has not started. |
| `v2-golden-eval-passed` | A second golden cut passed strict prediction and policy evaluation; still requires generalization tests before promotion. |
| `v4-golden-plus-holdout-passed` | A contrastive golden cut passed both fixed golden eval and independent read-generalization holdout; still requires real KMP/MCP replay before promotion. |
| `v5-real-requested-replay-passed` | The golden read contract plus a real requested benchmark slice passed strict eval, raw de-anonymization, and live KMP/MCP replay against the deployed kernel. |
| `read-profile-passed` | The read profile passed strict offline eval and live read replay; adjacent modes such as writer pre-read may still be diagnostic. |
| `running` | Training/eval is active. |
| `paused` | Execution stopped temporarily and may resume. |
| `failed` | The run produced a negative result or hit a blocker. |
| `quarantined` | Artifacts are retained but must not be used for claims/training. |
| `baseline-only` | Useful comparison, not a release candidate. |
| `internal-only` | Useful for engineering, not external claims. |
| `promoted` | Current baseline or publication candidate. |
| `aborted` | Stopped before useful evidence. |

## Attempt Index

Add rows here as attempts are opened.

| Attempt | Status | Scope | Result |
| --- | --- | --- | --- |
| [2026-05-14-next-operator-training-draft.md](2026-05-14-next-operator-training-draft.md) | internal-only | `operator-read` mixed MemoryArena + conformance | Strong MemoryArena result, but conformance rare API/MCP rows failed; not a public candidate. |
| [2026-05-15-opread-golden-v1.md](2026-05-15-opread-golden-v1.md) | v5-real-requested-replay-passed | `operator-read` golden conformance + real requested P111 replay | V1/V3 were diagnostic; V4 passed golden and holdout; V5 mixed golden v4 with 200 requested P111 rows, reached 55/55 exact offline eval, 43/43 raw P111 exact eval, and 32/32 successful live KMP/MCP tool calls with zero missing expected refs. |
| [2026-05-16-p111-scale-gate.md](2026-05-16-p111-scale-gate.md) | read-profile-passed | MemoryArena P1.11 221-task scale gate + rare read conformance | V3 baseline was not promotable. `read-rare-v1` reached 673/673 strict predictions, 665/665 exact `read` actions, 0 invalid, 0 missing, 0 unbounded, 3/3 trace continuation, and live MCP replay over all 633 real MemoryArena eval read rows with 469/469 successful tool calls. `write_context_read` remains diagnostic at 4/8 exact. |
| [2026-05-16-writer-pre-read-gate.md](2026-05-16-writer-pre-read-gate.md) | dataset-ready | Smart-writer pre-read conformance and SFT gate | Adds `writer-pre-read-v1` and `--capability-split-profile writer-pre-read`. The initial synthetic cut has 8 rows, 100% train/eval capability coverage over 16 writer pre-read capabilities, 0 duplicate model rows, and 0 no-gold audit findings. |
| [2026-05-16-writer-pre-read-p111-mixed.md](2026-05-16-writer-pre-read-p111-mixed.md) | dataset-ready | MemoryArena P1.11 smart-writer pre-read rows plus `writer-pre-read-v1` conformance | Real P1.11 writer pre-read rows cover only 13/16 target capabilities. The compact mixed cut has 207 rows, 100% train/eval writer-pre-read capability coverage, 0 no-gold findings, and a clear diversity gap: only 18 unique model-facing prompts. |
| [2026-05-16-writer-pre-read-v2-diversity-gate.md](2026-05-16-writer-pre-read-v2-diversity-gate.md) | dataset-ready | Expanded writer pre-read conformance plus MemoryArena P1.11 smart-writer pre-read rows | Expands the writer-pre-read profile from 16 to 21 capabilities: stop, stop-sufficient, trace continuation, post-trace state, and ambiguous candidate pools. The synthetic v2 cut has 14/14 unique prompts and 100% train/eval coverage; the mixed compact cut has 213 rows, 24 unique model-facing prompts, 21/21 train/eval coverage, and 0 no-gold findings. |
