# Context Service Phase 0 Contract Freeze

> **Historical (superseded).** Point-in-time record; code paths/links below (the source-of-truth plan and the removed swe-ai-fleet evidence dir) were refactored or deleted after this was written.

> Obsolete. This freeze captured the removed compatibility boundary. It remains
> useful only as historical evidence and should not be treated as a current
> kernel contract.

Status: Complete
Source of truth: [`CONTEXT_SERVICE_RUST_MIGRATION_PLAN.md`](./CONTEXT_SERVICE_RUST_MIGRATION_PLAN.md)

## Purpose

Freeze the observable contract of the existing Context Service before building
the Rust compatibility shell.

This freeze applies to the external service boundary only.
It does not change the internal language of this repo, which remains
node-centric:

- root node
- neighbor nodes
- relationships
- extended node detail in Valkey

## Evidence Used

Primary sources:

- [`context.proto`](../../to-delete-when-finish-the-standalone-extraction/swe-ai-fleet-base-for-context-extraction/specs/fleet/context/v1/context.proto)
- [`services/context/README.md`](../../to-delete-when-finish-the-standalone-extraction/swe-ai-fleet-base-for-context-extraction/services/context/README.md)
- [`services/context/server.py`](../../to-delete-when-finish-the-standalone-extraction/swe-ai-fleet-base-for-context-extraction/services/context/server.py)
- [`services/context/nats_handler.py`](../../to-delete-when-finish-the-standalone-extraction/swe-ai-fleet-base-for-context-extraction/services/context/nats_handler.py)
- [`core/context/adapters/env_config_adapter.py`](../../to-delete-when-finish-the-standalone-extraction/swe-ai-fleet-base-for-context-extraction/core/context/adapters/env_config_adapter.py)

Behavioral evidence:

- [`test_context_service_servicer.py`](../../to-delete-when-finish-the-standalone-extraction/swe-ai-fleet-base-for-context-extraction/services/context/tests/unit/test_context_service_servicer.py)
- [`test_nats_handler.py`](../../to-delete-when-finish-the-standalone-extraction/swe-ai-fleet-base-for-context-extraction/services/context/tests/unit/test_nats_handler.py)
- [`test_nats_messaging_adapter.py`](../../to-delete-when-finish-the-standalone-extraction/swe-ai-fleet-base-for-context-extraction/services/context/tests/unit/infrastructure/adapters/test_nats_messaging_adapter.py)
- [`test_env_config_adapter.py`](../../to-delete-when-finish-the-standalone-extraction/swe-ai-fleet-base-for-context-extraction/core/context/tests/unit/adapters/test_env_config_adapter.py)
- [`test_required_envelope_parser.py`](../../to-delete-when-finish-the-standalone-extraction/swe-ai-fleet-base-for-context-extraction/core/shared/tests/unit/events/infrastructure/test_required_envelope_parser.py)
- [`test_server.py`](../../to-delete-when-finish-the-standalone-extraction/swe-ai-fleet-base-for-context-extraction/services/context/tests/unit/test_server.py)

## Frozen External gRPC Contract

Package and service identity:

- package: `fleet.context.v1`
- service: `ContextService`

Frozen RPC inventory:

1. `GetContext`
2. `UpdateContext`
3. `RehydrateSession`
4. `ValidateScope`
5. `CreateStory`
6. `CreateTask`
7. `AddProjectDecision`
8. `TransitionPhase`
9. `GetGraphRelationships`

Frozen public field names that must be preserved at the boundary:

- `story_id`
- `case_id`
- `task_id`
- `subtask_id`
- `entity_type`
- `payload`
- `blocks`
- `packs`
- `PromptBlocks`
- `RoleContextPack`

Phase 1 and Phase 2 may adapt these names internally, but they may not rename
them on the public interface.

## Frozen External NATS Contract

Consumed planning subjects:

- `planning.project.created`
- `planning.epic.created`
- `planning.story.created`
- `planning.task.created`
- `planning.story.transitioned`
- `planning.plan.approved`

Consumed orchestration subjects:

- `orchestration.deliberation.completed`
- `orchestration.task.dispatched`

Async request subjects:

- `context.update.request`
- `context.rehydrate.request`

Published subjects:

- `context.update.response`
- `context.rehydrate.response`
- `context.events.updated`

Published envelope event types proven in adapter tests:

- `context.update.response` -> `event_type=context.update.response`
- `context.rehydrate.response` -> `event_type=context.rehydrate.response`
- `context.events.updated` -> `event_type=context.updated`

## Frozen EventEnvelope Rules

Inbound async request messages are wrapped in a required `EventEnvelope`.
They are not raw request payloads.

Required fields:

- `event_type`
- `payload`
- `idempotency_key`
- `correlation_id`
- `timestamp`
- `producer`

Optional fields:

- `causation_id`
- `metadata`

Observed and frozen handler behavior:

- invalid JSON -> `ack` and drop
- invalid or incomplete envelope -> `ack` and drop
- envelope payload not being an object -> `ack` and drop
- handler or servicer failure after successful parsing -> `nak`

## Frozen Startup and Configuration Behavior

Defaults and validations frozen from the Python implementation:

- `GRPC_PORT`: default `50054`
- `NEO4J_URI`: default `bolt://neo4j:7687`
- `NEO4J_USER`: default `neo4j`
- `NEO4J_PASSWORD`: required
- `REDIS_HOST`: default `redis`
- `REDIS_PORT`: default `6379`
- `NATS_URL`: default `nats://nats:4222`
- `ENABLE_NATS`: boolean, default `true`

Observed fail-fast behavior:

- missing or empty `NEO4J_PASSWORD` fails config loading
- invalid `REDIS_PORT` fails config loading
- `ENABLE_NATS=false` still parses successfully, but startup aborts because
  NATS is required for the service

Observed lenient behavior:

- missing scopes YAML -> empty config
- invalid scopes YAML -> empty config

This leniency is part of the frozen external baseline until explicitly changed
in a later migration decision.

## Frozen Observable Behavior

Only behaviors proven by source or tests are frozen here.

### gRPC

- `GetGraphRelationships.depth` is clamped to `1..3`
- `GetGraphRelationships` maps validation errors to `INVALID_ARGUMENT`
- unexpected servicer failures are mapped to `INTERNAL`

### Async NATS

- request handlers invoke servicer methods through an internal servicer context
- if the servicer sets an error on that context, the handler raises and `nak`s
  the original message

## Compatibility Boundary Rule

The migration keeps a strict split:

1. external callers keep speaking `fleet.context.v1`
2. external NATS flows keep using the existing subject names and envelope rules
3. compatibility mappers translate external nouns into node-centric inputs
4. internal application code stays graph-native
5. compatibility mappers render the external response DTOs on the way out

This means external nouns are allowed only in transport and adapter code.
They must not leak into the domain core of this repo.

## Non-Blocking Open Questions

Phase 0 is still considered complete because these questions do not block the
compatibility shell design:

- exact text parity for all gRPC error messages is not fully frozen yet
- response ordering is frozen only where tests or source prove determinism
- deduplication and retry policy beyond current `ack` or `nak` behavior is not
  specified by the baseline

These are explicit Phase 1 and Phase 3 verification tasks, not reasons to
reopen Phase 0.

## Phase 0 Exit Criteria

Phase 0 is closed because the repo now has:

- this contract freeze
- a compatibility gap inventory
- a golden test catalog
- a reuse-boundary note
- an execution roadmap that starts the compatibility shell next

## Consequence for Phase 1

The next phase must not continue redesigning the internal core in search of
compatibility.

The next phase must build:

- a `fleet.context.v1` transport shell
- external DTOs and mappers at the edge
- external NATS routers and envelope handling
- golden tests that validate the boundary against this freeze
