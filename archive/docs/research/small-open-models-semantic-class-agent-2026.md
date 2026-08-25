# Small Open Models for a Semantic-Class Catalog Agent

_Research note, curated on April 10, 2026._

## Scope

This note is about one narrow kernel task: classify each graph relation into one
of the six `semantic_class` labels:

- `causal`
- `motivational`
- `procedural`
- `evidential`
- `constraint`
- `structural`

This is materially cheaper than full graph extraction. We do not need a broad
"general assistant" here. We need a small model that is:

- cheap to dedicate as a side agent
- deterministic enough for label selection
- easy to serve in-cluster
- permissively licensed when possible

## Executive Summary

- If we keep the current OpenAI-compatible `chat/completions` path, the best
  default is `Qwen/Qwen3-1.7B`.
- If we are willing to change the classifier into a scoring/reranking service,
  `Qwen/Qwen3-Reranker-0.6B` is the lowest-resource specialist in this list.
- If we want an Apache-2.0 edge-first alternative, IBM Granite 4.0 Nano
  (`granite-4.0-h-1b` and `granite-4.0-h-350m`) is the most interesting family.
- For this slice, 7B+ models should be a fallback, not the default.

## Recommended Order

1. `Qwen/Qwen3-1.7B`
2. `Qwen/Qwen3-Reranker-0.6B`
3. `ibm-granite/granite-4.0-h-1b`
4. `Qwen/Qwen3-0.6B`
5. `ibm-granite/granite-3.2-2b-instruct` or `ibm-granite/granite-3.3-2b-instruct`
6. `microsoft/Phi-4-mini-instruct`
7. `ibm-granite/granite-4.0-h-350m` as an ultra-cheap experimental option

## Shortlist

| Model | Why it fits this task | Est. weight memory* | Notes |
|:------|:----------------------|--------------------:|:------|
| `Qwen/Qwen3-Reranker-0.6B` | Best low-cost specialist if we switch from generation to scoring. The Qwen3 reranker line is built for ranking and text classification, and the model card includes `vLLM` usage. | BF16 ~1.2 GB, INT4 ~0.3 GB | Not a drop-in replacement for our current `chat/completions` path. Best when we score the 6 labels and pick the max. |
| `Qwen/Qwen3-1.7B` | Best default if we keep the current agent shape. Apache-2.0, 32K context, tool/agent support, easy `vllm serve`, and still small. | BF16 ~3.4 GB, INT4 ~0.85 GB | Recommended first deployment for the semantic-class agent. Run with thinking disabled. |
| `ibm-granite/granite-4.0-h-1b` | Edge-oriented Apache-2.0 model with text classification and function-calling listed as core capabilities. Granite Nano was released specifically for small-footprint deployments. | BF16 ~3.0 GB, INT4 ~0.75 GB | Strong candidate if we want an IBM/Granite lane with long context and small footprint. |
| `Qwen/Qwen3-0.6B` | Cheapest generative drop-in. Same Qwen3 family and serving path as the 1.7B model, but much cheaper. | BF16 ~1.2 GB, INT4 ~0.3 GB | Good for a first "minimum resources" bake-off, but more prompt-sensitive than 1.7B. |
| `ibm-granite/granite-3.2-2b-instruct` | Mature Apache-2.0 2B instruct model with text classification and function-calling in the model card. vLLM supports Granite architectures. | BF16 ~4.0 GB, INT4 ~1.0 GB | Good alternative if Qwen underperforms on our label taxonomy. |
| `HuggingFaceTB/SmolLM3-3B` | Fully open, compact, long-context model. Strong choice if we want maximum openness and more headroom than 1B-2B models. | BF16 ~6.0 GB, INT4 ~1.5 GB | More expensive than the Qwen/Granite options; use if openness matters more than absolute cost. |
| `microsoft/Phi-4-mini-instruct` | Strongest general small model in this shortlist. MIT license, 128K context, explicit function-calling format, and documented vLLM inference. | BF16 ~7.6 GB, INT4 ~1.9 GB | Best "small but strong" fallback, but no longer "minimum cost". |
| `ibm-granite/granite-4.0-h-350m` | The cheapest serious Apache-2.0 option in this note. Designed for edge and on-device use, with native support called out for vLLM, llama.cpp, and MLX. | BF16 ~0.68 GB, INT4 ~0.17 GB | Experimental for our task. Very attractive on cost, but likely weaker and less stable on a 6-label taxonomy. |

## Best Fit by Architecture

### 1. If we keep the current generative classifier

Pick `Qwen/Qwen3-1.7B`.

Reason:

- minimal architecture change
- small enough for a dedicated side service
- same OpenAI-compatible serving path we already use
- better odds of stable JSON and stable labels than 0.6B-class models

### 2. If we optimize purely for minimum resource cost

Pick `Qwen/Qwen3-0.6B` first, and test `ibm-granite/granite-4.0-h-350m` as
the aggressive low-cost experiment.

Reason:

- both are far cheaper than 3B-4B models
- both are realistic candidates for a permanently-on side agent
- Granite 350M is compelling when cost dominates everything else

### 3. If we redesign the classifier into a specialist scorer

Pick `Qwen/Qwen3-Reranker-0.6B`.

Reason:

- the task is fixed-label classification, not open-ended generation
- reranking/scoring is a better fit than free-form completion
- the model card already documents `vLLM` usage and a low-token yes/no scoring
  path

For our kernel, that would mean:

1. build one prompt per candidate label
2. score each label against a relation
3. choose the top score

That is more engineering than a tiny generative model, but it is probably the
lowest-resource route that still has a credible accuracy ceiling.

## Open-Weight Alternatives With License Caveats

These are worth knowing, but they are not the cleanest "open source" answer:

- `google/gemma-3-1b-it`
  - gemma license, not Apache/MIT
  - 1B model has 32K context; the Gemma 3 family goes up to 128K on larger sizes
  - attractive for low-resource deployments, but not the cleanest licensing fit
- `meta-llama/Llama-3.2-1B-Instruct`
  - Llama 3.2 Community License, not OSI-style open source
  - 1B/3B models were explicitly positioned for constrained environments and
    agentic retrieval/summarization
  - viable technically, weaker licensing story than Apache/MIT models

## Deployment Guidance for This Specific Agent

- Disable thinking/reasoning mode. For this agent, latency and determinism are
  more valuable than chain-of-thought.
- Keep `temperature=0.0`.
- Keep prompts narrow and schema-constrained.
- Run it as a post-processor only: it should rewrite `semantic_class`, not
  regenerate nodes, relations, or details.
- Validate on our own labeled relation set before promoting it into the write
  path.

## Concrete Recommendation

If we want a practical first deployment with minimal engineering risk:

1. deploy `Qwen/Qwen3-1.7B`
2. keep it in non-thinking mode
3. use it only as a post-classifier for `semantic_class`
4. compare it against `Qwen/Qwen3-0.6B` and `ibm-granite/granite-4.0-h-1b`

If we later want to minimize cost harder, the next serious step is not "pick a
weaker generator". It is:

1. add a scoring-based classifier path
2. test `Qwen/Qwen3-Reranker-0.6B`

That is the best low-resource architecture for this task.

## Sources

- Qwen3-0.6B model card:
  <https://huggingface.co/Qwen/Qwen3-0.6B>
- Qwen3-1.7B model card:
  <https://huggingface.co/Qwen/Qwen3-1.7B>
- Qwen3-Reranker-0.6B model card:
  <https://huggingface.co/Qwen/Qwen3-Reranker-0.6B>
- Phi-4-mini-instruct model card:
  <https://huggingface.co/microsoft/Phi-4-mini-instruct>
- Granite 4.0 Nano overview:
  <https://huggingface.co/blog/ibm-granite/granite-4-nano>
- Granite 4.0 H-1B model card:
  <https://huggingface.co/ibm-granite/granite-4.0-h-1b>
- Granite 4.0 H-350M model card:
  <https://huggingface.co/ibm-granite/granite-4.0-h-350m>
- Granite 3.2 2B instruct model card:
  <https://huggingface.co/ibm-granite/granite-3.2-2b-instruct>
- SmolLM3 docs:
  <https://huggingface.co/docs/transformers/en/model_doc/smollm3>
- SmolLM3 model card:
  <https://huggingface.co/HuggingFaceTB/SmolLM3-3B-Base>
- vLLM supported models:
  <https://docs.vllm.ai/en/stable/models/supported_models.html>
- Gemma 3 1B instruct model card:
  <https://huggingface.co/google/gemma-3-1b-it>
- Llama 3.2 1B instruct model card:
  <https://huggingface.co/meta-llama/Llama-3.2-1B-Instruct>

---

\* Weight-memory numbers are engineering estimates, not vendor-published
runtime guarantees. They are derived from parameter count and precision only.
Actual runtime memory is higher because of KV cache, allocator overhead,
concurrency, batching, and context length.
