# Node-Centric Implementation Plan

> **Historical (superseded — paths refactored).** Point-in-time plan; the graph-native bundle model below shipped, but the code paths/files referenced were refactored after this was written.

This is the implementation plan for the next slice of this repo.

## Goal

Finish the hard cut from the current hybrid query model to a pure graph-native
rehydration model:

- root node
- neighbor nodes
- relationships
- extended node detail from Valkey
- role-aware rendering on top of that graph bundle

## Current problem

The workspace already reads graph neighborhoods and node details correctly, but
it still collapses them into a semantic pack-shaped bundle before transport.

That hybrid layer is the main target for removal.

## Main blockers

### Domain

`crates/rehydration-domain/src/model/rehydration_bundle.rs`

- bundle still stores one semantic pack
- bundle still stores pre-rendered sections as source-of-truth data

### Application

`crates/rehydration-application/src/queries/node_centric_projection_reader.rs`

- reads graph-native data
- returns a semantic pack instead of a graph-native bundle

`crates/rehydration-application/src/queries/bundle_assembler.rs`

- placeholders and synthetic bundles are still semantic-pack based

`crates/rehydration-application/src/queries/get_context.rs`

- rendering still operates over `sections`

`crates/rehydration-application/src/queries/rehydrate_session.rs`

- still follows `load_pack -> assemble bundle`

`crates/rehydration-application/src/queries/rehydration_diagnostics.rs`

- diagnostics still derive from semantic categories rather than graph facts

### Transport

`api/proto/underpass/rehydration/kernel/v1beta1/common.proto`

- bundle contract is still hybrid

`crates/rehydration-transport-grpc/src/transport/proto_mapping.rs`

- transport still depends on semantic-pack mapping

### Persistence

`crates/rehydration-adapter-valkey`

- node-detail path is aligned
- snapshot serialization is still bundle-hybrid

## Target architecture

## 1. Domain bundle

Replace the current bundle model with graph-native types under
`crates/rehydration-domain/src/model/`:

- `rehydration_bundle.rs`
- `bundle_node.rs`
- `bundle_relationship.rs`
- `bundle_node_detail.rs`
- `rehydration_stats.rs`

Target bundle fields:

- `root_node_id`
- `role`
- `root_node`
- `neighbor_nodes`
- `relationships`
- `node_details`
- `metadata`
- `stats`

Rules:

- root node id must match the root node
- relationships must reference existing bundle nodes
- node details must reference existing bundle nodes
- stats are derived from the bundle

## 2. Application reader

Replace `load_pack()` with `load_bundle()`:

- read graph neighborhood
- read node details
- build one graph-native bundle

Suggested file rename:

- `node_centric_projection_reader.rs` -> `node_centric_bundle_reader.rs`

## 3. Assembly and rendering

Replace the current assembler with a graph-native assembler:

- placeholder bundle when root node is absent
- synthetic bootstrap bundle when needed
- no invented semantic objects

Replace the renderer so it builds output from:

1. root node
2. related nodes
3. relationships
4. extended node details

Role remains a rendering/filtering concern only.

## 4. Rehydrate session and diagnostics

Refactor session and diagnostics so they operate on graph-native bundles:

- session returns graph-native bundles per role
- diagnostics count nodes, relationships, and available details
- token estimates are derived from rendered output

## 5. Transport

Refactor `common.proto` and gRPC mapping so the query contract is graph-native.

Keep:

- `GraphNode`
- `GraphRelationship`

Add:

- `BundleNodeDetail`

Refactor bundle messages so they expose:

- root node
- neighbor nodes
- relationships
- node details
- stats
- version

After this cut, no transport mapping should depend on a semantic pack accessor.

## 6. Snapshots

Keep node detail in Valkey as-is directionally, but replace snapshot shape with
a graph-native bundle payload.

Snapshot payload must include:

- root node id
- role
- root node
- neighbor nodes
- relationships
- node details
- bundle metadata
- stats

## 7. Eventing

Keep the repo event model node-centric:

- `graph.node.materialized`
- `node.detail.materialized`

Do not widen the event model with unrelated domain vocabulary.

## Implementation sequence

### Step 1

Refactor the domain bundle and exports:

- `crates/rehydration-domain/src/model/rehydration_bundle.rs`
- `crates/rehydration-domain/src/model/mod.rs`
- `crates/rehydration-domain/src/lib.rs`

Add the new graph-native model files.

### Step 2

Replace the application reader flow:

- `crates/rehydration-application/src/queries/node_centric_projection_reader.rs`
- `crates/rehydration-application/src/queries/mod.rs`
- `crates/rehydration-application/src/lib.rs`

### Step 3

Replace assembly, rendering, session flow, and diagnostics:

- `crates/rehydration-application/src/queries/bundle_assembler.rs`
- `crates/rehydration-application/src/queries/get_context.rs`
- `crates/rehydration-application/src/queries/rehydrate_session.rs`
- `crates/rehydration-application/src/queries/rehydration_diagnostics.rs`

### Step 4

Replace proto and transport mapping:

- `api/proto/underpass/rehydration/kernel/v1beta1/common.proto`
- `api/proto/underpass/rehydration/kernel/v1beta1/query.proto`
- `api/proto/underpass/rehydration/kernel/v1beta1/admin.proto`
- `crates/rehydration-transport-grpc/src/transport/proto_mapping.rs`

### Step 5

Replace snapshot serialization:

- `crates/rehydration-adapter-valkey/src/adapter/serialization.rs`
- related snapshot store code

### Step 6

Clean remaining hybrid references from exports, tests, and mapping code.

## Non-goals

- adding unrelated domain aggregates
- adding unrelated event subjects
- widening the current hybrid bundle model
- building a detached standalone domain model outside the graph-centered repo

## Acceptance criteria

1. The bundle is graph-native.
2. The main query reader no longer returns a semantic pack.
3. Rendering uses nodes, relationships, and node details.
4. Snapshot payloads are graph-native.
5. gRPC mapping no longer depends on semantic-pack accessors.
6. No new domain vocabulary is introduced into the core.

## Validation

- `cargo check --workspace --locked`
- `cargo test --workspace --locked`
- `cargo test -p rehydration-transport-grpc`
- `cargo test -p rehydration-adapter-neo4j`
- `cargo test -p rehydration-adapter-valkey`

Regression search:

- `rg "RoleContextPack|CaseHeader|PlanHeader|WorkItem|TaskImpact|DecisionRelation|bundle\\.pack\\(" crates`

Residual matches must be intentional and outside the main query path.
