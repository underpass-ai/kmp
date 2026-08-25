# LLM Response Determinism Strategy

> **Status: PLANNED** — not yet implemented

Status: `planned`

## Goal

Move this project from "parse JSON out of model text" to a provider-aware,
schema-first interaction model that is deterministic enough for production
agentic systems.

This is not just a parser cleanup. It is a contract strategy for how the
kernel and future integrating products should interact with:

- OpenAI-compatible endpoints
- Anthropic / Claude
- self-hosted vLLM
- reasoning-capable models in general

## Why This Matters

Recent provider-integration work already exposed the real problem:

- a model can return valid JSON in one call and `{}` in the next
- a model can mix `<think>...</think>` with the final answer
- a model can produce oversized one-shot plans that are syntactically fragile
- a naive parser can appear to work until the first real reasoning model or
  self-hosted endpoint behaves differently

That means the real design target is not "better regex" or "more tolerant
`json.loads`". The target is:

- deterministic output contracts
- explicit provider normalization
- schema validation before semantic validation
- reasoning-aware handling that never depends on chain-of-thought text

## State Of The Art Direction

### 1. Prefer provider-native structured output over free-text parsing

Current best practice is to ask the model to produce a typed structure at the
transport level whenever the provider supports it.

For this project, that means:

- OpenAI: prefer Structured Outputs / JSON Schema and strict tool contracts
- Anthropic: prefer tools with `input_schema` rather than parsing free text
- vLLM: prefer guided decoding / structured outputs with JSON Schema or grammar
  instead of only `response_format={"type":"json_object"}`

Implication:

- the parser becomes the last defense, not the primary protocol

### 2. Treat reasoning as a separate channel, never as business payload

Reasoning-capable models may emit:

- explicit reasoning fields
- content blocks with multiple types
- `<think>...</think>` wrappers
- free-form internal scratchpad before the final answer

This project should never parse that text as business output.

Rules:

- parse only final structured payloads or tool arguments
- reasoning is optional metadata, not a source of truth
- if reasoning appears inside text, strip or ignore it before validation
- never couple product logic to a provider's chain-of-thought surface format

### 3. Constrain the task shape, not only the parser

A lot of parse fragility comes from prompts that ask for too much in one shot.

Recent provider-integration work already confirmed the better pattern:

- use one selector schema for "what step is current"
- use one content schema for "write this single file"
- avoid "generate the whole phase as one big JSON object"

This must become a general rule:

- selectors should be enums or single-ID responses
- actions should be tool calls or narrow JSON contracts
- large deliverables should be chunked into bounded tasks

### 4. Separate syntax validation from semantic validation

The system needs multiple gates:

1. transport success
2. provider response normalization
3. syntactic parsing
4. schema validation
5. semantic validation
6. bounded repair or retry policy

Example:

- `{}`
  - may be valid JSON
  - but invalid schema for `selected_step_node_id`
- a fenced JSON block
  - may be syntactically recoverable
  - but still semantically wrong if required fields are missing

### 5. Observability is part of the parser strategy

For production use, every LLM interaction should leave enough evidence to
explain failures.

We need to capture:

- provider
- model
- request contract name
- raw response body
- normalized response blocks
- parse outcome
- schema outcome
- semantic outcome
- retry count
- repair path used

Without that, parser quality cannot be improved safely.

## Gap Analysis Against Current State

### What is already better in this repo

The current hardened integration path already moved in the right direction:

- strips `<think>...</think>` in OpenAI-compatible responses
- retries once with a repair prompt when JSON is invalid
- reduces task width by generating one deliverable at a time
- falls back deterministically for step selection if the structured answer is
  weak

### What is still missing here

- no canonical provider-neutral response envelope yet
- Anthropic path still extracts the first `text` block and parses text instead
  of preferring tool blocks or schema-first transport contracts
- OpenAI-compatible path still relies on text parsing after transport, even if
  `response_format` is requested
- no explicit refusal/truncation taxonomy
- no shared schema registry for task types
- no evaluation corpus for malformed, partial, or reasoning-heavy responses

### What is weaker in the copied `swe-ai-fleet` implementation

- the parser only strips markdown fences
- it does not strip `<think>...</think>`
- it does not inspect provider-specific reasoning channels
- it does not distinguish syntactic validity from semantic validity
- it has no repair retry
- it treats raw `message.content` as the payload boundary

## Target Architecture

### Layer 1: Provider adapters

Each provider adapter should return a rich raw response object, not only a
plain string.

Minimum normalized fields:

- provider
- model
- finish reason
- refusal indicator if present
- content blocks
- tool calls if present
- raw text payload if present
- raw provider JSON for diagnostics

### Layer 2: Response normalization

Introduce a canonical envelope, for example:

- `LlmResponseEnvelope`
- `LlmContentBlock`
- `StructuredPayloadCandidate`
- `ReasoningMetadata`

This layer should:

- normalize OpenAI-compatible content
- normalize Anthropic content blocks
- preserve provider-specific metadata without leaking it into business logic

### Layer 3: Contract registry

Each agentic interaction should declare an explicit response contract.

Examples:

- `CurrentStepSelection`
- `SingleFileWritePlan`
- `TaskStatusSummary`
- `ToolInvocationDecision`

Each contract should define:

- JSON Schema
- semantic invariants
- retry or repair policy
- whether tool calling is preferred over raw structured output

### Layer 4: Validator pipeline

Pipeline for every call:

1. normalize provider response
2. extract best structured candidate
3. validate JSON syntax
4. validate against schema
5. validate semantic constraints
6. either accept or trigger bounded retry or repair

### Layer 5: Reasoning-safe orchestration

Reasoning models should be used like this:

- allow them to think internally if the provider supports it
- only consume final structured output or tool call payloads
- never branch domain logic directly on reasoning text

## Recommended Interaction Patterns

### Pattern A: Selectors

Use for:

- current step selection
- current node focus
- next action choice

Preferred contract:

- single enum or single node id

Avoid:

- prose explanations plus a choice embedded in text

### Pattern B: Tool decisions

Use for:

- whether to call a tool
- which tool to call
- with what arguments

Preferred contract:

- provider-native tool calling

Avoid:

- parsing tool directives from text

### Pattern C: Structured summaries

Use for:

- plan summaries
- status payloads
- checkpoint reports

Preferred contract:

- strict JSON schema

Avoid:

- markdown sections parsed with heuristics

### Pattern D: Artifact generation

Use for:

- code files
- markdown reports
- prompts

Preferred contract:

- one artifact per request
- explicit target path outside the LLM payload contract
- bounded content response

Avoid:

- monolithic multi-file plans in one JSON blob

## Provider-Specific Guidance

### OpenAI-compatible

Preferred order:

1. strict tool calling when the task is an action decision
2. structured outputs with JSON Schema for typed payloads
3. only then text parsing fallback

Rules:

- request machine-readable output explicitly
- capture finish reason and refusal signals
- treat `response_format=json_object` as weaker than schema-constrained output

### Anthropic

Preferred order:

1. tools with `input_schema`
2. typed content-block interpretation
3. text parsing only as fallback

Rules:

- inspect content block types, not only the first text block
- support thinking blocks without treating them as payload
- validate only the final structured content or tool input

### vLLM

Preferred order:

1. guided decoding / structured outputs with schema or grammar
2. strict parsing fallback

Rules:

- do not assume hosted-provider behavior from a self-hosted endpoint
- treat empty JSON objects and near-empty payloads as common failure modes
- validate the endpoint's actual support for structured generation, not only
  the client request shape

## Strategic Milestones

### Milestone 1: Canonical response envelope

Add provider-neutral normalized response objects and stop returning raw strings
from provider clients.

Exit gate:

- OpenAI-compatible and Anthropic adapters both return normalized envelopes

### Milestone 2: Contract registry and schema-first calls

Introduce explicit schemas for selector, planning, and artifact-generation
contracts.

Exit gate:

- selectors and single-file generation run through declared schemas

### Milestone 3: Tool-first reasoning interactions

Move action decisions to tool calling wherever supported.

Exit gate:

- no business action selection depends on text parsing

### Milestone 4: Validation and repair engine

Add bounded, typed retry and repair behavior with explicit failure reasons.

Exit gate:

- parse failures are categorized
- retries are measurable
- malformed payloads do not silently pass as valid enough

### Milestone 5: Evaluation corpus

Create a fixture corpus with provider-specific bad responses:

- `{}` empty object
- fenced JSON
- prose before JSON
- `<think>...</think>` before JSON
- truncated JSON
- refusal / blocked outputs

Exit gate:

- parser and validator behavior is regression-tested against the corpus

## Immediate Recommendations For This Repo

1. Introduce a shared `LlmResponseEnvelope` module and make provider clients
   return it.
2. Replace the Anthropic "first text block wins" path with block-aware
   extraction and schema-first handling.
3. Add an explicit schema contract for step selection and single-file
   generation.
4. Add a semantic validator layer that rejects empty-but-valid JSON payloads.
5. Capture raw provider responses and validation outcomes in structured logs.
6. Add a malformed-response corpus to tests before expanding more agentic
   scenarios.

## Non-Negotiable Rules

- do not parse chain-of-thought as business payload
- do not trust plain `message.content` as a stable contract
- do not treat valid JSON as sufficient without schema and semantic validation
- do not ask one model call to produce large multi-artifact payloads when those
  artifacts can be decomposed
- do not hide parser repair behavior; it must be observable

## Primary References

Provider references used as state-of-the-art direction:

- OpenAI Structured Outputs:
  - `https://platform.openai.com/docs/guides/structured-outputs`
- OpenAI Reasoning:
  - `https://platform.openai.com/docs/guides/reasoning`
- Anthropic Tool Use:
  - `https://docs.anthropic.com/en/docs/agents-and-tools/tool-use/overview`
- Anthropic Extended Thinking:
  - `https://docs.anthropic.com/en/docs/build-with-claude/extended-thinking`
- vLLM Structured Outputs:
  - `https://docs.vllm.ai/en/latest/features/structured_outputs.html`

Those provider-specific details will evolve, but the architectural direction
above should remain stable:

- schema-first
- tool-first where applicable
- provider normalization
- reasoning-safe consumption
- observable repair and validation
