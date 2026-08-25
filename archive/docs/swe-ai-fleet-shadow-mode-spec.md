# swe-ai-fleet Shadow Mode Spec

> **Historical (superseded).** Point-in-time record; the externally-owned `swe-ai-fleet` program and some kernel-dependency links below were refactored after this was written.

Status: planned
Scope: external to this repo, owned by `swe-ai-fleet`

## Purpose

Define how `swe-ai-fleet` should compare the legacy context service against
`rehydration-kernel` before cutover.

This spec lives here because the kernel must be explicit about what an
integrating product should compare, but the implementation remains outside this
repo.

## Ownership Boundary

The kernel owns:

- deterministic enough contract examples
- observable request or response behavior
- stable node-centric boundary inputs and outputs

`swe-ai-fleet` owns:

- dual calling of old and new systems
- normalization
- mismatch storage and dashboards
- rollout flags
- cutover and rollback

## Shadowing Goal

Run the legacy context service and `rehydration-kernel` side by side for the
same product request and compare normalized outcomes without changing live user
behavior.

Shadow mode must stay read-only until mismatch rates are understood and
accepted.

## In-Scope Flows

### Read flows

- get context
- rehydrate session
- validate scope
- graph relationships

### Async flows

- product-owned async events translated into kernel-owned operations
- kernel publications observed for parity and operational correctness

## Out Of Scope

- moving `planning.*` or `orchestration.*` into the kernel
- forcing the kernel to emit product-specific subjects
- changing kernel domain language to match legacy nouns

## Comparison Inputs

For each shadowed request, `swe-ai-fleet` should record:

- normalized request id
- caller or tenant context if applicable
- legacy request payload
- kernel request payload
- time of request
- adapter version
- kernel version or commit

## Normalization Rules

Normalization must happen inside `swe-ai-fleet`, not in the kernel.

Normalize before comparison:

- ordering where contract order is not semantically relevant
- whitespace-only differences in rendered text where explicitly accepted
- timestamps or durations if the legacy path is nondeterministic
- envelope metadata that is operational rather than semantic

Do not normalize away:

- missing nodes
- wrong focused node
- scope differences
- render budget differences
- missing extended detail
- materially different rendered context

## Required Comparison Dimensions

### 1. Selected graph

Compare:

- root node id
- selected neighbor node ids
- relationship ids or normalized tuples
- presence or absence of node detail

Mismatch classes:

- missing root
- wrong focus
- missing neighbors
- extra neighbors
- missing relationships
- extra relationships
- detail mismatch

### 2. Rendered context

Compare:

- rendered text presence
- normalized rendered text
- section presence
- presence of focus-specific detail

Mismatch classes:

- empty render
- truncated render
- missing focus detail
- materially different render

### 3. Budget and scope behavior

Compare:

- effective token budget
- whether output respects budget expectations
- scope allow or deny result
- missing or extra scope items

Mismatch classes:

- budget overflow
- budget underuse with missing key context
- wrong allow or deny decision
- wrong missing or extra scope set

### 4. Snapshot behavior

Compare where applicable:

- snapshot created or not
- snapshot lookup success
- effective TTL bucket

Mismatch classes:

- missing snapshot
- wrong TTL family
- stale snapshot behavior

## Read Shadow Execution Model

For each eligible request:

1. `swe-ai-fleet` serves the live response from the legacy path.
2. In parallel or asynchronously, it calls `rehydration-kernel`.
3. It normalizes both outputs.
4. It computes mismatch categories.
5. It emits telemetry and stores a sampled diff artifact.

The user-visible response must remain the legacy one until cutover is approved.

## Async Shadow Execution Model

For each eligible product event:

1. `swe-ai-fleet` consumes the product-owned subject.
2. It executes the existing legacy path.
3. It also translates the event into kernel-owned operations.
4. It observes publication success, failures, and result shape.
5. It records parity data without changing user-visible behavior.

## Telemetry Requirements

At minimum, emit:

- shadow request count
- shadow success count
- mismatch rate
- mismatch rate by class
- legacy latency
- kernel latency
- adapter failure rate
- stale or missing node detail rate
- snapshot hit or miss rate

Recommended dimensions:

- product flow
- tenant or workspace family if available
- role
- scope family
- root node kind

## Artifact Storage

For sampled mismatches, store:

- normalized request
- normalized legacy output
- normalized kernel output
- mismatch classes
- adapter version
- kernel commit

Avoid storing raw sensitive payloads unless required by product policy.

## Acceptance Thresholds

Shadow mode should not advance to cutover until `swe-ai-fleet` defines explicit
thresholds for:

- total mismatch rate
- mismatch rate by severity
- render mismatch rate
- focus-selection mismatch rate
- snapshot failure rate
- latency regression tolerance

Thresholds are product decisions and must not be hardcoded in the kernel.

## Rollout Gates

### Gate 1: Read shadowing enabled

Requirements:

- read shadow path runs in production-like traffic
- mismatch telemetry is stable
- no user-visible dependency on kernel results

### Gate 2: Read cutover candidate

Requirements:

- mismatch rates stay within agreed thresholds
- critical mismatch classes are near zero
- latency is acceptable

### Gate 3: Async shadowing enabled

Requirements:

- product-owned subjects are translated cleanly
- publish failures and retries are observable
- no kernel-side fleet nouns are introduced

### Gate 4: Full cutover candidate

Requirements:

- read parity accepted
- async parity accepted
- rollback switch tested

## Rollback Requirements

`swe-ai-fleet` must be able to:

- disable read cutover independently
- disable async cutover independently
- keep telemetry running during rollback
- preserve mismatch evidence after rollback

## Dependencies From This Repo

This spec assumes `swe-ai-fleet` depends only on kernel-owned artifacts such as:

- [`kernel-node-centric-integration-contract.md`](migration/kernel-node-centric-integration-contract.md)
- [`kernel-runtime-integration-reference.md`](migration/kernel-runtime-integration-reference.md)
- [`api/examples/README.md`](../../api/examples/README.md)

## Exit Statement

Shadow mode is complete when `swe-ai-fleet` can prove that:

- legacy and kernel outputs are comparable through a stable normalization layer
- drift is measurable and operationally explainable
- rollout decisions can be made without changing kernel code
