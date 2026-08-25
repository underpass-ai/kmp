# Context Service Compatibility Matrix

> Obsolete. This document describes a compatibility surface that has been
> removed from the kernel repo after the `v1beta1` cut. Keep it only as
> migration history; do not use it to drive current implementation changes.

This matrix compares the external Context Service contract frozen in Phase 0
against the current `main` branch of this repo.

Direction update:

- rows that refer to `swe-ai-fleet` legacy transport remain here for migration
  visibility
- future implementation of those legacy-specific adapters should happen in
  `swe-ai-fleet`, not in this repo
- this repo should stay node-centric and generic

Status values:

- `match`
- `partial`
- `diverged`
- `missing`

## gRPC Identity and Service Layout

| Surface | External source of truth | Current repo state | Status | Required action | Target phase |
| --- | --- | --- | --- | --- | --- |
| package name | `fleet.context.v1` | `underpass.rehydration.kernel.v1beta1` | diverged | add compatibility proto package and transport shell | Phase 1 |
| service shape | one `ContextService` | three services: query, command, admin | diverged | expose one compatibility service at the edge | Phase 1 |
| read RPC family | `GetContext`, `RehydrateSession`, `ValidateScope`, `GetGraphRelationships` | compatibility facade routes the frozen read RPCs over the node-centric application layer | match | keep parity with golden tests and parity report | Phase 2 |
| write RPC family | `UpdateContext`, `CreateStory`, `CreateTask`, `AddProjectDecision`, `TransitionPhase` | only `UpdateContext` exists in current public transport | partial | add missing command paths and edge mapping | Phase 4 |

## RPC Coverage

| RPC | External source of truth | Current repo state | Status | Required action | Target phase |
| --- | --- | --- | --- | --- | --- |
| `GetContext` | required | implemented on compatibility shell with focused request and response mapping | match | keep phase, focus, and budget semantics at the edge | Phase 2 |
| `UpdateContext` | required | present with different field names | partial | preserve external field names and semantics at boundary | Phase 4 |
| `RehydrateSession` | required | implemented on compatibility shell with external `packs` rendering and snapshot TTL propagation | match | keep snapshot TTL and timeline defaults frozen at the edge | Phase 2 |
| `ValidateScope` | required | implemented on compatibility shell with compatibility vocabulary and result mapping | match | keep compatibility scope catalog at the edge | Phase 2 |
| `CreateStory` | required | absent | missing | add compatibility command path | Phase 4 |
| `CreateTask` | required | absent | missing | add compatibility command path | Phase 4 |
| `AddProjectDecision` | required | absent | missing | add compatibility command path | Phase 4 |
| `TransitionPhase` | required | absent | missing | add compatibility command path | Phase 4 |
| `GetGraphRelationships` | required | implemented on compatibility shell with parity validation rules | match | keep depth clamp and invalid or missing node handling frozen | Phase 2 |

## Boundary Naming and DTO Shape

| Surface | External source of truth | Current repo state | Status | Required action | Target phase |
| --- | --- | --- | --- | --- | --- |
| primary identifiers | `story_id`, `case_id` | `root_node_id` | diverged | map at transport edge only | Phase 1 |
| task focus identifiers | `task_id`, `subtask_id` | `work_item_id` or no direct equivalent | diverged | keep external names at edge and map internally | Phase 1 |
| update entity field | `entity_type` | `entity_kind` | diverged | preserve external field name | Phase 1 |
| update payload field | `payload` | `payload_json` | diverged | preserve external field name | Phase 1 |
| `GetContextResponse` | `context`, `token_count`, `scopes`, `version`, `blocks` | compatibility renderer maps graph-native result into frozen external DTO | match | keep parity proven by golden tests | Phase 2 |
| `RehydrateSessionResponse` | `case_id`, `generated_at_ms`, `packs`, `stats` | compatibility renderer maps graph-native result into frozen external DTO | match | keep parity proven by golden tests | Phase 2 |
| `PromptBlocks` | public message | exposed only on compatibility shell | match | keep edge-only DTO | Phase 2 |
| `RoleContextPack` | public message | exposed only on compatibility shell | match | keep edge-only DTO | Phase 2 |

## NATS Contract

| Surface | External source of truth | Current repo state | Status | Required action | Target phase |
| --- | --- | --- | --- | --- | --- |
| planning consumed subjects | six `planning.*` subjects | intentionally absent from the generic kernel | missing | implement in `swe-ai-fleet` anti-corruption layer, not here | swe-ai-fleet integration |
| orchestration consumed subjects | `orchestration.deliberation.completed`, `orchestration.task.dispatched` | intentionally absent from the generic kernel | missing | implement in `swe-ai-fleet` anti-corruption layer, not here | swe-ai-fleet integration |
| async request subjects | `context.update.request`, `context.rehydrate.request` | compatibility consumer implemented and wired through JetStream runtime | match | keep parity covered by runtime integration tests | Phase 3 |
| async response subjects | `context.update.response`, `context.rehydrate.response` | compatibility envelope publication implemented and wired through JetStream runtime | match | keep parity covered by runtime integration tests | Phase 3 |
| context-updated event | `context.events.updated` | compatibility publisher implemented and proven against a real NATS runtime | match | keep generic publication support in the kernel; emit any fleet-specific triggers from `swe-ai-fleet` | Phase 3 |
| request wrapper | required `EventEnvelope` | compatibility parser enforces the required envelope and is wired into runtime subscriptions | match | keep parity covered by runtime integration tests | Phase 3 |
| current internal subjects | not part of external contract | `graph.node.materialized`, `node.detail.materialized` | diverged | keep as internal projection traffic only | Phase 1 |

## Behavioral Parity

| Surface | External source of truth | Current repo state | Status | Required action | Target phase |
| --- | --- | --- | --- | --- | --- |
| `GetGraphRelationships.depth` | clamp to `1..3` | compatibility shell clamps to the frozen bounds | match | keep parity covered by tests | Phase 2 |
| validation error mapping | `INVALID_ARGUMENT` in tested paths | current transport uses internal status mapping | partial | freeze boundary error mapping | Phase 1 |
| unexpected error mapping | `INTERNAL` in tested paths | current transport uses repo-local mapping | partial | align compatibility surface | Phase 1 |
| invalid inbound async JSON | `ack` and drop | compatibility consumer acks and drops invalid JSON | match | keep parity covered by golden tests | Phase 3 |
| invalid inbound envelope | `ack` and drop | compatibility consumer acks and drops invalid envelope or non-object payload | match | keep parity covered by golden tests | Phase 3 |
| post-parse handler failure | `nak` | compatibility consumer naks on application or publish failure | match | keep parity covered by golden tests | Phase 3 |

## Configuration and Startup

| Surface | External source of truth | Current repo state | Status | Required action | Target phase |
| --- | --- | --- | --- | --- | --- |
| `GRPC_PORT` default | `50054` | different bootstrap surface | diverged | add compatibility config mapping | Phase 1 |
| `NEO4J_PASSWORD` required | yes | graph access exists, but not under the same public config surface | partial | preserve fail-fast behavior | Phase 1 |
| `REDIS_HOST` and `REDIS_PORT` defaults | `redis`, `6379` | current config differs | diverged | add compatibility env mapping | Phase 1 |
| `NATS_URL` default | `nats://nats:4222` | compatibility NATS config preserves the frozen default | match | keep parity at the compatibility edge | Phase 3 |
| `ENABLE_NATS` behavior | parsed as bool; startup fails if false | compatibility NATS config preserves the bool parse and server startup fails fast when disabled | match | keep parity at the compatibility edge | Phase 3 |
| scopes YAML fallback | missing or invalid file -> empty config | current scope path differs | diverged | preserve or document intentional deviation before rollout | Phase 1 |

## Internal Architecture Position

| Surface | External source of truth | Current repo state | Status | Required action | Target phase |
| --- | --- | --- | --- | --- | --- |
| internal domain language | unspecified by external contract | node-centric core | match | keep | all phases |
| location of legacy vocabulary | boundary concern only | not implemented yet | missing | keep external nouns confined to adapters and transport | Phase 1 |

## Closeout

The migration direction is now inverted:

- `rehydration-kernel` stays generic and node-centric
- `swe-ai-fleet` owns legacy compatibility and subject translation

The next slice is therefore not more fleet-specific transport inside this repo.
The next slice is freezing and documenting the node-centric boundary so another
system can integrate without contaminating the kernel.
