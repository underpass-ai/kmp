# Relation Explanation Model

Status: Active (implemented in domain, proto, asyncapi, and paper artifact)

## Intent

Define how a relationship between two nodes should carry explanatory context in
`rehydration-kernel`.

The kernel already models:

- root node
- neighbor nodes
- relationships
- extended node detail

This document strengthens the meaning of `relationships` so they can express
more than connectivity.

## Core Rule

A relationship may explain why the target node exists, why it became relevant,
or how it was produced.

That means a relationship can represent:

- cause
- motivation
- justification
- procedure
- evidence
- constraint

Not every relationship is explanatory. Some remain purely structural.

## Semantic Classes

The kernel should treat explanatory semantics as data carried by the producing
application or agent, not as something inferred by the kernel itself.

Recommended `semantic_class` values:

- `structural`
  - containment, membership, composition
- `causal`
  - one node triggered or produced another
- `motivational`
  - one node justifies or authorizes another
- `procedural`
  - one node explains how another was executed
- `evidential`
  - one node validates, proves, or verifies another
- `constraint`
  - one node limits or shapes another

## Ownership Boundary

The kernel does not invent explanatory semantics.

The explanatory metadata must be provided by:

- an agent runtime
- an orchestration component
- a product-specific application
- any upstream producer with domain authority

The kernel is responsible for:

- preserving relation metadata
- storing it
- rehydrating it
- exposing it through contracts
- rendering it into bounded context

## Why Nodes Still Matter

Relationships should not replace nodes.

Recommended split:

- node:
  - the main entity, decision, task, incident, artifact, checklist, evidence
- relationship:
  - the minimal explanation of how the source node relates to the target node

Recommended pattern:

- short explanation on the relationship
- long explanation in node detail

## Recommended Relation Properties

These keys stay generic and avoid product-specific nouns.

- `semantic_class`
  - one of the classes above
- `rationale`
  - short explanation of why the transition exists
- `motivation`
  - alternate short explanation when the transition is goal-driven
- `method`
  - brief note on how something was executed
- `decision_id`
  - id of the decision node that produced or authorized the target
- `caused_by_node_id`
  - id of the node that directly produced the target when distinct from source
- `evidence`
  - concise evidence marker or identifier
- `confidence`
  - optional confidence level or score encoded as a string
- `sequence`
  - narrative or procedural order within siblings

## Examples

### Decision Motivation

`incident -> decision`

- `relation_type=triggers`
- `semantic_class=causal`
- `rationale=containment margin dropped below threshold`

### Decision To Task

`decision -> task`

- `relation_type=authorizes`
- `semantic_class=motivational`
- `rationale=reserve power must be diverted before repair`
- `decision_id=decision:reroute-reserve-power`

### Task To Evidence

`task -> artifact`

- `relation_type=verified_by`
- `semantic_class=evidential`
- `method=post-reroute telemetry validation`
- `evidence=telemetry:capture:segment-c`

## Rendering Guidance

When bounded context is rendered for an agent, the relationship text should
prefer explanatory metadata when present.

Example style:

- `Relationship decision-1 --AUTHORIZES--> task-7 because reserve power must be diverted before repair`
- `Relationship task-7 --VERIFIED_BY--> artifact-3 via post-reroute telemetry validation`

If no explanatory metadata exists, the kernel should fall back to the plain
relationship rendering.

## Ordering Guidance

If `sequence` is present, relation ordering should prefer it over purely
lexicographic ordering.

Fallback remains deterministic lexicographic ordering when no sequence exists.

## Non-Goals

This model does not require:

- product-specific relation enums in the kernel core
- kernel-side inference of motivation or procedure
- large free-form paragraphs on every relationship

The goal is preservation and reuse of upstream explanatory intent.
