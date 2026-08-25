# Incident Report: E2E Ground Truth Penalizes Better Models

> **Historical (superseded).** Point-in-time record; code paths/line citations below were refactored after this was written (ground-truth construction now lives in `crates/rehydration-testkit/src/llm_evaluator.rs`).

**Date**: 2026-03-26
**Severity**: P1 — evaluation results are misleading
**Status**: Resolved — full causal chain shipped

## Observed

Full E2E evaluation matrix (720 evals) shows Qwen3-8B (8B parameters)
outscoring GPT-5.4 and Claude Opus 4.6 (frontier models) on TaskOK:

| Agent | Judge | Task |
|-------|-------|------|
| qwen3-8b | opus-4.6 | **36/36** (100%) |
| gpt-5.4 | opus-4.6 | 22-32/36 (61-89%) |
| opus-4.6 | gpt-5.4 | 21-35/36 (58-97%) |

A small model should not outperform frontier models on causal reasoning.

## Root Cause

The ground truth is constructed wrong. Three specific problems:

### Problem 1: `expected_failure_point` always equals the root node

```rust
// llm_judge_prompt_evaluation.rs:528
let failure_desc = format!("{} — {}", seed.root.node_kind, seed.root.summary);
```

The test assumes the failure point is always the root node of the graph.
But in a causal chain `root → chain-0 → chain-1`, the actual failure point
is the node where things went wrong — often an intermediate node, not the root.

**What happens**: Qwen3-8B, being less capable, defaults to "incident root"
(the obvious answer). Opus 4.6, understanding the causal chain, identifies
`chain-1` as the specific failure node. The judge marks Opus **wrong** because
it doesn't match the root-based ground truth.

**Evidence** from run `2026-03-26_192545`:

```
Qwen3-8B response:  "failure_point": "incident root"          → Task=OK (matches trivial ground truth)
Opus 4.6 response:  "failure_point": "chain-1"                → Task=FAIL (more precise but doesn't match)
```

### Problem 2: `expected_restart_node` is vague

```rust
// llm_judge_prompt_evaluation.rs:529-532
let restart_desc = format!(
    "Any node on the main chain: {} or {}",
    first_chain.map(|n| n.title.as_str()).unwrap_or("first"),
    last_chain.map(|n| n.title.as_str()).unwrap_or("last"),
);
```

"Any node on the main chain" is not a ground truth — it's a hand-wave.
The judge has to interpret this loosely, which makes strict judges fail
everything and lenient judges pass everything:

| Judge prompt | Restart accuracy |
|-------------|-----------------|
| strict-judge + gpt-5.4 | **0/36** (0%) |
| lenient-judge + gpt-5.4 | **36/36** (100%) |

If strict and lenient produce opposite results, the ground truth is the problem.

### Problem 3: `expected_reason` uses only the first relation

```rust
// llm_judge_prompt_evaluation.rs:534
let reason = seed.relations.first().and_then(|r| r.rationale.clone());
```

Only the first relation's rationale is used as expected reason. The causal
chain may have multiple relations with distinct rationales. A model that
cites a deeper rationale (more correct) gets marked as not preserving
the expected reason.

### Problem 4: Opus wraps JSON in markdown backticks

```
Opus response:  ```json\n{ "failure_point": "chain-1", ... }\n```
Qwen response:  { "failure_point": "incident root", ... }
```

The LLM evaluator should strip markdown code fences before parsing.
This is a parsing issue, not a ground truth issue, but it compounds
the scoring bias.

## Impact

- **Paper results are unreliable** — the 100% Task score for Qwen3-8B
  is an artifact of trivial ground truth, not model quality.
- **Cannot compare models** — the ground truth rewards surface-level
  pattern matching over causal reasoning.
- **Judge prompt calibration is blocked** — strict/lenient divergence
  is caused by vague ground truth, not prompt quality.

## Fix Plan

### Ground truth construction (code change in `llm_judge_prompt_evaluation.rs`)

1. **`expected_failure_point`**: Include the full causal chain, not just root.
   Accept any node on the chain as a valid failure point. Specifically:
   the root node, any intermediate chain node, or the leaf. The judge
   should accept semantic matches against any of these.

2. **`expected_restart_node`**: Use the specific causal ancestor of the
   failure point. The dataset generator knows the chain topology — use it
   to produce a precise expected restart node (e.g., `chain-0` for a
   failure at `chain-1`).

3. **`expected_reason`**: Concatenate rationales from all causal relations
   in the chain, not just the first one. The judge should accept a
   paraphrase of any rationale in the chain.

4. **Markdown stripping**: Strip ` ```json ``` ` fences in the LLM evaluator
   before parsing the response JSON.

### Judge prompt (change in `llm_prompts.yaml`)

Update the judge prompt to:
- Accept any node on the causal chain as a valid failure point
- Require causal justification for restart node (not just name matching)
- Accept paraphrases of any chain rationale, not exact match on one

### Validation

After fixing, re-run the full matrix. Expected outcome:
- Frontier models should score equal or higher than Qwen3-8B
- Strict and lenient judges should converge (not 0% vs 100%)
- Explanatory context should still beat structural (this is the real signal)

## Evidence

Run artifacts preserved at:
```
archive/artifacts/e2e-runs/2026-03-26_192545/
├── test.log          (8797 lines)
├── summary.json      (720 evaluations)
├── report.md
└── results/          (720 individual JSONs)
```
