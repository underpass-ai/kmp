# Context Service Golden Tests

> Obsolete. These golden notes describe the removed compatibility boundary and
> its historical test matrix. Do not use them as guidance for current kernel
> tests.

This catalog defines the minimum golden tests required to preserve the frozen
external Context Service contract while the Rust implementation stays
internally node-centric.

## Principles

1. Assert only externally observable behavior.
2. Keep internal graph-native DTOs out of golden assertions.
3. Normalize only what the baseline proves to be nondeterministic.
4. Use the Python service as the oracle until the Rust compatibility shell is
   fully trusted.

## Oracle Sources

Baseline evidence must come from one of these sources:

- `specs/fleet/context/v1/context.proto`
- `services/context/README.md`
- Python unit tests under `services/context/tests/unit/`
- Python unit tests under `core/context/tests/unit/`

If a rule is not proven by one of those sources, it must be treated as
unfrozen and explicitly documented before being added to a golden assertion.

## Exact-Match Fields

These must match exactly unless the baseline proves otherwise:

- RPC names and message field names
- `story_id`, `case_id`, `task_id`, `subtask_id`
- `ContextChange.entity_type`
- `ContextChange.payload`
- `PromptBlocks`
- `packs` keys and nested public fields
- NATS subjects
- `EventEnvelope.event_type`

## Allowed Normalization

Allowed:

- timestamps
- generated-at values
- correlation IDs
- idempotency keys
- ordering only when the Python baseline is explicitly unstable

Not allowed:

- renaming fields
- collapsing `PromptBlocks` into one string
- replacing external `packs` with internal bundle DTOs
- changing envelope `event_type`

## Required gRPC Golden Tests

| Fixture ID | RPC | Scenario | Oracle | Compare |
| --- | --- | --- | --- | --- |
| `grpc-get-context-basic` | `GetContext` | valid identifier, role, phase | proto plus baseline servicer tests | `context`, `token_count`, `scopes`, `version`, `blocks` |
| `grpc-get-context-subtask-focus` | `GetContext` | request includes `subtask_id` | baseline service behavior | subtask-focused response shape |
| `grpc-get-context-internal-error` | `GetContext` | internal failure path | servicer tests | gRPC status and externally visible error behavior |
| `grpc-update-context-basic` | `UpdateContext` | valid `changes` list | proto plus baseline tests | `version`, `hash`, `warnings` |
| `grpc-rehydrate-session-basic` | `RehydrateSession` | one case, multiple roles | proto plus baseline service behavior | `case_id`, `generated_at_ms`, `packs`, `stats` |
| `grpc-validate-scope-allowed` | `ValidateScope` | valid scopes | proto plus baseline tests | `allowed`, `missing`, `extra`, `reason` |
| `grpc-validate-scope-rejected` | `ValidateScope` | invalid scopes | proto plus baseline tests | `allowed`, `missing`, `extra`, `reason` |
| `grpc-create-story-basic` | `CreateStory` | valid create request | proto | `context_id`, `story_id`, `current_phase` |
| `grpc-create-task-basic` | `CreateTask` | valid create request | proto | `task_id`, `story_id`, `status` |
| `grpc-add-project-decision-basic` | `AddProjectDecision` | valid create request | proto | `decision_id` |
| `grpc-transition-phase-basic` | `TransitionPhase` | valid phase transition | proto | `story_id`, `current_phase`, `transitioned_at` |
| `grpc-get-graph-relationships-basic` | `GetGraphRelationships` | valid `node_id`, `node_type`, `depth=2` | proto plus use-case and servicer tests | `node`, `neighbors`, `relationships`, `success`, `message` |
| `grpc-get-graph-relationships-depth-clamp` | `GetGraphRelationships` | request `depth=5` | source plus tests | effective depth clamped to `3` |
| `grpc-get-graph-relationships-invalid-node-type` | `GetGraphRelationships` | invalid node type | servicer tests | `INVALID_ARGUMENT` |
| `grpc-get-graph-relationships-node-not-found` | `GetGraphRelationships` | valid node type, missing node | use-case and servicer tests | `INVALID_ARGUMENT` |

## Required Async NATS Golden Tests

| Fixture ID | Subject | Scenario | Oracle | Verify |
| --- | --- | --- | --- | --- |
| `nats-update-request-valid-envelope` | `context.update.request` | valid envelope and payload | handler tests | servicer invoked, response published, message acked |
| `nats-update-request-invalid-json` | `context.update.request` | invalid JSON | handler tests | message acked and dropped |
| `nats-update-request-invalid-envelope` | `context.update.request` | missing required envelope fields | handler tests | message acked and dropped |
| `nats-update-request-payload-not-object` | `context.update.request` | payload is not an object | handler tests | message acked and dropped |
| `nats-update-request-servicer-error` | `context.update.request` | servicer sets error on internal context | handler tests | message nacked |
| `nats-rehydrate-request-valid-envelope` | `context.rehydrate.request` | valid envelope and payload | handler tests | servicer invoked, response published, message acked |
| `nats-rehydrate-request-invalid-envelope` | `context.rehydrate.request` | invalid envelope | handler tests | message acked and dropped |
| `nats-rehydrate-request-servicer-error` | `context.rehydrate.request` | servicer sets error on internal context | handler tests | message nacked |

## Required Publish-Path Golden Tests

| Fixture ID | Subject | Oracle | Verify |
| --- | --- | --- | --- |
| `nats-publish-update-response` | `context.update.response` | adapter tests | subject, `event_type=context.update.response`, payload family |
| `nats-publish-rehydrate-response` | `context.rehydrate.response` | adapter tests | subject, `event_type=context.rehydrate.response`, payload family |
| `nats-publish-context-updated` | `context.events.updated` | adapter tests | subject, `event_type=context.updated`, payload family |

## Required Configuration Golden Tests

| Fixture ID | Surface | Oracle | Verify |
| --- | --- | --- | --- |
| `config-defaults` | env config adapter | config adapter tests | documented defaults |
| `config-missing-neo4j-password` | startup config | config adapter tests | fail-fast |
| `config-invalid-redis-port` | startup config | config adapter tests | fail-fast |
| `config-enable-nats-false` | startup | server and config tests | parses false, startup rejects it |
| `config-missing-scopes-yaml` | scope loading | server tests | empty config fallback |
| `config-invalid-scopes-yaml` | scope loading | server tests | empty config fallback |

## Implementation Order

Add golden tests in this order:

1. compatibility transport boot and error mapping
2. read RPCs
3. `GetGraphRelationships` parity and depth clamp
4. async NATS request and reply
5. write RPCs
6. startup and configuration parity

## Phase Gates

Phase 1 is not complete until:

- compatibility transport boots with the external package and service names
- error-mapping golden tests exist for the compatibility surface

Phase 2 is not complete until:

- read-side golden tests pass
- `GetGraphRelationships` parity tests pass
- external response DTOs match the frozen contract

Phase 3 is not complete until:

- each async request subject has at least one valid and one invalid golden test
- publish-path envelope tests pass

Phase 4 is not complete until:

- all write-path golden tests pass
- response publication parity is verified for write flows
