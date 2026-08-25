# Incident: A/B Arm B wasted run — thinking without reasoning-parser

**Date:** 2026-03-29
**Impact:** 108 evals wasted (0/108 accuracy), ~$0.50 judge cost, ~20 min runtime
**Root cause:** Server-side `--reasoning-parser=qwen3` not applied to cluster

## What happened

1. We updated `archive/k8s/vllm-server-current.yaml` to include `--reasoning-parser=qwen3`
2. We removed the `max_tokens * 8` multiplier in `llm_evaluator.rs`, assuming the
   reasoning parser was active and thinking tokens would go to a separate field
3. We ran Arm B with `LLM_ENABLE_THINKING=true` and `max_tokens=200`
4. **The k8s manifest was never applied to the cluster** — the server was still
   running WITHOUT `--reasoning-parser`

## What went wrong

Without `--reasoning-parser=qwen3` on the server:
- `enable_thinking: true` activates CoT in the model
- Thinking tokens come back mixed in the `content` field with `<think>` tags
- `max_tokens=200` limits TOTAL output (thinking + answer)
- The model uses all 200 tokens for thinking, zero left for the JSON answer
- `strip_thinking_tags()` extracts empty content
- Result: empty responses, 0/108 on all metrics

## Why it wasn't caught

- The code assumed server state matched the yaml in the repo
- No precheck validates whether `--reasoning-parser` is active on the server
- The commit message said "Server patched" but only the yaml was updated

## Prevention

### Rule: yaml in repo != deployed to cluster

Updating a k8s manifest in the repo does NOT change the running server.
You must `kubectl apply -f <manifest>` AND verify the pod restarts.

### Two mutually exclusive yamls

| File | Mode | `--reasoning-parser` | When to use |
|------|------|---------------------|-------------|
| `archive/k8s/vllm-no-thinking.yaml` | Baseline | absent | Default. `LLM_ENABLE_THINKING` must be `false` |
| `archive/k8s/vllm-thinking.yaml` | Thinking | `qwen3` | A/B thinking arm. `LLM_ENABLE_THINKING=true` safe |

Both deploy `vllm-server` in `underpass-runtime` — applying one replaces the other.

### Switching protocol

```bash
# Switch to thinking mode
kubectl apply -f archive/k8s/vllm-thinking.yaml
kubectl rollout status deployment/vllm-server -n underpass-runtime --timeout=120s
# Verify: the model list endpoint should respond
curl --cert /tmp/vllm-client.crt --key /tmp/vllm-client.key -k \
  https://llm.underpassai.com/v1/models

# Switch back to no-thinking
kubectl apply -f archive/k8s/vllm-no-thinking.yaml
kubectl rollout status deployment/vllm-server -n underpass-runtime --timeout=120s
```

### Guard in code

If `LLM_ENABLE_THINKING=true` but the server has no reasoning-parser,
the response will be empty (all thinking, no content). The evaluator should
detect this and fail fast with a clear error instead of silently scoring 0%.
