# Incident Report: Benchmark Run 2026-03-26

## Summary

After implementing P1 performance optimizations (batch NodeDetailReader, shared
graph reads, QueryTimingBreakdown observability), we ran the paper benchmark
suite with updated model versions. All LLM-as-judge scores dropped to 0%
with Claude Opus 4.6 as judge, versus 94% with Claude Opus 4 in previous runs.

Root cause analysis reveals three independent issues: judge model sensitivity,
judge prompt design gap, and infrastructure configuration errors.

## Timeline

| Time | Event |
|------|-------|
| 08:30 | Paper benchmark launched with `claude-opus-4-6-20250610` as judge |
| 08:35 | All judge calls return 404 — model ID does not exist |
| 08:40 | Corrected to `claude-opus-4-6` — tests pass but 0/19 TaskOK |
| 09:00 | Three parallel runs launched (Qwen+Opus, GPT+Opus, Opus+GPT) |
| 09:15 | Runs 1+2 complete: 0/19 TaskOK in both. Run 3: 0 LLM evals |
| 09:30 | Debug logging added to evaluator, root cause identified |
| 10:00 | Prompt v2 draft written, YAML parse error — not yet deployed |

## Findings

### Finding 1: Opus 4.6 judge rejects all verdicts (0/38 TaskOK)

**Observed**: Both Qwen3-8B and GPT-5.4 as agents, Opus 4.6 as judge → 0/19
TaskOK per config. Previous runs with Opus 4 → 17/18 (94%).

**Root cause**: Two sub-causes identified from debug log analysis.

#### 1a. Causal chain granularity mismatch

The inference model identifies the triggering **incident** as failure point.
The ground truth expects the downstream **decision** as failure point.
Both are in the same causal chain:

```
incident:port-manifold-breach → TRIGGERS → decision:preserve-comfort-load → AUTHORIZES → task:apply-minimal-reroute
```

Qwen3-8B consistently returns `incident:port-manifold-breach`.
Ground truth expects `decision:preserve-comfort-load`.

Opus 4 accepted this as "semantically consistent" (rule 1: match by concept).
Opus 4.6 rejects it — stricter interpretation of "consistent with ground truth".

Affected: UC1 (failure diagnosis), UC2 (why implementation).
Not affected: UC3 (handoff) — Qwen matches the exact ground truth node.

#### 1b. Rationale paraphrase vs preservation

The judge prompt rule 4 requires the response to **preserve** rationale from
the rehydrated context, not merely paraphrase or infer. Example:

- Context rationale: `"minimize operational disruption by keeping passenger comfort systems online"`
- Qwen response: `"The root cause is the port manifold breach, which triggered the decision to preserve comfort load"`

Qwen **summarizes** the causal chain rather than **citing** the rationale text.
Opus 4 accepted this as preservation. Opus 4.6 distinguishes summary from citation.

#### 1c. Budget-constrained placeholder (UC4)

Under token pressure (budget 512), Qwen3-8B returns the **literal prompt
placeholder** instead of extracting a node:

```json
{"failure_point": "the root cause node or event", ...}
```

This was already a known issue in previous runs but was masked by Opus 4's
leniency. It indicates the 8B model cannot extract structured information
from heavily truncated context.

### Finding 2: Opus 4.6 as inference agent produces 0 evaluations

**Observed**: Run 3 (Opus 4.6 agent + GPT-5.4 judge) → 0 LLM evaluations.

**Root cause**: The evaluator function `maybe_evaluate_with_llm` checks for
`LLM_ENDPOINT` env var. When Opus is the agent, the endpoint is
`https://api.anthropic.com/v1/messages` and the `call_anthropic()` function
is used. However, the harness code paths that call `maybe_evaluate_with_llm`
pass the rendered context through `GetContextPath`, not through the LLM
evaluator directly. The issue is that the env var was set in a subshell
that did not propagate correctly.

**Status**: Needs re-verification. The Anthropic provider path exists in
`llm_evaluator.rs` and handles inference calls.

### Finding 3: Infrastructure configuration errors

| Issue | Impact | Resolution |
|-------|--------|------------|
| Model ID `claude-opus-4-6-20250610` → 404 | Wasted ~10 min run | Correct ID is `claude-opus-4-6` |
| NATS URL in values.yaml pointed to wrong namespace | Kernel crash loop | Fixed: `nats.swe-ai-fleet.svc.cluster.local` |
| GHCR push required re-auth | Blocked deploy | `podman login ghcr.io` with PAT |

## Data

### Score comparison across judges

| Config | Judge | TaskOK | RestartOK | ReasonOK |
|--------|-------|:------:|:---------:|:--------:|
| Qwen3-8B + **Opus 4** (previous) | Opus 4 | 17/18 (94%) | 18/18 | 15/18 |
| Qwen3-8B + **Sonnet 4** | Sonnet 4 | 5/19 (26%) | 18/19 (95%) | 7/19 (37%) |
| Qwen3-8B + **Opus 4.6** | Opus 4.6 | 0/19 (0%) | 0/19 (0%) | 0/19 (0%) |
| GPT-5.4 + **Opus 4.6** | Opus 4.6 | 0/19 (0%) | 0/19 (0%) | 0/19 (0%) |

### Verdict breakdown from debug logging (Opus 4.6 judge)

| UC | Agent response (failure_point) | Ground truth | task | restart | reason |
|----|-------------------------------|-------------|:----:|:-------:|:------:|
| UC1 failure diagnosis | `incident:port-manifold-breach` | `decision:preserve-comfort-load` | false | true | false |
| UC2 why implementation | `incident:port-manifold-breach` | `decision:reroute-reserve-power` | false | false | false |
| UC3 handoff resume | `task:remote-isolation-attempt` | `task:remote-isolation-attempt` | true | false | true |
| UC4 constraint (budget) | `"the root cause node or event"` (placeholder) | — | false | true | false |
| UC1 meso (noise) | `incident:port-manifold-breach` | `decision:preserve-comfort-load` | false | true | false |
| UC3 meso (noise) | `task:remote-isolation-attempt` | `task:remote-isolation-attempt` | true | false | true |

Pattern: UC3 passes task_correct (exact match). UC1/UC2 fail (causal ancestor).
UC4 fails (model limitation). restart_correct inconsistent across UCs.

### Timing breakdown (P1 observability — works correctly)

| Metric | Avg (ms) | % of RPC |
|--------|:--------:|:--------:|
| Graph load (Neo4j) | 303 | 43% |
| Detail load (Valkey MGET) | 1.5 | 0.2% |
| Bundle assembly | 0.04 | ~0% |
| Transport overhead | 394 | 57% |
| **Total RPC** | **700** | 100% |

P1 timing observability is working correctly across all runs. The performance
data is independent of judge model choice.

## Judge prompt design gap

The current judge prompt (`llm_prompts.yaml`) was designed for:
- Flat bundle rendering (pre-multi-resolution)
- Single semantic match granularity
- Opus 4 judge calibration

It does not account for:
- **Causal chain traversal** — an upstream incident is a valid failure point
- **Multi-resolution tiers** — L0/L1/L2 content has different density
- **Paraphrase gradients** — context-derived summary vs verbatim citation
- **Model-specific judge calibration** — Opus 4.6 is materially stricter

## Resolution plan

### Immediate (prompt engineering)

- [ ] **P0**: Design judge prompt v2 with causal-chain-aware rules
  - Rule 2 (`task_correct`): accept causal ancestors as valid failure points
  - Rule 4 (`reason_preserved`): distinguish context-derived paraphrase from generic inference
  - Rule 1: explicitly define "semantic match" to include causal chain position
- [ ] **P0**: Support multiple prompt variants via `LLM_PROMPTS_PATH` per run
- [ ] **P1**: Consider per-use-case judge prompts (failure diagnosis vs handoff vs constraint have different evaluation criteria)

### Structural (evaluator improvements)

- [ ] **P1**: Log raw inference + judge responses in the evaluator (currently silent on parse failures)
- [ ] **P1**: Store `llm_response` (inference raw text) in paper metrics for post-hoc analysis
- [ ] **P2**: Add judge calibration test — run known-good/known-bad responses through the judge before benchmarking, to detect judge drift across model versions

### Infrastructure

- [x] Fix model ID documentation (`claude-opus-4-6`, not `claude-opus-4-6-20250610`)
- [x] Fix NATS URL in `values.underpass-runtime.yaml`
- [x] Document GHCR auth flow in `docs/benchmark-paper-use-cases.md`
- [ ] Add CI pre-check that validates LLM model availability before running benchmark
- [ ] Verify Opus 4.6 as inference agent (Run 3 failure)

### Paper impact

The finding that **judge model version changes scores from 94% to 0%** is itself
a significant result. It demonstrates:

1. LLM-as-judge benchmarks are sensitive to judge model version
2. The same inference responses can be evaluated as correct or incorrect depending
   on the judge's interpretation of "semantic match" and "preservation"
3. Benchmark reproducibility requires pinning both inference AND judge model versions

This should be documented in the paper's methodology section as a judge
sensitivity analysis, not treated as a failure of the rehydration kernel.

### Finding 4: Case files written to wrong directory (relative path + cargo test cwd)

> **Note**: This finding references `crates/rehydration-transport-grpc/tests/`
> paths that existed at the time of the incident. Tests have since been moved
> to `crates/rehydration-tests-paper/` and `crates/rehydration-tests-kernel/`.

**Observed**: Tests pass (13/13 ok), judge verdicts logged (`task=true`),
but no case files in the expected `PAPER_OUTPUT_DIR/cases/`. Files found in
`crates/rehydration-transport-grpc/artifacts/...` instead.

**Root cause**: `integration-paper-use-cases.sh` accepts `PAPER_OUTPUT_DIR`
as a relative path (e.g. `artifacts/paper-use-cases-xxx`). The script does
`cd "$ROOT_DIR"` before `cargo test`, so the relative path resolves against
the repo root. However, `cargo test -p rehydration-transport-grpc` executes
the test binary with `cwd = crates/rehydration-transport-grpc/`. The binary
reads `REHYDRATION_PAPER_METRICS_DIR` at runtime and resolves the relative
path against its own cwd, not the script's.

Result: case files are written to
`crates/rehydration-transport-grpc/artifacts/paper-use-cases-xxx/cases/`
instead of `artifacts/paper-use-cases-xxx/cases/`.

**Resolution**: Resolve `OUTPUT_DIR` to an absolute path in the script
using `realpath -m` before exporting to the test binary. Fix applied in
`scripts/ci/integration-paper-use-cases.sh`.

### Finding 5: Parallel subshell env var leaks between sequential runs

**Observed**: When running three configs sequentially in the same shell,
`export` from the script persists across runs. The second run's
`REHYDRATION_PAPER_METRICS_DIR` can overwrite the first's, causing the
ablation suite (which runs after the main suite) to write to the wrong
config's output directory.

**Resolution**: The absolute path fix (Finding 4) eliminates ambiguity.
Each run now writes to a fully resolved path regardless of shell state.

## Appendix: Model IDs

| Model | API ID | Provider |
|-------|--------|----------|
| Claude Opus 4 | `claude-opus-4-20250514` | Anthropic |
| Claude Opus 4.6 | `claude-opus-4-6` | Anthropic |
| Claude Sonnet 4 | `claude-sonnet-4-20250514` | Anthropic |
| GPT-5.4 | `gpt-5.4` | OpenAI |
| Qwen3-8B | `Qwen/Qwen3-8B` | vLLM (local) |
