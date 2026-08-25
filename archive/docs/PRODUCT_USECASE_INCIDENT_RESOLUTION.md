# Production Incident Resolution — Kernel Perspective

> Archived on 2026-04-09. This document reflects an earlier product write-path
> assumption around `UpdateContext` and should not be used as current guidance
> for model-driven graph ingestion.

## What the Kernel Provides

The Rehydration Kernel is a **causal memory system** that stores knowledge
graphs of decisions, incidents, and operational history. For the production
incident resolution use case, it provides:

- **Causal knowledge graph** (Neo4j) with typed explanatory relationships
  (causal, motivational, evidential, constraint, procedural, structural)
- **Multi-resolution rendering**: L0 Summary (~100 tokens), L1 Causal Spine
  (~500 tokens), L2 Evidence Pack (remaining budget)
- **Role-scoped rehydration bundles** via `RehydrateSession` — each agent
  gets only the context relevant to its role
- **Scope validation** per agent role — enforces what context an agent can see
- **UpdateContext** for appending new nodes/relationships with optimistic
  concurrency (revision + content_hash)
- **CQRS + Event Sourcing** via NATS JetStream — append-only event store
  with durable projection consumers

---

## How the Kernel Serves This Use Case

### First incident (empty kernel)

The kernel starts **empty**. There is no pre-authored context, no seeded
graph data, no fictional incident history.

Agents investigate from first principles:
1. **triage-agent** reads real code, real metrics, classifies severity
2. **diagnostic-agent** analyzes config, identifies root cause, detects
   uncertainty about historical patterns
3. **rehydration-agent** asks the kernel: "has this happened before?"
   → **The kernel returns nothing** (no history yet)
4. Agents resolve the incident from scratch

As agents discover findings, they **materialize them via UpdateContext**:
- Root cause analysis → becomes an `agent_finding` node
- Config analysis → becomes a `config` node with real values
- Fix decision → becomes a `decision` node with rationale
- Commit SHA, PR URL → become `fix_artifact` nodes
- Recovery metrics → become `resolution` node with before/after values

The kernel builds its graph **FROM the investigation**, not from
pre-authored YAML.

### Subsequent incidents

When a similar alert fires again:
1. **rehydration-agent** asks: "has this happened before?"
2. The kernel queries by alert pattern (alertname, service) and finds the
   previous incident subgraph
3. `RehydrateSession` returns a **real** causal bundle:
   - "Last time pool saturation spiked on payments-api, bumping
     `max_open_conns` alone failed. The fix required BOTH increasing
     the pool budget AND switching to `adaptive-recovery` mode."
   - Commit SHA of the previous fix, PR URL, metric evidence
4. Agents resolve faster and more confidently because they have **real**
   historical context

### Compound value

Each resolved incident enriches the graph. The kernel accumulates
institutional knowledge the way a senior engineer accumulates experience:

- **1st incident**: Slow. Agents investigate from first principles. ~15 min.
- **5th incident**: Faster. Kernel has 4 previous investigations. Common
  patterns are identified. ~5 min.
- **20th incident**: Near-instant triage. The kernel has seen this pattern
  before and knows exactly what worked. ~1 min.

**Switching away from Underpass means losing this accumulated knowledge.**
This is the moat.

---

## Event Ownership Model

### Integration events (client infrastructure)

These events are produced by the **client's** systems. Underpass defines the
AsyncAPI contract; the client implements the adapter.

| Event | Producer | Notes |
|-------|----------|-------|
| `observability.alert.firing` | Client's alert-relay | Must conform to AsyncAPI spec |
| `observability.alert.resolved` | Client's alert-relay | Sent when alert condition clears |
| `payments.fix.deployed` | Client's CI/CD pipeline | Triggered by merge → build → deploy |

The kernel **consumes** these as trigger events and resolution confirmations.

### Internal events (Underpass-controlled)

These events are produced by Underpass agents and enriched with investigation
findings. The kernel can consume them to build its graph in real-time.

| Event | Kernel action |
|-------|---------------|
| `payments.incident.triaged` | Materialize severity, diagnostic target |
| `payments.context.rehydration_requested` | Trigger rehydration bundle delivery |
| `payments.context.rehydrated` | Record that context was delivered |
| `payments.fix.plan_proposed` | Materialize root cause, proposed changes |
| `payments.verification.passed` | Materialize test results, confidence |
| `payments.fix.branch_pushed` | Materialize commit SHA, PR URL |
| `payments.recovery.confirmed` | Materialize before/after metrics |

Each internal event carries the producing agent's findings as structured
payload — the kernel doesn't need to re-derive them.

---

## Agent Purpose in Rehydration Bundles

Each agent has a defined **purpose** (autonomy boundary + success criteria).
When the kernel delivers a bundle via `RehydrateSession`, it should include
the agent's purpose as a structured preamble.

### Current state

`RehydrateSession` accepts roles and delivers role-scoped bundles. The
role determines which nodes/relationships are included and what resolution
level is used.

### Target state

The bundle preamble includes the agent's purpose:

```
[AGENT PURPOSE — repair-agent]
Role: repair
Autonomy boundary:
  - Patch code or config
  - Select governed runtime tools
  - Produce execution evidence
  - Create hotfix branch and PR after verification passes
You do NOT: classify severity, merge PRs, generate RCA.
Success criteria:
  - Patch prepared
  - Runtime execution requested with evidence

[HISTORICAL CONTEXT]
Last time this alert fired (2026-03-15):
  Root cause: effective_capacity=10, needed 28 for current load
  Fix: max_open_conns 12→30, recovery_mode strict→adaptive-recovery
  Commit: abc123, PR: #47, deployed 2026-03-15T14:30Z
  Recovery confirmed: saturation dropped from 7.4 to 1.8
```

This ensures agents know what they're supposed to do and what they should
NOT do — grounded by real historical context.

---

## Task Planning

### Already implemented

- [x] `GetContext` / `RehydrateSession` with bounded retrieval
- [x] Multi-resolution rendering with token budgets
- [x] Explanatory relationships (6 semantic classes)
- [x] Scope validation per agent role
- [x] `UpdateContext` RPC with optimistic concurrency
- [x] CQRS projection pipeline (NATS → Neo4j + Valkey)
- [x] Salience ordering (causal > motivational > evidential > ...)

### Phase C: Kernel context (Priority 1)

| Issue | Task | Complexity | Depends on |
|-------|------|------------|------------|
| [#70](https://github.com/underpass-ai/rehydration-kernel/issues/70) | Wire UpdateContext for real-time agent findings | Medium | — |
| [#71](https://github.com/underpass-ai/rehydration-kernel/issues/71) | Temporal queries: find past incidents by alert pattern | Medium | #70 |

**#70 — UpdateContext for agent findings**

Define new node kinds for agent-produced context:
- `agent_finding` — investigation results (root cause, config analysis)
- `fix_artifact` — git commits, PR URLs, CI results
- `resolution` — confirmed recovery with metric evidence

Define new relationship types:
- `DISCOVERED_BY` — finding → agent
- `FIXED_BY` — incident → fix_artifact
- `CONFIRMED_BY` — resolution → metric evidence

Wire the demo's `DispatchDomainEventUseCase` to call `UpdateContext` after
each agent produces findings. The UpdateContext RPC already supports
CREATE/UPDATE/UPSERT operations with idempotency keys.

**#71 — Temporal queries by alert pattern**

Add filtering to `GetContext` or `RehydrateSession`:
- Match nodes by `labels` (alertname, service) not just by node ID
- Query: "find all resolved incidents where service=payments-api"
- Return most relevant match with full causal context
- Handle zero results gracefully (first incident → empty response)

Implementation approach: add a `labels` filter to the Neo4j query in the
GetContext handler. The graph already stores labels as node properties.

### Phase D: Agent purpose (Priority 2)

| Issue | Task | Complexity | Depends on |
|-------|------|------------|------------|
| [#72](https://github.com/underpass-ai/rehydration-kernel/issues/72) | Agent purpose in rehydration bundles | Low | — |

Accept agent purpose metadata in `RehydrateSession` request (new field in
the proto). Include purpose as a structured preamble in the rendered bundle
before the historical context.

### Phase F: Full loop (Priority 3)

| Issue | Task | Complexity | Depends on |
|-------|------|------------|------------|
| [#73](https://github.com/underpass-ai/rehydration-kernel/issues/73) | Materialize resolved incident as historical context | Medium | #70 |

After an incident is fully resolved, batch-materialize the entire resolution
subgraph via `UpdateContext`. Schema:

```
incident (root)
  ├── TRIGGERED_BY → alert_event (labels, metrics)
  ├── INVESTIGATED_BY → agent_finding[] (triage, diagnostic)
  ├── CONTEXT_FROM → rehydration_bundle (if historical context was used)
  ├── FIXED_BY → fix_artifact (commit, PR, CI result)
  ├── VERIFIED_BY → verification (test results, metric check)
  └── CONFIRMED_BY → resolution (before/after metrics, timestamp)
```

---

## How Context Becomes Legitimate

### The principle

**The kernel never contains pre-authored fiction.**

| Approach | Source | Legitimate? |
|----------|--------|-------------|
| Pre-authored graph.yaml | Human wrote it | No |
| graph.yaml with real file paths | Human wrote it better | No |
| Agent investigates real code, materializes findings | Agent discovered it | **Yes** |
| Previous agent resolution stored as history | System learned from experience | **Yes** |

### The graph data in demos/

The `demos/payments-sev1/graph/` directory contains pre-authored graph data.
This is used for the **scripted replay** demo mode (bootstrapMode=replay)
where events play in sequence for a TUI demonstration.

For the **alert-driven full-loop** demo (bootstrapMode=alert), the kernel
starts empty. Agents build the context from real investigation. The graph
data in `demos/` is NOT used.

### The test

Context is legitimate if and only if:
1. An agent observed it in a real system (code, metrics, git history)
2. OR an agent produced it as a finding during a real investigation
3. OR it was materialized from a real resolution outcome

If a human wrote it, it's not legitimate — even if the facts are accurate.
