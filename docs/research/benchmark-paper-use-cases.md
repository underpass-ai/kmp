# E2E Evaluation Matrix

The main E2E benchmark for the Rehydration Kernel.

## Why this test exists

This test answers five questions:

1. **Does rehydration actually help?** Compares explanatory (full causal
   metadata) vs structural (relations only) vs detail-only contexts. If the
   LLM scores higher with explanatory context, the rehydration pipeline is
   adding real value. If it doesn't, something is broken.

2. **Do small models benefit as much as large ones?** Runs the same contexts
   through Qwen3-8B (local vLLM), GPT-5.4, and Claude Opus 4.6. A small
   model that scores well on explanatory context proves the kernel compensates
   for model capacity — the graph does the reasoning, not the model.

3. **Is the judge prompt reliable?** Runs 5 prompt variants (v1-original,
   v2-causal, citation-agent, strict-judge, lenient-judge) across all
   model+context combinations. If strict and lenient judges agree, the
   verdicts are robust. If they diverge, the prompt needs calibration.

4. **Do new features improve or regress quality?** Every run produces
   timestamped evidence. Compare run A (before feature) vs run B (after
   feature) at the same scale/domain/mix. If scores drop, the feature
   introduced a regression. If scores hold, it's safe to merge.

5. **Is the test infrastructure itself trustworthy?** The matrix covers
   36 graph variants (3 scales x 2 domains x 3 mixes x 2 noise modes).
   Consistent results across variants prove the dataset generator,
   seed publisher, projection runtime, and renderer are all working
   correctly end-to-end. A single broken variant exposes the bug.

## What you need on your machine

The test raises its own infrastructure (Neo4j, Valkey, NATS) via containers.
You do **not** need a Kubernetes cluster. You need:

### Required (the test will not start without these)

| Dependency | What it is | How to verify |
|-----------|------------|---------------|
| **Rust 1.97+** | Compiler | `rustc --version` |
| **Docker or Podman** | Container runtime for Neo4j, Valkey, NATS | `docker info` or `podman info` |
| **ANTHROPIC_KEY** | API key for Claude (agent + judge) | `echo $ANTHROPIC_KEY` |
| **OPENAI_KEY** | API key for GPT-5.x (agent + judge) | `echo $OPENAI_KEY` |
| **vLLM TLS certs** | mTLS client cert+key for the vLLM endpoint | `ls /tmp/vllm-client.crt /tmp/vllm-client.key` |
| **Network access** | To `api.anthropic.com`, `api.openai.com`, `llm.underpassai.com` | Precheck verifies this |

### Not required (the test provides these)

| Component | How it is provided |
|-----------|--------------------|
| Neo4j | Testcontainer — ephemeral, auto-created, auto-destroyed |
| Valkey (Redis) | Testcontainer — ephemeral, auto-created, auto-destroyed |
| NATS (JetStream) | Testcontainer — ephemeral, auto-created, auto-destroyed |
| Rehydration Kernel | In-process gRPC server on random port |
| Projection runtime | In-process NATS consumer |

The test boots **all kernel infrastructure locally**. The only external
services are the LLM APIs (inference + judge) configured in the YAML.

### Quick setup

```bash
# 1. API keys
export ANTHROPIC_KEY="$(cat /path/to/anthropic-key.txt)"
export OPENAI_KEY="$(cat /path/to/openai-key.txt)"

# 2. vLLM TLS certs (first time — extract from k8s)
kubectl get secret vllm-client-cert -n underpass-runtime \
  -o jsonpath='{.data.tls\.crt}' | base64 -d > /tmp/vllm-client.crt
kubectl get secret vllm-client-cert -n underpass-runtime \
  -o jsonpath='{.data.tls\.key}' | base64 -d > /tmp/vllm-client.key

# 3. Run (precheck validates everything before booting containers)
cargo test -p rehydration-tests-paper --features container-tests \
  --test llm_judge_prompt_evaluation -- --nocapture --test-threads=1
```

If you are missing anything, the precheck will tell you exactly what and how
to fix it before any container starts.

## Architecture

```
                         evaluation-matrix.yaml
                                  |
                     (single source of truth)
                                  |
                    +-------------+-------------+
                    |                           |
            evaluate-prompt-           llm_judge_prompt_
            variants.sh                evaluation.rs
            (orchestrator)             (test binary)
                    |                           |
                    |    cargo test per cell     |
                    +----------->---------------+
                                                |
                          +---------+-----------+-----------+
                          |         |           |           |
                       Precheck   Phase 1     Phase 2     Phase 3
                       (strict)   (capture)   (evaluate)  (report)
                          |         |           |           |
                          v         v           v           v
                       Validate  Testcontainers  LLM APIs   Run dir
                       YAML,     (local infra)   (remote)   (artifacts)
                       keys,
                       certs,
                       endpoints
```

### Infrastructure raised by the test (Phase 1)

```
  Your machine (Docker / Podman)
  +------------------------------------------------------------------+
  |                                                                  |
  |  testcontainers (ephemeral, auto-destroyed)                      |
  |  +------------------+  +----------------+  +-----------------+   |
  |  |  Neo4j 5.26      |  |  Valkey 8.1    |  |  NATS 2.10      |   |
  |  |  :random_port    |  |  :random_port  |  |  :random_port   |   |
  |  |  (graph store)   |  |  (detail +     |  |  (projection    |   |
  |  |                  |  |   snapshot)    |  |   events + JS)  |   |
  |  +--------+---------+  +-------+--------+  +--------+--------+   |
  |           |                    |                     |            |
  |           +--------------------+---------------------+            |
  |                                |                                  |
  |                   +------------+------------+                     |
  |                   |   Rehydration Kernel    |                     |
  |                   |   (gRPC server on       |                     |
  |                   |    random port)         |                     |
  |                   |                         |                     |
  |                   |  - ProjectionRuntime    |                     |
  |                   |    (NATS consumer)      |                     |
  |                   |  - QueryService         |                     |
  |                   |  - CommandService        |                     |
  |                   +------------+------------+                     |
  |                                |                                  |
  +------------------------------------------------------------------+
                                   |
              Phase 1: seed graphs via NATS, query context via gRPC
                                   |
                                   v
                          Captured rendered contexts
                          (36 variants in memory)
                                   |
              Phase 2: send to remote LLM APIs
                                   |
              +--------------------+--------------------+
              |                    |                    |
    +---------v--------+ +--------v---------+ +--------v---------+
    |  vLLM (Qwen3-8B) | |  OpenAI (GPT-5.4)| | Anthropic (Opus) |
    |  mTLS ingress    | |  API key auth    | | API key auth     |
    |  (inference)     | |  (inference)     | | (inference)      |
    +------------------+ +------------------+ +------------------+
              |                    |                    |
              +--------------------+--------------------+
                                   |
              Phase 2: send LLM response + ground truth to judge
                                   |
              +--------------------+--------------------+
              |                                        |
    +---------v-----------+              +-------------v-----------+
    |  Anthropic (Opus)   |              |  OpenAI (GPT-5.4)      |
    |  (judge)            |              |  (judge)               |
    +---------------------+              +------------------------+
              |                                        |
              +--------------------+-------------------+
                                   |
              Phase 3: write results to timestamped run directory
                                   |
                                   v
                    artifacts/e2e-runs/YYYY-MM-DD_HHMMSS/
                    +-- test.log          (full stderr log)
                    +-- summary.json      (all results)
                    +-- report.md         (markdown table)
                    +-- results/
                        +-- 0001_qwen3-8b_opus-4.6_default_micro-ops-explanatory-clean.json
                        +-- 0002_...
```

## Single source of truth: `evaluation-matrix.yaml`

Location: `crates/rehydration-testkit/resources/evaluation-matrix.yaml`

Everything is configured here. The test reads this file and fails hard if
anything is missing or unreachable.

```yaml
# API keys — names of env vars, not values
keys:
  anthropic_env: ANTHROPIC_KEY
  openai_env: OPENAI_KEY

# TLS client certs — filesystem paths, not env vars
tls:
  cert: /tmp/vllm-client.crt
  key: /tmp/vllm-client.key

agents:
  qwen3-8b:
    endpoint: https://llm.underpassai.com/v1/chat/completions
    model: Qwen/Qwen3-8B
    provider: openai        # openai | openai-new | anthropic
    tls: true               # uses tls.cert and tls.key above

  gpt-5.4:
    endpoint: https://api.openai.com/v1/chat/completions
    model: gpt-5.4
    provider: openai-new
    api_key_env: OPENAI_KEY

  opus-4.6:
    endpoint: https://api.anthropic.com/v1/messages
    model: claude-opus-4-6
    provider: anthropic
    api_key_env: ANTHROPIC_KEY

judges:
  opus-4.6:
    endpoint: https://api.anthropic.com/v1/messages
    model: claude-opus-4-6
    provider: anthropic
    api_key_env: ANTHROPIC_KEY

  gpt-5.4:
    endpoint: https://api.openai.com/v1/chat/completions
    model: gpt-5.4
    provider: openai-new
    api_key_env: OPENAI_KEY

  sonnet-4.6:
    endpoint: https://api.anthropic.com/v1/messages
    model: claude-sonnet-4-6
    provider: anthropic
    api_key_env: ANTHROPIC_KEY

prompts:
  v1-original: prompt-variants/v1-original-judge.yaml
  default: null           # compiled-in v2 causal-aware prompt
  citation-agent: prompt-variants/citation-agent.yaml
  strict-judge: prompt-variants/strict-judge.yaml
  lenient-judge: prompt-variants/lenient-judge.yaml

scales:
  micro: micro            # 4 nodes, ~200-500 tokens
  meso: meso              # 21 nodes, ~1500 tokens
  stress: stress          # 49 nodes, ~4000 tokens

seeds_per_cell: 3         # distinct graph seeds per variant cell

noise:
  clean: structural               # baseline: irrelevant structural nodes
  competing: competing-causal     # plausible alternative rationale
  conflicting: conflicting-path   # contradicts main chain reasoning
  restart: competing-restart      # alternative recovery point
```

Noise is **distributed per relation mix** (not a free cross-product): each
mix pairs `clean` with one adversarial mode — `explanatory`/`competing`,
`structural`/`conflicting`, `mixed`/`restart` — so every cell yields two
noise modes.

### What the YAML controls

| Section | What it configures |
|---------|--------------------|
| `keys` | Names of env vars holding API keys (not the keys themselves) |
| `tls` | Filesystem paths to mTLS client cert and key |
| `agents` | Inference model endpoints, models, providers, auth method |
| `judges` | Judge model endpoints, models, providers, auth method |
| `prompts` | Prompt variant files (relative to `resources/`) |
| `scales` | Graph sizes for context capture |
| `noise` | Noise modes for distractor injection |

### What the test hardcodes (in code, not configurable)

| What | Value | Why |
|------|-------|-----|
| Domains | `ops`, `debug` | Two representative domains |
| Relation mixes | `explanatory`, `structural`, `mixed` | Three ablation variants |
| Self-judge skip | agent name == judge name | Avoids circular evaluation |
| Kernel context budget | 4096 | `GetContextRequest.token_budget` (override via `BENCHMARK_TOKEN_BUDGET`) |
| LLM inference `max_tokens` | 200 | Per-call answer budget — forced in the eval path, not read from YAML |
| Temperature | 0.0 | Deterministic inference — forced in the eval path, ignores per-agent YAML |
| Readiness poll | 40 attempts x 200ms | Wait for projection to populate |

## Precheck

Before booting any container, the test validates **every** dependency.
If anything fails, the test panics with a clear error message listing
exactly what is wrong and how to fix it.

The precheck validates:

1. `evaluation-matrix.yaml` exists and parses correctly
2. Every agent has `endpoint`, `model`, and valid `provider`
3. Every judge has `endpoint`, `model`, and valid `provider`
4. Every `api_key_env` resolves to a non-empty env var
5. If any agent has `tls: true`, the YAML has `tls.cert` and `tls.key`
6. TLS cert and key files exist on disk
7. Every endpoint is reachable (real HTTP request with auth)
8. Every prompt variant file exists on disk
9. Docker or Podman is available
10. Subset filters are reported (informational)

### Example precheck output (success)

```
  ✔ evaluation-matrix.yaml loaded
  ✔ tls section: cert=/tmp/vllm-client.crt, key=/tmp/vllm-client.key
  ✔ 3 agents configured
  ✔ agent 'qwen3-8b': TLS certs present
  ✔ agent 'qwen3-8b': TLS endpoint reachable
  ✔ agent 'gpt-5.4': OPENAI_KEY set
  ✔ agent 'gpt-5.4': endpoint reachable
  ✔ agent 'opus-4.6': ANTHROPIC_KEY set
  ✔ agent 'opus-4.6': endpoint reachable
  ✔ 2 judges configured
  ✔ judge 'opus-4.6': ANTHROPIC_KEY set
  ✔ judge 'opus-4.6': endpoint reachable
  ✔ judge 'gpt-5.4': OPENAI_KEY set
  ✔ judge 'gpt-5.4': endpoint reachable
  ✔ prompt 'v1-original': prompt-variants/v1-original-judge.yaml
  ✔ prompt 'citation-agent': prompt-variants/citation-agent.yaml
  ✔ prompt 'strict-judge': prompt-variants/strict-judge.yaml
  ✔ prompt 'lenient-judge': prompt-variants/lenient-judge.yaml
  ✔ container runtime: podman
```

### Example precheck output (failure)

```
  ======================================================================
    PRECHECK FAILED — fix the issues below before running
  ======================================================================

  ✘ agent 'qwen3-8b': TLS cert not found at /tmp/vllm-client.crt (tls.cert in YAML)
  ✘ agent 'gpt-5.4': env var OPENAI_KEY not set
  ✘ judge 'gpt-5.4': env var OPENAI_KEY not set

  thread 'judge_prompt_evaluation_across_all_use_cases' panicked at 'precheck failed: 3 error(s)'
```

## How to run

### Step 1: Set API keys

```bash
export ANTHROPIC_KEY="$(cat /path/to/anthropic-key.txt)"
export OPENAI_KEY="$(cat /path/to/openai-key.txt)"
```

### Step 2: Extract vLLM TLS certs (first time only)

```bash
kubectl get secret vllm-client-cert -n underpass-runtime \
  -o jsonpath='{.data.tls\.crt}' | base64 -d > /tmp/vllm-client.crt
kubectl get secret vllm-client-cert -n underpass-runtime \
  -o jsonpath='{.data.tls\.key}' | base64 -d > /tmp/vllm-client.key
```

The cert paths are configured in `evaluation-matrix.yaml` under `tls.cert`
and `tls.key`. Change them there if you store certs elsewhere.

### Step 3: Run

**Full matrix (all agents x judges x prompts):**

```bash
cargo test -p rehydration-tests-paper --features container-tests \
  --test llm_judge_prompt_evaluation -- --nocapture --test-threads=1
```

**Via orchestrator (one process per cell):**

```bash
bash scripts/ci/evaluate-prompt-variants.sh
```

**Subset (filter by agent, prompt, scale, noise, judge):**

```bash
FILTER_MODELS="qwen3-8b" \
FILTER_PROMPTS="default" \
FILTER_SCALES="micro" \
  cargo test -p rehydration-tests-paper --features container-tests \
    --test llm_judge_prompt_evaluation -- --nocapture --test-threads=1
```

All filter env vars accept comma-separated values. Omit a filter to include
all values for that dimension.

| Filter env var | Values |
|----------------|--------|
| `FILTER_MODELS` | `qwen3-8b`, `gpt-5.4`, `opus-4.6` |
| `FILTER_PROMPTS` | `v1-original`, `default`, `citation-agent`, `strict-judge`, `lenient-judge` |
| `FILTER_SCALES` | `micro`, `meso`, `stress` |
| `FILTER_NOISE` | `clean`, `competing`, `conflicting`, `restart` |
| `FILTER_JUDGES` | `opus-4.6`, `gpt-5.4`, `sonnet-4.6` |

### Step 4: Check output

Results are written to a timestamped directory:

```
artifacts/e2e-runs/2026-03-26_201530/
├── test.log                 # full stderr log (every eval, every response)
├── summary.json             # all results as JSON array
├── report.md                # markdown summary table + aggregates
└── results/                 # one JSON per evaluation
    ├── 0001_qwen3-8b_opus-4.6_default_micro-ops-explanatory-clean.json
    ├── 0002_qwen3-8b_opus-4.6_default_micro-ops-structural-clean.json
    └── ...
```

Override the base directory:

```bash
E2E_OUTPUT_DIR=/custom/path cargo test ...
```

## Evaluation dimensions

The full matrix produces:

- **3 scales** x **2 domains** x **3 mixes** x **2 noise/mix** x **3 seeds** (`seeds_per_cell`) = **108 captured variants** per boot
- **3 agents** x **3 judges** (minus 2 self-judge) = **7 agent-judge pairs**
- **5 prompt variants**
- Total: 108 x 7 x 5 = **3780 evaluations** in the full matrix

These counts are derived from `evaluation-matrix.yaml` (`seeds_per_cell`,
`judges`) and the per-mix noise distribution — change the YAML and the
matrix size changes with it.

Each evaluation measures:

| Metric | Source | What it measures |
|--------|--------|-----------------|
| `task` (TaskOK) | Judge | Did the LLM correctly identify the failure point? |
| `restart` (RestartOK) | Judge | Did the LLM identify the correct restart node with causal justification? |
| `reason` (ReasonOK) | Judge | Was the rationale preserved from context (not inferred)? |
| `latency_ms` | Timer | Combined inference + judge latency |
| `llm_reason_source` | Evaluator (deterministic) | What source did the model declare? (`graph_metadata` / `inferred` / `not_available`) |
| `llm_confidence` | Evaluator (deterministic) | Self-reported confidence (`high` / `medium` / `low`) |
| `llm_reason_fabricated` | Evaluator (deterministic) | `true` when `reason_source == "graph_metadata"` but `causal_density == 0.0` — the model claims rationale from the graph when none exists. No judge involved. |

## Single-config benchmarks (paper use cases)

For running a single agent+judge pair against the 4 structural use cases.
These tests use `REHYDRATION_PAPER_METRICS_DIR` to write per-use-case JSON
metrics. Configuration is via env vars, not the YAML matrix.

```bash
# Set inference model
export LLM_ENDPOINT=https://llm.underpassai.com/v1/chat/completions
export LLM_MODEL=Qwen/Qwen3-8B
export LLM_PROVIDER=openai
export LLM_TLS_CERT_PATH=/tmp/vllm-client.crt
export LLM_TLS_KEY_PATH=/tmp/vllm-client.key
export LLM_TLS_INSECURE=true
export LLM_TEMPERATURE=0.0

# Set judge
export LLM_JUDGE_ENDPOINT=https://api.anthropic.com/v1/messages
export LLM_JUDGE_MODEL=claude-opus-4-6
export LLM_JUDGE_PROVIDER=anthropic
export LLM_JUDGE_API_KEY="$(cat /path/to/anthropic-key.txt)"

# Output directory
export PAPER_OUTPUT_DIR=artifacts/paper-use-cases-qwen3-8b-agent_opus46-judge

# Run
bash scripts/ci/integration-paper-use-cases.sh
```

## Metric fields (paper use cases)

| Field | Description |
|-------|-------------|
| `query_latency_ms` | gRPC round-trip time |
| `total_latency_ms` | End-to-end including LLM evaluation |
| `graph_load_ms` | Neo4j neighborhood load time |
| `detail_load_ms` | Valkey batch MGET time |
| `bundle_assembly_ms` | Bundle construction time |
| `detail_batch_size` | Number of nodes in batch detail load |
| `llm_task_success` | Judge: failure point correctly identified |
| `llm_restart_accuracy` | Judge: restart node with causal justification |
| `llm_reason_preserved` | Judge: rationale preserved, not inferred |
| `llm_latency_ms` | Combined inference + judge latency |

## P1 performance measurement

Measures shared graph reads and batch detail loading (multi-role vs
repeated single-role). Containers only, no LLM:

```bash
cargo test -p rehydration-tests-kernel --features container-tests \
  --test kernel_p1_performance_measurement -- --nocapture --test-threads=1
```

## Determinism

Set `LLM_TEMPERATURE=0.0` in the YAML or env vars. API-side non-determinism
(batching, quantization) may cause minor variation between runs.

## Modifying the matrix

Edit `crates/rehydration-testkit/resources/evaluation-matrix.yaml`.

**Add a new agent:**

```yaml
agents:
  my-model:
    endpoint: https://my-api.example.com/v1/chat/completions
    model: my-model-name
    provider: openai          # openai | openai-new | anthropic
    api_key_env: MY_API_KEY   # name of env var
```

**Add a TLS agent:**

```yaml
agents:
  my-tls-model:
    endpoint: https://secure.example.com/v1/chat/completions
    model: my-model-name
    provider: openai
    tls: true                 # uses tls.cert and tls.key paths

tls:
  cert: /path/to/client.crt
  key: /path/to/client.key
```

**Add a new judge:**

```yaml
judges:
  my-judge:
    endpoint: https://my-judge-api.example.com/v1/chat/completions
    model: my-judge-model
    provider: openai-new
    api_key_env: MY_JUDGE_KEY
```

**Add a new prompt variant:**

```yaml
prompts:
  my-prompt: prompt-variants/my-custom-prompt.yaml
```

Then create `crates/rehydration-testkit/resources/prompt-variants/my-custom-prompt.yaml`.

The precheck will validate your changes before any container starts.
