# PIR Fix Planning Model Research Notes

Status: working research note
Date: 2026-04-12
Scope: theory, practice, model cards, and state-of-the-art references for model
selection around `PIR` `fix_planning`

## Purpose

This note complements
[`pir-fix-planning-long-budget-retry-plan.md`](pir-fix-planning-long-budget-retry-plan.md).
Execution sequencing lives in
[`pir-fix-planning-experiment-matrix.md`](pir-fix-planning-experiment-matrix.md).

The plan answers:

- what to change next

This note answers:

- what kinds of models actually matter for the current bottleneck
- which model families are strong candidates
- what the current state of the art says about planners, rerankers, reward
  models, and coder models
- what is theoretically attractive but practically wrong for this use case

## First Theoretical Clarification

There are two very different meanings of "expert models":

1. internal experts inside a Mixture-of-Experts model
2. externally separated specialist models in a multi-model system

For our use case, the second meaning matters more.

Why:

- an MoE planner such as a Qwen3 or Qwen3-Coder MoE model may route internally
  across experts, but we do not control those experts directly
- we cannot operationally assign one internal expert to `json repair` and another
  to `reward scoring`
- for `PIR`, useful specialization happens at the system level:
  - planner model
  - repair model
  - reranker
  - reward or judge model
  - coder model

So the practical architecture question is not:

- "does the base model contain experts?"

It is:

- "which separate model function should own which part of the incident loop?"

## Practical Translation To Our Use Case

The current `fix_planning` bottleneck is:

- planner latency and planner reliability during the first structured generation

That means the most relevant model classes are:

1. planner models
2. repair/jsonizer models
3. reward/judge models
4. rerankers
5. coder models for later stages

Each class solves a different problem.

### Planner Models

These should:

- reason over the local graph context
- produce remediation hypotheses
- propose a concrete mitigation plan

They are the right place to spend long wall-clock budget.

### Repair / JSONizer Models

These should:

- turn semantically useful but malformed output into strict JSON
- preserve meaning while reducing structured-output failure

They are not substitutes for planner reasoning.

### Reward / Judge Models

These should:

- score a completed candidate
- rank two or more candidate plans
- act as an acceptance sidecar

Important practical caveat:

- many reward models are sequence-classification models, not chat-completion
  models
- this means they are not drop-in replacements for the current `CompleteJSON`
  path

### Rerankers

These should:

- rank retrieval candidates
- rank evidence chunks
- rank multiple planner outputs

They do not replace generation.

### Coder Models

These should:

- generate patches
- reason about implementation details
- support `patch_application`

They should not be used to solve the current `fix_planning` latency bottleneck
first.

## State Of The Art References

The references below are primary sources or direct paper pages tied to the
model families under consideration.

### 1. Qwen3 Technical Report

- paper: [Qwen3 Technical Report](https://hf.co/papers/2505.09388)
- relevance:
  - strongest direct reference for the current Qwen3 instruct family
  - emphasizes unified thinking and non-thinking modes
  - relevant to agent tasks, code, multilingual behavior, and MoE variants
- implication for us:
  - Qwen3-family planner candidates are reasonable first-class planner options
  - but the paper does not remove the need for a real task budget

### 2. Qwen3 Embedding And Reranking

- paper: [Qwen3 Embedding: Advancing Text Embedding and Reranking Through Foundation Models](https://hf.co/papers/2506.05176)
- relevance:
  - direct state-of-the-art reference for the Qwen3 reranker family
  - specifically about reranking and retrieval quality
- implication for us:
  - Qwen3 rerankers are strong support-model candidates
  - they should be used for ranking or retrieval, not as planners

### 3. DeepSeek-R1

- paper: [DeepSeek-R1: Incentivizing Reasoning Capability in LLMs via Reinforcement Learning](https://hf.co/papers/2501.12948)
- relevance:
  - direct reasoning-focused reference for the DeepSeek-R1 family and its
    distilled derivatives
  - useful when the main concern is reasoning strength rather than compactness
- implication for us:
  - `DeepSeek-R1-Distill-Qwen-32B` is a serious alternate planner candidate
  - but it likely increases latency pressure rather than reducing it

### 4. Skywork-Reward-V2

- paper: [Skywork-Reward-V2: Scaling Preference Data Curation via Human-AI Synergy](https://hf.co/papers/2507.01352)
- relevance:
  - direct state-of-the-art reference for the Skywork reward family
  - useful for understanding why reward models can be strong acceptance scorers
- implication for us:
  - reward models are promising judge sidecars
  - they should not be treated as direct replacements for planner generation

### 5. RewardBench 2

- paper: [RewardBench 2: Advancing Reward Model Evaluation](https://hf.co/papers/2506.01937)
- relevance:
  - not a model card, but a benchmark reference for evaluating reward models
  - useful for understanding what "good judge" means in practice
- implication for us:
  - if we later introduce a reward-model sidecar, we should evaluate it against
    a modern reward benchmark rather than assuming judgment quality

### 6. Qwen2.5-Coder Technical Report

- paper: [Qwen2.5-Coder Technical Report](https://hf.co/papers/2409.12186)
- relevance:
  - direct reference for the coder family we may use later
  - focuses on code generation, completion, reasoning, and repair
- implication for us:
  - coder models belong to `patch_application` and adjacent stages
  - they are not the first optimization target while `fix_planning` is still
    planner-bound

### 7. Survey On Mixture Of Experts

- paper: [A Survey on Mixture of Experts](https://hf.co/papers/2407.06204)
- relevance:
  - useful conceptual reference for MoE behavior and tradeoffs
- implication for us:
  - MoE architecture can improve compute efficiency or capability scaling
  - it does not give us direct operational control over internal task experts

## Internet-Sourced Model Card Inventory

All models below were inventoried from current official Hugging Face model cards
or metadata.

### Planner Candidates

#### `Qwen/Qwen3-30B-A3B-Instruct-2507`

- card: [https://hf.co/Qwen/Qwen3-30B-A3B-Instruct-2507](https://hf.co/Qwen/Qwen3-30B-A3B-Instruct-2507)
- model class: `AutoModelForCausalLM`
- architecture: `qwen3_moe`
- license: Apache-2.0
- why it matters:
  - strongest direct Qwen3-family planner candidate in the shortlist
- practical fit:
  - good primary planner candidate if we accept long budgets

#### `deepseek-ai/DeepSeek-R1-Distill-Qwen-32B`

- card: [https://hf.co/deepseek-ai/DeepSeek-R1-Distill-Qwen-32B](https://hf.co/deepseek-ai/DeepSeek-R1-Distill-Qwen-32B)
- model class: `AutoModelForCausalLM`
- architecture: `qwen2`
- license: MIT
- why it matters:
  - reasoning-focused planner alternative
- practical fit:
  - good alternate planner if we want reasoning emphasis

#### `mistralai/Mistral-Small-3.2-24B-Instruct-2506`

- card: [https://hf.co/mistralai/Mistral-Small-3.2-24B-Instruct-2506](https://hf.co/mistralai/Mistral-Small-3.2-24B-Instruct-2506)
- architecture: `mistral3`
- card metadata highlights `vllm`
- license: Apache-2.0
- why it matters:
  - useful family-diverse planner candidate with a strong serving story
- practical fit:
  - best alternate-family planner in the current shortlist

### Repair / JSONizer Candidate

#### `Qwen/Qwen3-4B-Instruct-2507`

- card: [https://hf.co/Qwen/Qwen3-4B-Instruct-2507](https://hf.co/Qwen/Qwen3-4B-Instruct-2507)
- model class: `AutoModelForCausalLM`
- architecture: `qwen3`
- license: Apache-2.0
- why it matters:
  - small instruct model for structured repair and bounded retries
- practical fit:
  - best current repair-sidecar candidate in the shortlist

### Reranker Candidates

#### `Qwen/Qwen3-Reranker-4B`

- card: [https://hf.co/Qwen/Qwen3-Reranker-4B](https://hf.co/Qwen/Qwen3-Reranker-4B)
- task: `text-ranking`
- model class: `AutoModelForCausalLM`
- why it matters:
  - stronger reranking sidecar
- practical fit:
  - useful when evidence or candidate ordering becomes important

#### `Qwen/Qwen3-Reranker-0.6B`

- card: [https://hf.co/Qwen/Qwen3-Reranker-0.6B](https://hf.co/Qwen/Qwen3-Reranker-0.6B)
- task: `text-ranking`
- model class: `AutoModelForCausalLM`
- why it matters:
  - lighter reranking sidecar
- practical fit:
  - low-cost ranking support

### Reward / Judge Candidate

#### `Skywork/Skywork-Reward-V2-Qwen3-8B`

- card: [https://hf.co/Skywork/Skywork-Reward-V2-Qwen3-8B](https://hf.co/Skywork/Skywork-Reward-V2-Qwen3-8B)
- task: `text-classification`
- model class: `AutoModelForSequenceClassification`
- why it matters:
  - strong reward-model candidate for ranking or acceptance scoring
- practical fit:
  - useful as a judge sidecar
- practical caveat:
  - requires classifier-style integration, not chat-completions wiring

### Coder Candidates

#### `Qwen/Qwen3-Coder-30B-A3B-Instruct`

- card: [https://hf.co/Qwen/Qwen3-Coder-30B-A3B-Instruct](https://hf.co/Qwen/Qwen3-Coder-30B-A3B-Instruct)
- model class: `AutoModelForCausalLM`
- architecture: `qwen3_moe`
- why it matters:
  - strong patch-stage model candidate
- practical fit:
  - save for `patch_application`

#### `Qwen/Qwen2.5-Coder-14B-Instruct`

- card: [https://hf.co/Qwen/Qwen2.5-Coder-14B-Instruct](https://hf.co/Qwen/Qwen2.5-Coder-14B-Instruct)
- model class: `AutoModelForCausalLM`
- architecture: `qwen2`
- why it matters:
  - lighter code-specialized fallback
- practical fit:
  - useful if `30B` coder cost is too high

## What Looks State Of The Art For Our Exact Problem

The exact current problem is not generic "agent quality".

It is:

- long-budget graph-local incident planning with structured output and truthful
  escalation

For that exact problem, the most relevant state-of-the-art pattern is:

1. a strong planner model
2. enough wall-clock budget
3. optional small repair model
4. optional ranking or reward sidecars

The papers and cards do not support this conclusion:

- "rerankers can replace planners"
- "reward models can replace chat-completion planners"
- "coder models should fix planner latency"

They do support this conclusion:

- different model classes are state of the art for different narrow tasks
- the strongest architecture for one support task may be the wrong choice for
  the main planner slot

## Recommended Experimental Ordering

The current evidence supports this order:

1. give the planner a real budget first

- use the existing planner or swap to one strong planner candidate
- test under long-budget conditions before changing too many variables

2. add a small repair model second

- likely `Qwen/Qwen3-4B-Instruct-2507`
- use only for structure repair or bounded retries

3. add reranking or reward sidecars third

- reranker if retrieval or candidate ordering becomes the issue
- reward model if acceptance quality becomes the issue

4. reserve coder models for patch stages

- do not conflate planning and patch generation

## Current Best Working Shortlist

If we had to shortlist only the most actionable candidates right now:

- primary planner:
  - `Qwen/Qwen3-30B-A3B-Instruct-2507`
  - `deepseek-ai/DeepSeek-R1-Distill-Qwen-32B`
  - `mistralai/Mistral-Small-3.2-24B-Instruct-2506`
- repair sidecar:
  - `Qwen/Qwen3-4B-Instruct-2507`
- reranker sidecar:
  - `Qwen/Qwen3-Reranker-0.6B`
  - `Qwen/Qwen3-Reranker-4B`
- reward sidecar:
  - `Skywork/Skywork-Reward-V2-Qwen3-8B`
- patch-stage coder:
  - `Qwen/Qwen3-Coder-30B-A3B-Instruct`
  - `Qwen/Qwen2.5-Coder-14B-Instruct`

## Recommended Use Of This Note

Use this document as:

- the theory and source appendix for the iteration plan
- the starting point for deployment experiments in `PIR`
- the shortlist reference before wiring multi-model fallback logic

Do not use it as:

- proof that a support model should be added before the planner has enough time
- proof that MoE internals give us controllable task experts
- proof that the current `fix_planning` bottleneck is retrieval or coding
