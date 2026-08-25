# Rehydration Kernel Reuse Boundary

> Review required. This note was written in the compatibility-era migration
> context. The reuse advice may still contain useful architecture guidance, but
> the target boundaries must be revalidated against the current beta-only
> kernel contract.

This note defines what from this repo can be reused in the Context Service Rust
migration without allowing the internal kernel contract to become the external
service contract by accident.

## Boundary rule

This repo is allowed to remain internally node-centric.

The migration must reuse internal architecture and implementation patterns from
this repo, but it must not reuse the current public `underpass.rehydration`
contract as the external Context Service boundary.

## Safe reuse

These pieces are good candidates for direct reuse or light adaptation.

| Crate / area | Classification | Reuse rule |
| --- | --- | --- |
| `crates/rehydration-domain` | reusable with adaptation | keep graph-native core concepts; do not expose them directly as `fleet.context.v1` |
| `crates/rehydration-application` | reusable with adaptation | keep small use cases and query or command boundaries; add edge mappers outside the core |
| `crates/rehydration-adapter-neo4j` | reusable with adaptation | reuse graph neighborhood and write adapter patterns |
| `crates/rehydration-adapter-valkey` | reusable with adaptation | reuse extended node detail and snapshot patterns where compatible |
| `crates/rehydration-adapter-nats` | reusable with adaptation | reuse modular consumer and publisher structure, not the current subject set as public truth |
| `crates/rehydration-ports` | reusable with adaptation | keep port ownership explicit and small |
| `crates/rehydration-testkit` | reusable with adaptation | reuse harness structure, rebuild fixtures around the external contract |
| `crates/rehydration-config` | reusable with adaptation | keep config loading patterns, not the current env surface as public truth |
| `crates/rehydration-observability` | reusable with adaptation | reuse directly |
| `crates/rehydration-server` | reusable with adaptation | reuse as composition root pattern |

## Reuse only behind compatibility adapters

These areas are useful internally but must not leak through the external
boundary unchanged.

| Area | Why |
| --- | --- |
| `api/proto/underpass/rehydration/kernel/v1beta1/*` | kernel transport packages are useful internally, but they are not the external Context Service contract |
| `crates/rehydration-transport-grpc` | transport plumbing is useful, but the public service and message shapes diverge |
| `api/asyncapi/context-projection.v1beta1.yaml` | useful for internal projection eventing, not for the external Context Service subjects |
| graph-native bundle responses | good internal output, wrong external response contract |

## Do not treat as source of truth

The migration must not treat any of the following as the external source of
truth:

- canonical `underpass.rehydration.kernel.v1beta1`
- `root_node_id` as a public replacement for `story_id` or `case_id`
- current internal NATS subjects:
  - `graph.node.materialized`
  - `node.detail.materialized`
- current bundle transport model as a replacement for `PromptBlocks` or
  `RoleContextPack`

## Required compatibility pattern

The safe pattern for the migration is:

1. external request arrives in `fleet.context.v1` or on external NATS subjects
2. compatibility mapper converts it to internal node-centric inputs
3. internal core executes using graph-native models
4. compatibility mapper renders the external response contract

This pattern is also a structural rule:

- no god compatibility adapter
- no transport file that owns multiple unrelated mappings
- separate files for DTOs, request mappers, response mappers, handlers, and
  subject routers
- domain stays free of external legacy nouns

## Consequence

Future work should prefer:

- reusing internal domain, adapters, and testing patterns
- adding compatibility modules at the edge

Future work should avoid:

- changing the internal node-centric core to imitate the external legacy shape
- exposing current kernel transport packages as the public Context Service API
