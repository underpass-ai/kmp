# Incident: vLLM thinking mode — 3 failed runs before correct configuration

**Date:** 2026-03-29
**Impact:** 3 wasted benchmark runs (216+ evals, ~$1.50 judge cost, ~60 min runtime)
**Root cause:** Configuration applied without verifying against official documentation

## Timeline

| Run | Config | Result | Failure mode |
|-----|--------|--------|-------------|
| Arm B v1 | v0.15.0, `chat_template_kwargs: {enable_thinking: true}`, `max_tokens * 8` | 0/108 | Code assumed `--reasoning-parser` separated tokens. Server yaml not applied to cluster. |
| Arm B v2 | v0.15.0, `--reasoning-parser=qwen3` on server, no `chat_template_kwargs`, `max_tokens=200` | 0/108 | Parser needs `</think>` tag to separate. 200 tokens too few — model thinks all 200, never closes tag. |
| Arm B v3 | v0.15.0, `max_tokens=800`, `thinking_token_budget=512` | 0/18 (stopped) | `thinking_token_budget` requires `--reasoning-config` on server. Flag doesn't exist in v0.15.0. |
| Smoke | v0.18.0-cu130, `--reasoning-parser=qwen3`, `max_tokens=4096`, `temp=0.6` | **Working** | Model thinks ~1600 tokens, responds ~200. Parser separates correctly. |

## Root causes

### 1. `chat_template_kwargs: {enable_thinking: true}` breaks the reasoning parser

Qwen3 thinks by default. Sending `enable_thinking: true` via `chat_template_kwargs`
changes the chat template output format — the model wraps thinking in `<think>` tags
inside the `content` field instead of letting the reasoning parser handle separation.

**Correct behavior:**
- Thinking ON: do NOT send `chat_template_kwargs` (Qwen3 default)
- Thinking OFF: send `chat_template_kwargs: {enable_thinking: false}`

Source: vLLM docs (`docs/features/reasoning_outputs.md`)

### 2. `max_tokens` controls TOTAL output (thinking + content)

The reasoning parser separates `<think>...</think>` from content AFTER generation.
It needs to see the closing `</think>` tag. If `max_tokens` is too low, the model
hits the limit mid-think (`finish_reason: length`) and never produces the tag.

Result: `content` contains raw `<think>` text, `reasoning_content` is null.

**Correct behavior:**
- `max_tokens` must be large enough for thinking + JSON answer
- Qwen3-8B uses ~500-1800 tokens for thinking depending on prompt complexity
- `max_tokens: 4096` gives sufficient headroom (model stops naturally at ~1800)

### 3. `thinking_token_budget` requires `--reasoning-config` on the server

The vLLM docs describe `thinking_token_budget` as a per-request hard cap that
forces `</think>` emission when the budget is exhausted. But this only works
when the server is started with `--reasoning-config '{"think_start_str":"<think>","think_end_str":"</think>"}'`.

- v0.15.0: `--reasoning-config` does not exist
- v0.18.0: `--reasoning-config` does not exist either (main branch only)

Without `--reasoning-config`, `thinking_token_budget` is silently ignored.

### 4. Qwen3 model card contradicts vLLM docs

| Source | Parser | Extra flag |
|--------|--------|-----------|
| vLLM `reasoning_outputs.md` | `--reasoning-parser qwen3` | none |
| HuggingFace Qwen/Qwen3-8B | `--reasoning-parser deepseek_r1` | `--enable-reasoning` |

`--enable-reasoning` does not exist in vLLM v0.15.0 or v0.18.0 (`unrecognized arguments`).
The correct parser for both versions is `qwen3`.

### 5. HuggingFace model card says DO NOT use greedy decoding

Qwen3 with thinking requires `temperature > 0` (recommended 0.6, top_p=0.95).
`temperature: 0.0` causes performance degradation and repetition loops.

Our baseline config had `temperature: 0.0`. Fixed to `0.6` in `evaluation-matrix.yaml`.

## Correct configuration

### Server (archive/k8s/vllm-qwen3-8b.yaml)

```yaml
args:
  - "--model=Qwen/Qwen3-8B"
  - "--reasoning-parser=qwen3"
  # ... other standard args
image: docker.io/vllm/vllm-openai:v0.18.0-cu130
```

### Client (evaluation-matrix.yaml)

```yaml
agents:
  qwen3-8b:
    max_tokens: 4096     # enough for thinking (~1800) + answer (~200)
    temperature: 0.6     # Qwen3 model card: DO NOT use greedy
```

### Code (llm_evaluator.rs)

```rust
if provider == LlmProvider::OpenAI {
    let disable = env("LLM_ENABLE_THINKING") == "false";
    if disable {
        body["chat_template_kwargs"] = json!({"enable_thinking": false});
    }
    // Thinking ON: send nothing. Qwen3 thinks by default.
    // Parser separates <think> into `reasoning` field.
}
```

### Response structure (vLLM v0.18.0)

```json
{
  "choices": [{
    "message": {
      "content": "\n\n{\"failure_point\": \"chain-1\", ...}",
      "reasoning": "<think>Okay, let me analyze...</think>",
      "reasoning_content": null
    },
    "finish_reason": "stop"
  }]
}
```

- `content`: clean JSON answer (no `<think>` tags)
- `reasoning`: CoT thinking (separated by parser)
- `reasoning_content`: null in v0.18.0 (legacy field, renamed to `reasoning`)
- `finish_reason`: must be `stop`, not `length`

## Prevention checklist

Before configuring any LLM inference parameter:

1. **Read the official vLLM docs** for the exact version deployed
   (`github.com/vllm-project/vllm/blob/v{VERSION}/docs/features/reasoning_outputs.md`)
2. **Read the model card** on HuggingFace for model-specific requirements
3. **If docs contradict**, test both and trust the vLLM source (it's the runtime)
4. **Verify with a single curl** before running the benchmark
5. **Check `finish_reason`**: `stop` = model finished, `length` = truncated (likely broken)
6. **Check `content` is not null**: null means all tokens went to thinking
7. **Agent calibration** catches empty responses before wasting eval budget
8. **Never assume** `thinking_token_budget` works — verify the server supports it
