# Qwen3.5 Configuration Reference

This note is the operational reference for using `Qwen3.5` in `PIR` and adjacent kernel-integrating products.

It is based on official Qwen and vLLM documentation, plus a small amount of explicitly marked engineering inference for our incident-response use case.

## Scope

This document is about the `Qwen3.5` family served through an OpenAI-compatible API, especially via `vLLM`.

Our current deployed model is `Qwen/Qwen3.5-9B`, but most serving and API behavior documented here is described by Qwen as applying to the `Qwen3.5` family more broadly.

## Official Facts

### 1. Qwen3.5 thinks by default

Official Qwen guidance says Qwen3.5 models operate in thinking mode by default and emit thinking content before the final answer.

For OpenAI-compatible APIs, official examples disable visible thinking with:

```json
{
  "chat_template_kwargs": {
    "enable_thinking": false
  }
}
```

Important:

- Qwen3.5 does not officially support the soft `/think` and `/nothink` switch used in some Qwen3 flows.
- For Alibaba Cloud Model Studio, the parameter shape differs; for `vLLM` and `SGLang`, `chat_template_kwargs.enable_thinking` is the official path.

### 2. vLLM supports Qwen3 reasoning explicitly

Official vLLM documentation states:

- Qwen3 series reasoning is enabled by default.
- To disable it, pass `enable_thinking=false` in `chat_template_kwargs`.
- Request-level `chat_template_kwargs` override server defaults.

That gives us two valid control planes:

1. server-wide default
2. per-request override

Server-wide example from vLLM docs:

```bash
vllm serve Qwen/Qwen3-8B \
  --reasoning-parser qwen3 \
  --default-chat-template-kwargs '{"enable_thinking": false}'
```

Request-level example from Qwen docs:

```python
client.chat.completions.create(
    model="Qwen/Qwen3.5-9B",
    messages=messages,
    extra_body={
        "chat_template_kwargs": {"enable_thinking": False},
    },
)
```

### 3. Thinking can be separated from final content

Official vLLM reasoning-output docs say reasoning models return an additional `reasoning` field and a final `content` field.

This matters for us:

- if thinking is enabled, the model may spend output budget on `reasoning`
- the final answer may still be empty or truncated if the budget is exhausted before `content` is completed

The vLLM Qwen3/Qwen3.5 reasoning parser docs are especially important here:

- `enable_thinking=false` is a strict switch
- when thinking is disabled, output is treated as content directly
- when thinking is enabled and the output is truncated before reasoning closes, the parser may treat the generated text as reasoning rather than final content

This is directly relevant to the failure mode we observed in `PIR`.

### 3b. "Thinking yes, but not visible thinking text" is a valid target

For Qwen3.5 on vLLM, "thinking internally but not mixing the thinking text into the contractual answer" is a valid design goal, but only if the serving and client layers are configured correctly.

The intended shape is:

- `thinking enabled`
- vLLM `reasoning-parser=qwen3`
- reasoning captured in `message.reasoning`
- final answer captured in `message.content`

That is different from:

- `thinking enabled`
- client only cares about `content`
- output budget exhausted before final content is emitted

In the second case, the system may still fail even though the model "thought correctly", because the contractual answer never completed.

### 4. Structured outputs do not constrain the reasoning stream

Official vLLM structured-output examples for reasoning models state that the thinking process is not guided by the JSON schema and only the final output is structured.

Practical implication:

- `thinking on` + `structured JSON` is not the same as "the whole response is JSON-safe"
- the final content may be schema-constrained while the reasoning still consumes output budget

For strict JSON-only subroles, this is a critical distinction.

### 5. Official sampling guidance

Official Qwen guidance recommends these sampling profiles:

- Thinking mode, general tasks:
  `temperature=1.0`, `top_p=0.95`, `top_k=20`, `min_p=0.0`, `presence_penalty=1.5`, `repetition_penalty=1.0`
- Thinking mode, precise coding tasks:
  `temperature=0.6`, `top_p=0.95`, `top_k=20`, `min_p=0.0`, `presence_penalty=0.0`, `repetition_penalty=1.0`
- Instruct or non-thinking mode, general tasks:
  `temperature=0.7`, `top_p=0.8`, `top_k=20`, `min_p=0.0`, `presence_penalty=1.5`, `repetition_penalty=1.0`
- Instruct or non-thinking mode, reasoning tasks:
  `temperature=1.0`, `top_p=1.0`, `top_k=40`, `min_p=0.0`, `presence_penalty=2.0`, `repetition_penalty=1.0`

These are model-vendor recommendations, not hard requirements.

### 6. Official output-length guidance is large

Official Qwen guidance recommends:

- `32,768` output tokens for most queries
- `81,920` for highly complex problems

This is much larger than our current `PIR` JSON-completion cap of `1024`.

### 7. Context length and long-context serving

Official Qwen guidance states:

- native context length: `262,144`
- extensible up to roughly `1,010,000` with YaRN-based scaling
- if preserving thinking capability, maintain at least `128K` context when possible

This matters more for long-horizon planning than for our current `fix_planning` prompt, which is short by comparison.

### 8. Tool-calling support

Official Qwen guidance for vLLM tool calling uses:

```bash
vllm serve Qwen/Qwen3.5-35B-A3B \
  --port 8000 \
  --tensor-parallel-size 8 \
  --max-model-len 262144 \
  --reasoning-parser qwen3 \
  --enable-auto-tool-choice \
  --tool-call-parser qwen3_coder
```

This is useful for future agentic stages, but it is not the immediate fix for our `fix_planning` JSON path.

## Configuration Profiles

The profiles below mix official facts with `PIR`-specific engineering recommendations.

### Profile A: Planner / Investigator

Goal:
open-ended diagnosis, mitigation design, or deep analysis.

Recommended configuration:

- `reasoning-parser=qwen3`
- `enable_thinking=true` or omit the override
- generous output budget
- capture `message.reasoning` separately from `message.content`
- avoid relying on strict JSON as the only output channel

Use when:

- the task benefits from multi-step analysis
- the output can be prose, plan text, or a two-stage pipeline

Do not use when:

- the stage requires a strict one-shot JSON contract

This is the right profile when we want the model to think, but we do not want the visible reasoning stream to be mistaken for the final structured answer.

### Profile B: JSONizer / Repair

Goal:
rewrite or normalize content into one valid JSON object.

Recommended configuration:

- `reasoning-parser=qwen3`
- request-level `chat_template_kwargs.enable_thinking=false`
- lower-variance sampling
- structured output guidance when available
- modest output cap, but not so low that valid JSON is clipped

Use when:

- the job is syntactic repair
- the output contract is strict
- any visible reasoning is a liability

Why:

Official vLLM docs say structured output only constrains the final output, not the reasoning stream. So keeping thinking enabled for a JSONizer can still burn output budget before the final JSON is complete.

### Profile C: Fast Judge

Goal:
return a bounded verdict such as `accept/reject`, confidence, or a short critique JSON.

Recommended configuration:

- `enable_thinking=false`
- structured output or bounded-choice output
- smaller output budget

Use when:

- latency matters
- the verdict schema is small
- we want stable, repeatable outputs

### Profile D: Deep Judge

Goal:
do a more nuanced review of a candidate plan.

Recommended configuration:

- `enable_thinking=true`
- separate `reasoning` capture
- larger output budget
- final answer not forced to be the only channel

Use when:

- the review is expensive and high-value
- we can afford a slower second pass

### Profile E: Tool-Calling Agent

Goal:
interactive agent or runtime-mediated action selection.

Recommended configuration:

- `reasoning-parser=qwen3`
- `--enable-auto-tool-choice`
- `--tool-call-parser qwen3_coder`
- decide `enable_thinking` per request, not globally

Use when:

- the model is orchestrating tools
- the output includes tool calls rather than only plain text or JSON

## Recommended Default For PIR

This section is an engineering recommendation, not a statement from Qwen or vLLM.

### 1. `fix_planning.generate`

Use a planner profile.

Two valid strategies exist:

1. keep thinking on and capture `reasoning` separately
2. split the stage into `planner -> jsonizer`

If we insist on a single strict JSON response from the planner, then thinking-on becomes operationally risky because the model may spend too much of the output budget before emitting final content.

### 2. `fix_planning.repair`

Use the JSONizer profile.

For `repair`, `enable_thinking=false` should be treated as mandatory by default.

Reason:

- the task is structural
- visible reasoning adds no value
- official docs say reasoning and structured output are distinct channels
- our live failure matched that exact risk pattern

### 3. `fix_planning.judge`

Default to the fast judge profile.

Only use deep-judge mode if:

- the first-pass judge is too weak, and
- we are willing to pay the latency budget

### 4. Global server default vs request override

Recommended policy for `PIR`:

- keep the vLLM server capable of reasoning
- set behavior explicitly at request level per role

Why:

- vLLM says request-level `chat_template_kwargs` override server defaults
- this lets us run planner and repair differently without maintaining separate model deployments unless needed

## What We Should Not Assume

- We should not assume `/think` or `/nothink` works on Qwen3.5. Official Qwen docs say it is not supported.
- We should not assume "JSON mode" alone prevents reasoning output from consuming tokens.
- We should not assume a `200 OK` means the final content is valid. It may still end with `finish_reason="length"`.
- We should not assume the official Qwen sampling guidance is optimal for deterministic production JSON. Those values are vendor recommendations for general generation quality, not necessarily for contract-strict pipelines.

## Immediate Takeaway For Current PIR Failure

The current failure mode is consistent with the official docs:

- Qwen3.5 thinks by default
- vLLM exposes reasoning separately
- structured output only constrains final output
- a JSON repair subrole with thinking enabled can consume output budget before the final JSON is completed

So the first configuration correction is:

- keep `Qwen3.5` as the model
- disable thinking at request level for `CompleteJSON`, especially `repair`

## Sources

Primary sources used in this note:

- Official Qwen model card for `Qwen/Qwen3.5-9B`:
  https://huggingface.co/Qwen/Qwen3.5-9B
- Official Qwen model card for `Qwen/Qwen3.5-35B-A3B`:
  https://huggingface.co/Qwen/Qwen3.5-35B-A3B
- Official vLLM reasoning outputs guide:
  https://docs.vllm.ai/en/stable/features/reasoning_outputs/
- Official vLLM Qwen3/Qwen3.5 reasoning parser docs:
  https://docs.vllm.ai/en/stable/api/vllm/reasoning/qwen3_reasoning_parser/
- Official vLLM structured outputs guide:
  https://docs.vllm.ai/en/v0.12.0/features/structured_outputs/
- Official vLLM structured outputs with reasoning example:
  https://docs.vllm.ai/en/latest/examples/online_serving/openai_chat_completion_structured_outputs_with_reasoning.html
- Official vLLM Qwen3.5 recipe:
  https://docs.vllm.ai/projects/recipes/en/latest/Qwen/Qwen3.5.html
