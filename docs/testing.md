# Testing Guide

Workspace unit tests, container integration tests, benchmark tests, and live
`vLLM` smoke coverage for schema-constrained graph extraction.

## Quick Start

```bash
# Unit tests (no infrastructure needed)
cargo test --workspace

# Integration tests (requires container runtime)
bash scripts/ci/testcontainers-runtime.sh
cargo test -p kmp-tests-kernel --features container-tests -- --nocapture --test-threads=1

# Quality gate (pre-merge: format + clippy + contract + tests)
bash scripts/ci/quality-gate.sh

# Optional local pre-commit hook:
# clippy with the same deny-warnings mode as CI, plus production kernel coverage >= 80%.
# Reference clients are excluded from the production coverage denominator.
git config core.hooksPath .githooks
```

## Unit Tests

Workspace unit coverage across the core crates. No external infrastructure
needed.

| Crate | What it validates |
|:------|:------------------|
| kmp-application | Rendering pipeline, quality metrics, tier classification, mode heuristic |
| kmp-domain | Value objects, invariants, BundleQualityMetrics, aggregate validation |
| kmp-adapter-valkey | RESP protocol, endpoint parsing, TLS config, detail and snapshot stores |
| kmp-testkit | Dataset generator, GraphBatch extraction, retry and repair flows |
| kmp-transport-grpc | gRPC roundtrip, TLS and mTLS handshake, proto mapping |
| kmp-adapter-neo4j | Endpoint parsing, TLS CA config, projection store |
| kmp-proto | Contract stability, fixture compliance, AsyncAPI |
| kmp-config | AppConfig defaults, gRPC TLS modes, NATS TLS validation |
| kmp-observability | OTel, tracing, and composite quality observers |
| kmp-ports | Port trait delegation through Arc |

Run a specific crate:
```bash
cargo test -p kmp-domain
```

## Integration Tests

Require `--features container-tests`. Testcontainers spins up Neo4j, Valkey, NATS automatically.

### Prerequisites

```bash
# Initialize container runtime (docker or podman)
bash scripts/ci/testcontainers-runtime.sh
```

### Test Matrix

| Test | What it validates | Infra | Script |
|------|-------------------|-------|--------|
| `llm_graph_materialization_integration` | GraphBatch materialization: minimal flow + medium incremental flow on the same root aggregate | Neo4j + Valkey + NATS + gRPC | — |
| `pir_graph_batch_integration` | PIR-style GraphBatch integration: idempotent republish and incremental waves on one incident root | Neo4j + Valkey + NATS + gRPC | — |
| `kernel_full_journey_integration` | Projection → query → command full cycle | Neo4j + Valkey + NATS + gRPC | `scripts/ci/integration-kernel-full-journey.sh` |
| `kernel_full_journey_tls_integration` | mTLS end-to-end with generated certs | Same + OpenSSL certs | `scripts/ci/integration-kernel-full-journey-tls.sh` |
| `kernel_golden_integration` | Golden contract tests (4 RPCs) | Neo4j + Valkey + NATS + gRPC | — |
| `agentic_integration` | Agent using kernel context | Neo4j + Valkey + NATS + gRPC | `scripts/ci/integration-agentic-context.sh` |
| `agentic_event_integration` | Event-driven recording runtime | Neo4j + Valkey + NATS + gRPC | `scripts/ci/integration-agentic-event-context.sh` |
| `mcp_real_kernel_integration` | MCP KMP tools reading from the live kernel gRPC server | Neo4j + Valkey + gRPC + MCP adapter | `scripts/ci/integration-mcp-real-kernel.sh` |
| `relation_materialization_integration` | Relation-only async subject adds a PIR-like spine edge across waves without re-materializing the source node | Neo4j + Valkey + NATS + gRPC | — |
| `tier_resolution_integration` | Multi-resolution tiers L0/L1/L2 | Neo4j + Valkey + NATS + gRPC | — |
| `kernel_p1_performance_measurement` | Performance regression (batch vs N+1) | Neo4j + Valkey + NATS + gRPC | — |
| `relationship_use_case_integration` | 4 paper use cases (failure, handoff, constraint, pressure) | Neo4j + Valkey + NATS + gRPC | `scripts/ci/integration-paper-use-cases.sh` |
| `relationship_use_case_ablation_integration` | Ablation: degradation without explanatory relations | Same | Same script |

### Running individual tests

```bash
cargo test -p kmp-tests-kernel \
  --features container-tests \
  --test kernel_full_journey_integration \
  -- --nocapture --test-threads=1
```

### Model-driven ingestion coverage

The repo now validates the `GraphBatch` path in three layers:

1. **Translator/unit invariants** — parse + validate + translate the bounded
   batch shape.
2. **Deterministic container E2E** — materialize GraphBatch fixtures through
   NATS into Neo4j + Valkey, then read them back via gRPC.
3. **Live `vLLM` smoke** — hit the real `vLLM` endpoint with strict schema
   output and prove that the response still parses and translates.

For consumer integration hardening, the repo now adds three PIR-oriented
slices:

1. **Republish idempotency E2E** — same `run_id`, same event ids, same graph shape.
2. **Incremental-wave E2E** — same incident root, new nodes and details over time.
3. **Live roundtrip smoke** — publish fixture to NATS and read it back from a live kernel.

### Relation-Boundary Coverage

Status: initial additive subject implemented; broader convergence coverage is
still pending.

The kernel consumes `graph.relation.materialized` and maps it to a direct
relation projection without re-materializing the source node. The first
container test proves a PIR-like cross-wave relation path through the real NATS
projection runtime, Neo4j, Valkey, and gRPC read path. The required
`integration-conformance` CI job runs that target on every candidate tree.

Coverage state:

| Test | Status | Purpose | Infra |
|------|--------|---------|-------|
| `relation_materialization_integration` | Implemented | Add a relation across waves without re-materializing the source node | Neo4j + Valkey + NATS + gRPC |
| `relation_materialization_out_of_order_integration` | Planned | Prove convergence when a relation arrives before one or both node events | Neo4j + Valkey + NATS + gRPC |
| `pir_sequential_spine_relation_integration` | Planned | Reproduce the full `incident -> finding -> decision -> task -> verification` spine with relation-only waves where needed | Neo4j + Valkey + NATS + gRPC |
| `relation_materialization_idempotency_integration` | Planned | Prove duplicate relation events do not create duplicate edges | Neo4j + Valkey + NATS + gRPC |
| `placeholder_filtering_integration` | Planned | Prove convergence placeholders do not leak into default rendered context | Neo4j + Valkey + NATS + gRPC |
| `graph_relation_roundtrip_smoke` | Planned | Minimal live cluster smoke for one relation-only event | live kernel + live NATS |
| `pir_sequential_spine_roundtrip_smoke` | Planned | Live PIR-like multi-wave smoke validating the full spine end to end | live kernel + live NATS |

Design rule for those tests:

- they should stay contract-oriented
- they should use the same kernel-owned harness style as the current
  roundtrip smokes
- and they should prefer PIR-like stable ids and wave ordering over synthetic
  graph toy cases whenever possible

Suggested homes:

- contract/unit tests:
  `crates/kmp-proto/`, `crates/kmp-adapter-nats/`,
  `crates/kmp-application/`
- container E2E:
  `crates/kmp-tests-kernel/tests/`
- live roundtrip and smoke orchestration:
  `crates/kmp-testkit/tests/`, `crates/kmp-testkit/src/bin/`,
  `e2e/kernel-runner/tests/`

Commands:

```bash
# Local translator + schema invariants
cargo test -p kmp-testkit llm_graph -- --nocapture

# Container E2E: minimal + incremental GraphBatch materialization
cargo test -p kmp-tests-kernel \
  --features container-tests \
  --test llm_graph_materialization_integration \
  -- --nocapture --test-threads=1

# Container E2E: PIR-style republish idempotency + incremental incident waves
cargo test -p kmp-tests-kernel \
  --features container-tests \
  --test pir_graph_batch_integration \
  -- --nocapture --test-threads=1

# Live vLLM smoke (requires endpoint + mTLS env vars)
RUN_VLLM_SMOKE=1 cargo test -p kmp-testkit \
  vllm_graph_prompt_smoke -- --nocapture

# Live blind vLLM smoke: weaker fixture, still bounded GraphBatch contract
RUN_VLLM_BLIND_SMOKE=1 cargo test -p kmp-testkit \
  vllm_graph_blind_prompt_smoke -- --nocapture

# Live blind structural smoke: evaluate graph structure before/after reranker
RUN_VLLM_BLIND_STRUCTURAL_SMOKE=1 cargo test -p kmp-testkit \
  vllm_graph_blind_structural_smoke -- --nocapture

# Optional: exercise the dedicated repair judge too
RUN_VLLM_SMOKE=1 \
LLM_GRAPH_BATCH_USE_REPAIR_JUDGE=1 \
cargo test -p kmp-testkit \
  vllm_graph_prompt_smoke -- --nocapture

# Dedicated live repair smoke: invalid primary output -> repair judge -> valid GraphBatch
RUN_VLLM_SMOKE=1 cargo test -p kmp-testkit \
  vllm_graph_repair_judge_smoke -- --nocapture

# Live PIR-style roundtrip smoke against a real kernel/NATS deployment
RUN_PIR_GRAPH_BATCH_SMOKE=1 cargo test -p kmp-testkit \
  pir_graph_batch_roundtrip_smoke -- --nocapture

# Live PIR incremental consumption smoke: two stable-ID waves -> kernel -> rendered context -> vLLM answer
RUN_PIR_GRAPH_BATCH_CONTEXT_CONSUMPTION_SMOKE=1 cargo test -p kmp-testkit \
  pir_graph_batch_incremental_context_consumption_smoke_succeeds_against_live_kernel -- --nocapture

# Live combined smoke: vLLM -> GraphBatch -> NATS -> projection -> read model
RUN_VLLM_GRAPH_BATCH_ROUNDTRIP_SMOKE=1 cargo test -p kmp-testkit \
  vllm_graph_batch_roundtrip_smoke -- --nocapture

# PIR-style consumption smoke: vLLM -> GraphBatch -> kernel -> rendered context -> vLLM answer
RUN_VLLM_GRAPH_BATCH_CONTEXT_CONSUMPTION_SMOKE=1 cargo test -p kmp-testkit \
  vllm_graph_batch_roundtrip_smoke_consumes_rehydrated_context -- --nocapture

# Larger 15-20 minute soak: same incident, primary + semantic reranker
RUN_VLLM_GRAPH_BATCH_ROUNDTRIP_SOAK=1 \
VLLM_GRAPH_BATCH_ROUNDTRIP_SOAK_ITERATIONS=2 \
cargo test -p kmp-testkit \
  vllm_graph_batch_roundtrip_large_incident_soak -- --nocapture

# Manual equivalent for cluster pods
graph_batch_vllm_request --run-id vllm-live-roundtrip-001 --namespace-node-ids \
  | graph_batch_roundtrip --input - \
      --nats-url nats://kmp-nats:4222 \
      --grpc-endpoint https://127.0.0.1:50054 \
      --run-id vllm-live-roundtrip-001 \
      --grpc-tls-ca-path /var/run/kmp/tls/ca.crt \
      --grpc-tls-cert-path /var/run/kmp/tls/tls.crt \
      --grpc-tls-key-path /var/run/kmp/tls/tls.key \
      --grpc-tls-domain-name kmp \
      --nats-tls-ca-path /var/run/kmp/nats-tls/ca.crt \
      --nats-tls-cert-path /var/run/kmp/nats-tls/tls.crt \
      --nats-tls-key-path /var/run/kmp/nats-tls/tls.key

# Manual large incident variant.
graph_batch_vllm_request --large-incident --run-id vllm-live-large-001 \
  --use-semantic-classifier --namespace-node-ids \
  | graph_batch_roundtrip --input - \
      --nats-url nats://kmp-nats:4222 \
      --grpc-endpoint https://127.0.0.1:50054 \
      --run-id vllm-live-large-001 \
      --grpc-tls-ca-path /var/run/kmp/tls/ca.crt \
      --grpc-tls-cert-path /var/run/kmp/tls/tls.crt \
      --grpc-tls-key-path /var/run/kmp/tls/tls.key \
      --grpc-tls-domain-name kmp \
      --nats-tls-ca-path /var/run/kmp/nats-tls/ca.crt \
      --nats-tls-cert-path /var/run/kmp/nats-tls/tls.crt \
      --nats-tls-key-path /var/run/kmp/nats-tls/tls.key \
      --rehydration-mode reason_preserving \
      --include-rendered-content

# Manual blind extraction variant.
graph_batch_vllm_request --blind --run-id vllm-blind-001
```

### Kubernetes Contract Runner

The legacy [`e2e/Dockerfile`](../e2e/Dockerfile) remains intentionally narrow:
it is a gRPC-only transport probe based on `grpcurl`.

For contract-level cluster tests that need:

- gRPC for synchronous reads
- NATS for asynchronous publish/projection paths
- `graph_batch_roundtrip` for fixture-driven async validation
- `graph_relation_roundtrip` for raw projection-event async validation
- `graph_batch_vllm_request` for model-driven async validation

use the new runner in [`e2e/kernel-runner/`](../e2e/kernel-runner/).

Build the image:

```bash
bash scripts/ci/e2e-kernel-runner-image.sh kmp-e2e-runner:dev
```

Cluster job template:

- [`k8s/kmp-e2e-runner.example.yaml`](../k8s/kmp-e2e-runner.example.yaml)

Supported `TEST_ID` values:

- `sync-grpc-handshake`
- `sync-mtls-enforcement`
- `sync-kernel-memory-service`
- `async-graph-batch-roundtrip`
- `async-graph-relation-roundtrip`
- `async-vllm-spine-ab-comparison`
- `async-vllm-graph-batch-roundtrip`
- `async-vllm-blind-context-consumption`

`sync-mtls-enforcement` is only applicable when `E2E_GRPC_TLS_MODE=mutual`;
for `disabled` and `server` TLS modes it exits as skipped because those modes do
not enforce client certificates.

Example async fixture-driven run inside the cluster:

```bash
TEST_ID=async-graph-batch-roundtrip
PIR_GRAPH_BATCH_NATS_URL=nats://kmp-nats:4222
PIR_GRAPH_BATCH_GRPC_ENDPOINT=https://kmp:50054
PIR_GRAPH_BATCH_GRPC_TLS_CA_PATH=/var/run/kmp/tls/ca.crt
PIR_GRAPH_BATCH_GRPC_TLS_CERT_PATH=/var/run/kmp/tls/tls.crt
PIR_GRAPH_BATCH_GRPC_TLS_KEY_PATH=/var/run/kmp/tls/tls.key
PIR_GRAPH_BATCH_GRPC_TLS_DOMAIN_NAME=kmp
PIR_GRAPH_BATCH_NATS_TLS_CA_PATH=/var/run/kmp/nats-tls/ca.crt
PIR_GRAPH_BATCH_NATS_TLS_CERT_PATH=/var/run/kmp/nats-tls/tls.crt
PIR_GRAPH_BATCH_NATS_TLS_KEY_PATH=/var/run/kmp/nats-tls/tls.key
```

Example raw relation-materialization run inside the cluster:

```bash
TEST_ID=async-graph-relation-roundtrip
GRAPH_RELATION_INPUT=/app/api/examples/kernel/v1beta1/async/pir-sequential-spine.relation-roundtrip.json
PIR_GRAPH_BATCH_NATS_URL=nats://kmp-nats:4222
PIR_GRAPH_BATCH_GRPC_ENDPOINT=https://kmp:50054
PIR_GRAPH_BATCH_GRPC_TLS_CA_PATH=/var/run/kmp/tls/ca.crt
PIR_GRAPH_BATCH_GRPC_TLS_CERT_PATH=/var/run/kmp/tls/tls.crt
PIR_GRAPH_BATCH_GRPC_TLS_KEY_PATH=/var/run/kmp/tls/tls.key
PIR_GRAPH_BATCH_GRPC_TLS_DOMAIN_NAME=kmp
PIR_GRAPH_BATCH_NATS_TLS_CA_PATH=/var/run/kmp/nats-tls/ca.crt
PIR_GRAPH_BATCH_NATS_TLS_CERT_PATH=/var/run/kmp/nats-tls/tls.crt
PIR_GRAPH_BATCH_NATS_TLS_KEY_PATH=/var/run/kmp/nats-tls/tls.key
```

Example legacy-vs-spine A/B comparison with `vLLM` plus a judge:

```bash
TEST_ID=async-vllm-spine-ab-comparison
LEGACY_GRAPH_BATCH_INPUT=/app/api/examples/kernel/v1beta1/async/pir-sequential-spine.legacy-batch.json
GRAPH_RELATION_INPUT=/app/api/examples/kernel/v1beta1/async/pir-sequential-spine.relation-roundtrip.json
PIR_GRAPH_BATCH_INCLUDE_RENDERED_CONTENT=true
LLM_ENDPOINT=http://vllm-qwen35-9b:8000/v1/chat/completions
LLM_MODEL=Qwen/Qwen3.5-9B
LLM_PROVIDER=openai
LLM_ENABLE_THINKING=false
LLM_JUDGE_ENDPOINT=http://vllm-judge:8000/v1/chat/completions
LLM_JUDGE_MODEL=Qwen/Qwen3.5-9B
LLM_JUDGE_PROVIDER=openai
PIR_GRAPH_BATCH_NATS_URL=nats://kmp-nats:4222
PIR_GRAPH_BATCH_GRPC_ENDPOINT=https://kmp:50054
PIR_GRAPH_BATCH_GRPC_TLS_CA_PATH=/var/run/kmp/tls/ca.crt
PIR_GRAPH_BATCH_GRPC_TLS_CERT_PATH=/var/run/kmp/tls/tls.crt
PIR_GRAPH_BATCH_GRPC_TLS_KEY_PATH=/var/run/kmp/tls/tls.key
PIR_GRAPH_BATCH_GRPC_TLS_DOMAIN_NAME=kmp
PIR_GRAPH_BATCH_NATS_TLS_CA_PATH=/var/run/kmp/nats-tls/ca.crt
PIR_GRAPH_BATCH_NATS_TLS_CERT_PATH=/var/run/kmp/nats-tls/tls.crt
PIR_GRAPH_BATCH_NATS_TLS_KEY_PATH=/var/run/kmp/nats-tls/tls.key
```

Example model-driven async run inside the cluster:

```bash
TEST_ID=async-vllm-graph-batch-roundtrip
VLLM_REQUEST_KIND=blind
VLLM_REQUEST_USE_SEMANTIC_CLASSIFIER=true
VLLM_REQUEST_NAMESPACE_NODE_IDS=true
LLM_ENDPOINT=http://vllm-qwen35-9b:8000/v1/chat/completions
LLM_MODEL=Qwen/Qwen3.5-9B
LLM_PROVIDER=openai
LLM_ENABLE_THINKING=false
LLM_SEMANTIC_CLASSIFIER_ENDPOINT=http://vllm-semantic-reranker:8000/score
LLM_SEMANTIC_CLASSIFIER_MODEL=Qwen/Qwen3-Reranker-0.6B
LLM_SEMANTIC_CLASSIFIER_PROVIDER=openai
LLM_SEMANTIC_CLASSIFIER_MODE=score
PIR_GRAPH_BATCH_NATS_URL=nats://kmp-nats:4222
PIR_GRAPH_BATCH_GRPC_ENDPOINT=https://kmp:50054
```

Example model-driven blind context-consumption run inside the cluster:

```bash
TEST_ID=async-vllm-blind-context-consumption
VLLM_REQUEST_KIND=blind
VLLM_REQUEST_USE_SEMANTIC_CLASSIFIER=true
VLLM_REQUEST_NAMESPACE_NODE_IDS=true
PIR_GRAPH_BATCH_INCLUDE_RENDERED_CONTENT=true
PIR_GRAPH_BATCH_REHYDRATION_MODE=reason_preserving
LLM_ENDPOINT=http://vllm-qwen35-9b:8000/v1/chat/completions
LLM_MODEL=Qwen/Qwen3.5-9B
LLM_PROVIDER=openai
LLM_ENABLE_THINKING=false
LLM_SEMANTIC_CLASSIFIER_ENDPOINT=http://vllm-semantic-reranker:8000/score
LLM_SEMANTIC_CLASSIFIER_MODEL=Qwen/Qwen3-Reranker-0.6B
LLM_SEMANTIC_CLASSIFIER_PROVIDER=openai
LLM_SEMANTIC_CLASSIFIER_MODE=score
PIR_GRAPH_BATCH_NATS_URL=nats://kmp-nats:4222
PIR_GRAPH_BATCH_GRPC_ENDPOINT=https://kmp:50054
```

For `underpass-runtime`, the live PIR smoke should use:

- `PIR_GRAPH_BATCH_NATS_URL=nats://kmp-nats:4222`
- mounted NATS TLS certs/keys/CA
- mounted gRPC TLS certs/keys/CA
- no `PIR_GRAPH_BATCH_NATS_TLS_FIRST`

The dedicated `repair-judge` path is experimental:

- it is useful for stabilization and benchmark experiments
- it is not part of the stable kernel contract
- it should not be described as a required runtime dependency for graph ingestion

The live PIR roundtrip smoke uses the `graph_batch_roundtrip` binary as its
single source of truth, so the automated smoke and manual cluster smoke follow
the same publish/query path.

The incremental PIR consumption smoke adds the missing integration proof for a
consumer product: three waves keep the same incident identity, each wave uses
its own `run_id`, the semantic reranker reclassifies relations before publish,
and the final `rendered.content` is fed back to the LLM.

The single-wave live smokes namespace GraphBatch node ids with the test `run_id`
before publishing. This prevents a repeated run from passing against a graph
that was already projected by an earlier attempt.

The blind extraction fixture is intentionally only partially blind. It removes
`Confirmed finding`, `Mitigation decision`, and deterministic non-root node
ids, but it still constrains root identity and graph size so the result stays
bounded and parsable for contract testing. A pass on this blind smoke means the
model can still emit a valid local `GraphBatch` under weaker hints. It does not
mean the model diagnosed the incident autonomously.

The blind structural smoke is the next slice after that. It keeps the same
blind fixture, then scores the emitted graph in two stages:

- primary model output
- reranked output after `semantic_class` reclassification

The scorecard checks:

- root node present
- graph connected from the root
- at least one finding candidate node
- at least one evidence candidate node
- at least one action candidate node
- detail attachment targets
- relation `semantic_class` coverage before and after the reranker

This smoke is intentionally honest about scope. It evaluates kernel-friendly
graph structure and semantic-class usefulness. It still does **not** prove that
the model solved the incident autonomously.

The next blind slice is downstream consumption. It takes that weaker graph,
publishes it through the kernel, requests `rendered_content` with
`reason_preserving`, and then asks the model a non-literal question about cause
and mitigation. A pass on that slice means the weaker graph still carries
enough information for the kernel to support a correct downstream answer. It
still does **not** prove autonomous incident diagnosis.

The incremental PIR smoke uses a different pattern: one stable node namespace
shared by all waves, plus a distinct `run_id` per wave. That mirrors the PIR
contract more closely: stable incident identity, new materialization wave.

The PIR-style consumption smoke now covers both graph growth and graph
correction: `wave 1` seeds the incident, `wave 2` expands it with a new finding
and task, and `wave 3` updates the current incident/task state plus
`node_detail` revision without growing the graph. After the final query it
passes `rendered.content` back to vLLM and asserts that the answer uses the
corrected rehydrated context. It uses `--rehydration-mode reason_preserving`
because this is the PIR LLM-consumption path, not a compact UI preview.

For cluster-managed runs, do not keep these values as ad-hoc shell exports.
Prefer a `ConfigMap` for non-secret LLM client settings and a `Secret` for API
keys, then inject them with `envFrom`. A reference manifest lives at
[`k8s/llm-client-config.example.yaml`](../k8s/llm-client-config.example.yaml).

## Benchmark Tests (LLM-as-Judge)

> **This is the primary empirical validation harness for the kernel.**
> It is the only test that evaluates the kernel's core value proposition
> end-to-end: explanatory relationships improve LLM context quality over
> structural-only edges. Results provide directional evidence — methodology
> refinement is ongoing (see [kernel-roadmap-milestones.md](product/kernel-roadmap-milestones.md)).

Each evaluation calls an LLM for inference (agent) and a second LLM for judging.
The test uses testcontainers to spin up Neo4j + Valkey + NATS + kernel locally,
seeds data with the dataset generator, then runs gRPC queries and evaluates
the rendered context with the LLM pair.

### What runs where

```
Local machine                          External
┌────────────────────┐                 ┌──────────────────┐
│ testcontainers:    │                 │ vLLM (GPU)       │
│   Neo4j            │  agent call     │   Qwen3-8B       │
│   Valkey           │ ───────────────>│                  │
│   NATS             │                 └──────────────────┘
│   kernel (gRPC)    │                 ┌──────────────────┐
│                    │  judge call     │ OpenAI / Anthropic│
│ test binary        │ ───────────────>│   GPT-5.4        │
│   (Rust)           │                 │   Claude Opus 4.6│
└────────────────────┘                 └──────────────────┘
```

### Execution phases

The test runs 5 phases in order. The main API cost is phase 3 (Evaluate), with a small calibration cost in phase 1.

| Phase | What it does | Cost |
|:------|:-------------|:-----|
| **0. Precheck** | Validates YAML structure, API key presence, endpoint connectivity. Fails fast before any containers boot. | Free |
| **1. Calibrate** | Sends known-good and known-bad synthetic responses to each judge. Catches miscalibrated judge/prompt combos before burning real evals. | ~$0.01/judge |
| **2. Capture** | Boots containers, seeds graphs, renders context for all variant cells. Logs `compression_ratio`, `causal_density` per variant. | Free (local) |
| **3. Evaluate** | Runs agent inference + judge verdict for each cell in the filtered matrix. | Main cost |
| **4. Report** | Generates `summary.json` and `report.md` with aggregate tables. | Free |

### Key concepts

**Domains:** Operations and SoftwareDebugging — two independent graph schemas to avoid domain-specific bias.

**Mixes:** `explanatory` (causal/motivational/evidential relations) vs `structural` (only structural edges). This is the independent variable — the kernel's thesis is that explanatory > structural.

**Self-eval exclusion:** When agent and judge are the same model (e.g., opus-4.6 as both), the combo is skipped. 3 agents × 3 judges - 3 self = 6 valid cross-eval combos per prompt/scale/noise cell.

### Configuration

The benchmark is driven by two YAML files:

| File | Purpose |
|:-----|:--------|
| `crates/kmp-testkit/resources/evaluation-matrix.yaml` | **Single source of truth**: agents, judges, prompt variants, scales, noise modes, seeds |
| `crates/kmp-testkit/resources/llm_prompts.yaml` | Default inference + judge prompt templates (v2, causal-chain-aware) |

Both can be overridden at runtime:

| Variable | Default | Description |
|:---------|:--------|:------------|
| `EVAL_MATRIX_PATH` | `resources/evaluation-matrix.yaml` | Path to a custom matrix YAML |
| `LLM_PROMPTS_PATH` | compiled-in `llm_prompts.yaml` | Path to custom prompt templates |

This means you can create a custom YAML for a specific run without editing the defaults:

```bash
cp crates/kmp-testkit/resources/evaluation-matrix.yaml my-run.yaml
# Edit my-run.yaml: change models, scales, seeds...
EVAL_MATRIX_PATH=my-run.yaml cargo test -p kmp-tests-paper \
  --features container-tests --test llm_judge_prompt_evaluation \
  -- --nocapture --test-threads=1
```

The evaluation matrix YAML defines the full combinatorial space:

```yaml
agents:           # Inference models (qwen3-8b, gpt-5.4, opus-4.6)
judges:           # Judge models (opus-4.6, gpt-5.4, sonnet-4.6)
prompts:          # Prompt variants (v1-original, default, citation-agent, strict, lenient)
scales:           # Graph sizes (micro ~500tok, meso ~1500tok, stress ~4000tok)
noise:            # Noise modes (clean, competing, conflicting, restart)
seeds_per_cell: 3 # Distinct graph seeds per variant for variance estimation
```

Total evaluations = agents × judges × prompts × scales × domains × mixes × noise × seeds.
Use filters (below) to run subsets without editing the YAML.

### Tests

| Test | What it validates | Cost |
|:-----|:------------------|:-----|
| `llm_judge_prompt_evaluation` | Full matrix: agents × judges × prompts × scales × noise | ~$15 full, ~$0.10 diagnostic |
| `vllm_benchmark_integration` | vLLM inference quality under budget pressure | GPU time only |

### Filters (subset the matrix)

All filters accept comma-separated values. Names must match keys in `evaluation-matrix.yaml`.

| Variable | Values | Example |
|:---------|:-------|:--------|
| `FILTER_MODELS` | Agent names | `qwen3-8b`, `gpt-5.4,opus-4.6` |
| `FILTER_JUDGES` | Judge names | `sonnet-4.6`, `opus-4.6,gpt-5.4` |
| `FILTER_PROMPTS` | Prompt variant names | `default`, `strict-judge,lenient-judge` |
| `FILTER_SCALES` | Graph scales | `micro`, `meso,stress` |
| `FILTER_NOISE` | Noise modes | `clean`, `competing,conflicting` |

When a filter is empty, all values from the YAML are used.

### Environment variable overrides

The `LLM_*` env vars override the YAML for quick ad-hoc runs. For reproducible
benchmarks, prefer editing `evaluation-matrix.yaml` directly.

**Agent (inference):**

| Variable | Description |
|:---------|:------------|
| `LLM_ENDPOINT` | Inference endpoint (e.g. `https://llm.underpassai.com/v1/chat/completions`) |
| `LLM_MODEL` | Model name (e.g. `Qwen/Qwen3-8B`) |
| `LLM_PROVIDER` | `openai` (OpenAI-compatible, including vLLM), `openai-new` (GPT-5.x), `anthropic` |
| `LLM_TEMPERATURE` | Sampling temperature (recommend `0.0` for reproducibility) |
| `LLM_TLS_CERT_PATH` | Client cert for mTLS endpoints (vLLM) |
| `LLM_TLS_KEY_PATH` | Client key for mTLS endpoints |
| `LLM_ENABLE_THINKING` | `true` to activate Qwen3 chain-of-thought. Requires `--reasoning-parser=qwen3` on vLLM server. With the reasoning parser, thinking tokens go to a separate `reasoning_content` field and `thinking_budget` (512) is independent of `max_tokens` — no token overhead on the content budget. `strip_thinking_tags()` remains as fallback for vLLM servers without the parser. |
| `LLM_TLS_INSECURE` | Deprecated. Unsupported by the current client; use valid TLS certs and mounted secrets instead. |

**Judge:**

| Variable | Description |
|:---------|:------------|
| `LLM_JUDGE_ENDPOINT` | Judge endpoint (e.g. `https://api.openai.com/v1/chat/completions` or `https://vllm-repair-judge.underpassai.com/v1/chat/completions`) |
| `LLM_JUDGE_MODEL` | Judge model (e.g. `claude-sonnet-4-6`, `gpt-5.4`) |
| `LLM_JUDGE_PROVIDER` | `openai` (OpenAI-compatible, including vLLM), `openai-new`, `anthropic` |
| `LLM_JUDGE_API_KEY` | API key for the judge endpoint |

**Semantic classifier:**

| Variable | Description |
|:---------|:------------|
| `LLM_SEMANTIC_CLASSIFIER_ENDPOINT` | Semantic-class classifier endpoint. Use `.../v1/chat/completions` for a small generator or `.../score` for a vLLM reranker. |
| `LLM_SEMANTIC_CLASSIFIER_MODEL` | Semantic-class classifier model (e.g. `Qwen/Qwen3-0.6B` or `Qwen/Qwen3-Reranker-0.6B`) |
| `LLM_SEMANTIC_CLASSIFIER_PROVIDER` | `openai`, `openai-new`, or `anthropic` when the classifier uses chat-style completion |
| `LLM_SEMANTIC_CLASSIFIER_API_KEY` | API key for the classifier endpoint |
| `LLM_SEMANTIC_CLASSIFIER_MODE` | Optional override: `chat` or `score`. If omitted, endpoints ending in `/score` default to `score`. |

**GraphBatch transport policy:**

| Variable | Description |
|:---------|:------------|
| `LLM_GRAPH_BATCH_PRIMARY_MAX_ATTEMPTS` | Max transport/validation attempts for the primary inference request |
| `LLM_GRAPH_BATCH_PRIMARY_CONNECT_TIMEOUT_SECS` | Connect timeout for the primary request |
| `LLM_GRAPH_BATCH_PRIMARY_REQUEST_TIMEOUT_SECS` | Per-request timeout for the primary request |
| `LLM_GRAPH_BATCH_REPAIR_MAX_ATTEMPTS` | Max attempts for the repair-judge request |
| `LLM_GRAPH_BATCH_REPAIR_CONNECT_TIMEOUT_SECS` | Connect timeout for the repair-judge request |
| `LLM_GRAPH_BATCH_REPAIR_REQUEST_TIMEOUT_SECS` | Per-request timeout for the repair-judge request |
| `LLM_GRAPH_BATCH_SEMANTIC_CLASSIFIER_MAX_ATTEMPTS` | Max attempts for the semantic-class classifier pass |
| `LLM_GRAPH_BATCH_SEMANTIC_CLASSIFIER_CONNECT_TIMEOUT_SECS` | Connect timeout for the semantic-class classifier |
| `LLM_GRAPH_BATCH_SEMANTIC_CLASSIFIER_REQUEST_TIMEOUT_SECS` | Per-request timeout for the semantic-class classifier |
| `LLM_GRAPH_BATCH_USE_REPAIR_JUDGE` | `true` to make the live smoke call `request_graph_batch_with_repair_judge` instead of primary-only |
| `LLM_GRAPH_BATCH_USE_SEMANTIC_CLASSIFIER` | `true` to run the semantic-class post-classifier after the primary GraphBatch extraction |

### Cluster-owned LLM config

For shared environments, treat the LLM client configuration as cluster-owned
config, not as shell-local state.

Recommended split:

- `ConfigMap`: `LLM_ENDPOINT`, `LLM_MODEL`, `LLM_PROVIDER`,
  `LLM_TEMPERATURE`, `LLM_ENABLE_THINKING`, `LLM_TLS_CERT_PATH`,
  `LLM_TLS_KEY_PATH`, `LLM_JUDGE_ENDPOINT`, `LLM_JUDGE_MODEL`,
  `LLM_JUDGE_PROVIDER`, `LLM_SEMANTIC_CLASSIFIER_ENDPOINT`,
  `LLM_SEMANTIC_CLASSIFIER_MODEL`, `LLM_SEMANTIC_CLASSIFIER_PROVIDER`,
  `LLM_SEMANTIC_CLASSIFIER_MODE`
- `Secret`: `LLM_API_KEY`, `LLM_JUDGE_API_KEY`,
  `LLM_SEMANTIC_CLASSIFIER_API_KEY`

For in-cluster smoke or benchmark jobs, prefer the in-cluster vLLM service
endpoint, for example `http://vllm-qwen35-9b:8000/v1/chat/completions`. In
that case you usually do not need `LLM_TLS_CERT_PATH` or `LLM_TLS_KEY_PATH`.
Only set those when the client talks to an mTLS-protected endpoint such as the
public ingress.

This repo now supports `envFrom` for both the main chart deployment and the
Helm test Pods:

- `extraEnvFrom` for the kernel deployment
- `e2e.extraEnv` for explicit `secretKeyRef` mappings
- `e2e.extraEnvFrom` for Helm test Pods

That keeps reproducible smoke and benchmark config in Kubernetes rather than
in one developer shell session.

If your existing secret keys do not already match the `LLM_*` names expected by
this repo, map them explicitly in the Helm test Pod values. Example:

```yaml
e2e:
  extraEnvFrom:
    - configMapRef:
        name: kmp-llm-client
  extraEnv:
    - name: LLM_JUDGE_API_KEY
      valueFrom:
        secretKeyRef:
          name: llm-api-keys
          key: openai_api_key
```

For a dedicated repair judge behind `vllm-repair-judge.underpassai.com`, prefer
`LLM_JUDGE_PROVIDER=openai` and start with `LLM_ENABLE_THINKING=false` so the
repair pass stays deterministic and schema-focused.

The primary inference request and the repair judge now have independent timeout
and retry budgets. Keep the primary budget tight for fast failures, and give
the repair judge a longer request timeout when it runs on a slower dedicated
model.

For a dedicated semantic-class reranker behind a vLLM `/score` endpoint, prefer
`LLM_SEMANTIC_CLASSIFIER_MODE=score`. The client also auto-detects `score` when
the classifier endpoint ends in `/score`.

Current code defaults for the testkit transport layer:

| Layer | Connect timeout | Request timeout | Max attempts |
|:------|----------------:|----------------:|-------------:|
| Primary model extraction | 2 s | 45 s | 4 |
| Experimental repair-judge | 2 s | 180 s | 1 |
| Semantic-class post-classifier | 2 s | 20 s | 1 |

**API keys:**

| Variable | Source |
|:---------|:-------|
| `ANTHROPIC_KEY` | `cat /tmp/claude.txt` |
| `OPENAI_KEY` | `cat /tmp/openai.txt` |

### Understanding evaluation counts

Variants are generated as: scales × domains × mixes × noise_per_mix × seeds.
Each variant is then evaluated by each agent × judge × prompt combo.

The noise modes are distributed across mixes to keep the budget flat:
- `explanatory` → clean + competing-causal
- `structural` → clean + conflicting-path
- `mixed` → clean + competing-restart

So each mix has exactly 2 noise conditions. With 3 mixes, 2 domains, and
3 seeds, one scale produces: 3 mixes × 2 noise × 2 domains × 3 seeds = **36 variants**.
Each variant is evaluated once per agent × judge × prompt combo (minus self-eval).

### Model configuration examples

The three models we use in benchmarks, with their exact configuration:

**Qwen3-8B (local vLLM, zero API cost):**

```yaml
# evaluation-matrix.yaml
agents:
  qwen3-8b:
    endpoint: https://llm.underpassai.com/v1/chat/completions
    model: Qwen/Qwen3-8B
    provider: openai        # vLLM serves an OpenAI-compatible API
    tls: true               # mTLS — uses tls.cert and tls.key paths
```

One yaml per model — all include `--reasoning-parser=qwen3`:

| Model | Manifest | GPUs |
|-------|----------|:----:|
| Qwen3-8B | `k8s/vllm-qwen3-8b.yaml` | 1 |
| Qwen3-14B | `k8s/vllm-qwen3-14b.yaml` | 2 |

Qwen3 thinks by default. The reasoning parser separates `<think>` tags into
the `reasoning_content` response field and returns clean content.
Thinking/no-thinking is controlled **client-side** via `LLM_ENABLE_THINKING`:

| `LLM_ENABLE_THINKING` | Behavior |
|:----------------------:|----------|
| unset or `true` | Qwen3 thinks (default). No `chat_template_kwargs` sent. |
| `false` | Thinking disabled. Sends `chat_template_kwargs: {enable_thinking: false}`. |

Operational note:
short DNS or liveness probes against Qwen3-compatible `chat/completions`
endpoints can return `content: null` with `finish_reason: "length"` even when
the service is healthy. That usually means the model spent the small output
budget on reasoning. For short probes, disable thinking explicitly with
`chat_template_kwargs: {enable_thinking: false}`. Reserve thinking mode for
real extraction/evaluation requests with enough `max_tokens`.

Status note:
the public DNS path has been smoke-tested with thinking disabled for short
probes, but thinking mode over that path is not yet validated end-to-end. Do
not assume reasoning is operational there just because `/v1/models` and a short
no-thinking completion succeed. Depending on the vLLM image, parser support,
and runtime flags, additional server-side action may still be required to
enable or stabilize this capability.

Deploying a model:

```bash
kubectl apply -f k8s/vllm-qwen3-8b.yaml
kubectl rollout status deployment/vllm-server -n underpass-runtime --timeout=180s

# Verify
curl --cert /tmp/vllm-client.crt --key /tmp/vllm-client.key -k \
  https://llm.underpassai.com/v1/models
```

`max_tokens` must be large enough for thinking + JSON answer (configured
per agent in `evaluation-matrix.yaml`). Qwen3 model card recommends
`temperature: 0.6` for thinking mode (DO NOT use greedy/0.0).

**GPT-5.4 (OpenAI API):**

```yaml
agents:
  gpt-5.4:
    endpoint: https://api.openai.com/v1/chat/completions
    model: gpt-5.4
    provider: openai-new    # uses max_completion_tokens instead of max_tokens
    api_key_env: OPENAI_KEY
```

No TLS certs needed — auth is via bearer token from `OPENAI_KEY` env var.

**Claude Opus 4.6 (Anthropic API):**

```yaml
agents:
  opus-4.6:
    endpoint: https://api.anthropic.com/v1/messages
    model: claude-opus-4-6
    provider: anthropic     # uses Anthropic message format, not OpenAI
    api_key_env: ANTHROPIC_KEY
```

**Judges** follow the same schema. Any model can be agent or judge:

```yaml
judges:
  sonnet-4.6:
    endpoint: https://api.anthropic.com/v1/messages
    model: claude-sonnet-4-6
    provider: anthropic
    api_key_env: ANTHROPIC_KEY
```

**Adding a new vLLM model** (e.g. Qwen3-4B):

```yaml
agents:
  qwen3-4b:
    endpoint: https://llm.underpassai.com/v1/chat/completions
    model: Qwen/Qwen3-4B   # must match the --model arg in the k8s manifest
    provider: openai
    tls: true
```

Deploy the model with `--reasoning-parser=qwen3` for clean thinking separation.
The precheck validates that the endpoint is reachable and the model responds
before any container starts or any API dollar is spent.

### Run examples

All examples use the YAML matrix with filters — no env var overrides needed.

**Minimum smoke** — 1 agent, 1 judge, micro only, clean noise (~$0.01):

```bash
export OPENAI_KEY="$(cat /tmp/openai.txt)"

FILTER_MODELS="qwen3-8b" \
FILTER_JUDGES="gpt-5.4" \
FILTER_PROMPTS="default" \
FILTER_SCALES="micro" \
FILTER_NOISE="clean" \
bash -c '. scripts/ci/testcontainers-runtime.sh 2>/dev/null && \
cargo test -p kmp-tests-paper \
  --features container-tests \
  --test llm_judge_prompt_evaluation \
  -- --nocapture --test-threads=1'
```

Uses [`crates/kmp-testkit/resources/evaluation-matrix.smoke.yaml`](../crates/kmp-testkit/resources/evaluation-matrix.smoke.yaml).

6 evals: 1 scale × 2 domains × 3 mixes × 1 noise (clean) × 1 seed. This is
the smallest config-only run that still exercises the full harness path.

**Demonstrative run** — 1 agent, 1 judge, all noise, micro + meso (~$0.10):

```bash
export ANTHROPIC_KEY="$(cat /tmp/claude.txt)"

FILTER_MODELS="qwen3-8b" \
FILTER_JUDGES="sonnet-4.6" \
FILTER_PROMPTS="default" \
FILTER_SCALES="micro,meso" \
bash -c '. scripts/ci/testcontainers-runtime.sh 2>/dev/null && \
cargo test -p kmp-tests-paper \
  --features container-tests \
  --test llm_judge_prompt_evaluation \
  -- --nocapture --test-threads=1'
```

72 evals: 2 scales × 2 domains × 3 mixes × 2 noise × 3 seeds. Enough to
show the explanatory vs structural gap across noise conditions and graph sizes.

**Cross-provider comparison** — all agents, fixed judge, default prompt (~$2):

```bash
export ANTHROPIC_KEY="$(cat /tmp/claude.txt)"
export OPENAI_KEY="$(cat /tmp/openai.txt)"

FILTER_JUDGES="sonnet-4.6" \
FILTER_PROMPTS="default" \
FILTER_SCALES="micro,meso" \
bash -c '. scripts/ci/testcontainers-runtime.sh 2>/dev/null && \
cargo test -p kmp-tests-paper \
  --features container-tests \
  --test llm_judge_prompt_evaluation \
  -- --nocapture --test-threads=1'
```

216 evals: 3 agents × 72 variants. Compares qwen3-8b, gpt-5.4, opus-4.6
under a fixed judge for balanced agent comparison claims.

**Ground truth diagnostic** — validates that ground truth + judge produce
consistent scores before a full run:

```bash
export ANTHROPIC_KEY="$(cat /tmp/claude.txt)"
export OPENAI_KEY="$(cat /tmp/openai.txt)"
bash scripts/ci/e2e-ground-truth-diagnostic.sh
```

**Full matrix** — all agents × all judges × all prompts × all scales × all noise:

```bash
export ANTHROPIC_KEY="$(cat /tmp/claude.txt)"
export OPENAI_KEY="$(cat /tmp/openai.txt)"
bash scripts/ci/evaluate-prompt-variants.sh
```

The script also accepts a custom matrix YAML as argument:

```bash
bash scripts/ci/evaluate-prompt-variants.sh my-run.yaml
```

### Viewing logs

The test writes to stdout with `--nocapture`. Key log lines to watch:

```
[PRECHECK]  — API connectivity and key validation (before any evals)
[CALIBRATE] — Judge calibration with known-good/known-bad responses
[CAPTURE]   — Per-variant: tokens, compression ratio, causal density
[EVAL]      — Agent inference + judge verdict per evaluation
[RESULT]    — Summary table with TaskOK / RestartOK / ReasonPreserved
```

If running in background, check the output file:

```bash
tail -f /path/to/output.log | grep -E '\[RESULT\]|\[EVAL\]|\[CAPTURE\]'
```

### Output artifacts

Results are saved to `artifacts/e2e-runs/<timestamp>/`:

```
artifacts/e2e-runs/<timestamp>/
├── results/           # One JSON per evaluation
│   ├── micro-ops-explanatory-seed0.json
│   ├── micro-ops-structural-seed0.json
│   └── ...
├── test.log           # Full test output
└── summary.json       # Aggregated results
```

Each result JSON contains:
- **Quality** (from kernel domain): `rendered_tokens`, `raw_equivalent_tokens`, `compression_ratio`, `causal_density`, `noise_ratio`, `detail_coverage`
- **Planner**: `resolved_mode` (reason_preserving or resume_focused)
- **Tiers**: `tier_l0_tokens`, `tier_l1_tokens`, `tier_l2_tokens`, `tier_total_tokens`
- **Timing**: `graph_load_ms`, `detail_load_ms`, `bundle_assembly_ms`, `timing_batch_size`
- **Truncation**: `truncation_budget`, `truncation_used`, `truncation_sections_dropped`
- **LLM verdicts**: `task_correct`, `restart_exact`, `reason_correct`
- **Anti-fabrication**: `llm_reason_source` (graph_metadata/inferred/not_available), `llm_confidence` (high/medium/low), `llm_reason_fabricated` (deterministic detection)
- **Token cost**: `llm_prompt_tokens`, `llm_completion_tokens`
- **Raw responses**: `agent_response`, `judge_raw`

These metrics come from the **kernel response** (`rendered.quality` in the
proto), not computed by the test. Single source of truth.

### Budget pressure testing

Use `BENCHMARK_TOKEN_BUDGET` to exercise the planner under token pressure:

```bash
BENCHMARK_TOKEN_BUDGET=512 EVAL_MATRIX_PATH=pressure-test.yaml \
cargo test -p kmp-tests-paper --features container-tests \
  --test vllm_benchmark_integration -- --nocapture --test-threads=1
```

At 512 tokens with stress-scale graphs (49 nodes), the planner activates:
- `causal_density >= 50%` → keeps ReasonPreserving (rationale preserved)
- `causal_density < 50%` → switches to ResumeFocused (L2 pruned, 7-12x compression)

### Troubleshooting

| Symptom | Cause | Fix |
|:--------|:------|:----|
| `[PRECHECK] FAIL: missing ANTHROPIC_KEY` | API key env var not set | `export ANTHROPIC_KEY="$(cat /tmp/claude.txt)"` |
| `[PRECHECK] FAIL: endpoint unreachable` | vLLM server down or TLS cert expired | Check `kubectl get pods` and cert expiry |
| `[CALIBRATE] FAIL: known-good scored false` | Judge/prompt combo miscalibrated | Try a different judge or prompt variant |
| `[EVAL] parse error: invalid JSON` | Judge returned markdown instead of JSON | The test strips fences automatically; if persistent, check `judge_max_tokens` |
| Container startup timeout | Docker/Podman not running | `bash scripts/ci/testcontainers-runtime.sh` |
| `two different versions of crate async_nats` | Dependency version conflict | `cargo clean && cargo check --all-features` |

## Adapter Integration Tests

Isolated tests for individual adapters:

```bash
cargo test -p kmp-adapter-neo4j --features container-tests --test neo4j_integration
cargo test -p kmp-adapter-valkey --features container-tests --test valkey_integration
```

## CI Scripts Reference

| Script | Purpose |
|--------|---------|
| `scripts/ci/quality-gate.sh` | Pre-merge: format + clippy + contract + tests |
| `scripts/ci/contract-gate.sh` | Protobuf lint + breaking change detection |
| `scripts/ci/helm-lint.sh` | Helm chart validation (8 scenarios) |
| `scripts/ci/rust-coverage.sh` | LLVM coverage report including container tests |
| `scripts/ci/kubernetes-transport-smoke.sh` | In-cluster TLS/mTLS smoke test (gRPC + NATS + Valkey + OTel + Loki) |
| `scripts/ci/container-image.sh` | Build OCI container image |
| `scripts/ci/deploy-kubernetes.sh` | Helm deploy wrapper |

## Adding a New Test

Integration tests use the `TestFixture` builder pattern:

```rust
let fixture = TestFixture::builder()
    .with_neo4j()
    .with_valkey()
    .with_nats()
    .with_projection_runtime()
    .with_grpc_server()
    .with_seed(ClosureSeed::new(|ctx| {
        let client = ctx.nats_client().clone();
        Box::pin(async move { publish_events(&client).await })
    }))
    .with_readiness_check("root-node-id", "expected-node-id")
    .build()
    .await?;

let mut client = fixture.query_client();
// ... gRPC calls ...
fixture.shutdown().await?;
```
