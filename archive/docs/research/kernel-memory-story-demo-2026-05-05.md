# Kernel Memory Story Demo

Date: 2026-05-05
Run id: `20260504T233722Z`
Status: live MCP run against the deployed Kubernetes kernel

## Runtime

- Consumer: Codex using the `rehydration_kernel` MCP tools directly.
- MCP backend: live gRPC `KernelMemoryService`.
- Kernel release: Helm `rehydration-kernel` revision `110`.
- Kernel image: `ghcr.io/underpass-ai/rehydration-kernel:dev-93048c1`.
- Runner image deployed for e2e: `ghcr.io/underpass-ai/rehydration-kernel/e2e-runner:dev-7b02848`.
- Transport evidence: kernel logs show `KernelMemoryService.*` request and response records for this run.

## Scenario

The demo models a checkout latency incident as durable memory instead of a
prompt transcript. It uses three independent `about` anchors that share the
same local dimension ids but are namespaced by the kernel:

- `article:incident:checkout-latency:20260504T233722Z:war-room`
- `article:incident:checkout-latency:20260504T233722Z:payments`
- `article:incident:checkout-latency:20260504T233722Z:frontend`

Dimensions:

- `timeline`
- `decision`
- `evidence`

Ingested memory:

| About | Entries | Relations | Evidence |
| --- | ---: | ---: | ---: |
| war-room | 4 | 3 | 2 |
| payments | 2 | 1 | 1 |
| frontend | 1 | 0 | 1 |
| total | 7 | 4 | 4 |

## Tool Coverage

| Tool | Result |
| --- | --- |
| `kernel_ingest` | accepted all 7 entries, 4 relations, 4 evidence items, read-after-write ready |
| `kernel_wake` | returned a wake packet with current state and causal spine from the war-room timeline |
| `kernel_ask` | returned deterministic evidence-backed context, not a generative answer |
| `kernel_rewind` | returned scoped temporal history for `decision`, `current_about`, `abouts`, and `all_abouts` |
| `kernel_near` | returned the temporal neighborhood around the mitigation decision |
| `kernel_goto` | jumped to sequence `3` in the war-room timeline |
| `kernel_forward` | moved forward from the initial alert to diagnosis, mitigation, and verification |
| `kernel_trace` | returned the causal edge from hypothesis to mitigation and the follow-on recovery edge |
| `kernel_inspect` | returned incoming/outgoing links and evidence for the mitigation node |

## Key Results

| Query | Scope | Entries | Notes |
| --- | --- | ---: | --- |
| rewind decisions | `current_about` | 2 | only diagnosis and mitigation decision entries |
| rewind timeline | `current_about` | 4 | isolated war-room timeline |
| rewind timeline | `abouts` war-room + payments | 6 | includes payments context, excludes frontend distractor |
| rewind timeline | `all_abouts` | 7 | includes war-room, payments, and frontend |
| near mitigation | `current_about` | 3 | hypothesis, mitigation, verification |
| goto sequence 3 | `current_about` | 3 | alert, hypothesis, mitigation |
| forward from alert | `current_about` | 3 | hypothesis, mitigation, verification |
| trace hypothesis to mitigation | global graph path | 2 | causal mitigation edge plus recovery continuation |
| inspect mitigation | node detail | 4 incoming, 1 outgoing, 1 evidence | audit-ready local explanation |

## Validation Checks

| Check | Result |
| --- | --- |
| `scope=abouts` without `abouts` list | rejected: `dimension scope ABOUTS requires at least one about` |
| temporal `raw_refs=true` | returns typed raw audit refs for selected entries in the current cut |

## Log Evidence

Relevant kernel log facts from the run:

- `KernelMemoryService.Ingest` accepted war-room `entries=4`, `relations=3`, `evidence=2`.
- `KernelMemoryService.Ingest` accepted payments `entries=2`, `relations=1`, `evidence=1`.
- `KernelMemoryService.Ingest` accepted frontend `entries=1`, `relations=0`, `evidence=1`.
- `KernelMemoryService.Rewind` with `dimension_scope="abouts"` logged `selected_abouts` as payments + war-room and `entries=6`.
- `KernelMemoryService.Rewind` with `dimension_scope="all_abouts"` logged `selected_abouts` as war-room + frontend + payments and `entries=7`.
- `KernelMemoryService.Goto` now supports temporal `raw_refs=true` as typed raw audit refs; older runs before this cut logged an `InvalidArgument` fail-fast error.

## Article Angle

This run supports a pragmatic article claim:

> The kernel lets an agent resume, query, traverse, and audit operational memory
> through typed deterministic APIs, while preserving temporal and dimensional
> boundaries across multiple sessions.

Good post structure:

1. Problem: long-running agent work loses context when memory is a prompt buffer.
2. Model: memory is stored as about-scoped entries, dimensions, relations, and evidence.
3. Demo: checkout incident across war-room, payments, and frontend sessions.
4. Results: scoped temporal traversal and audit paths.
5. Engineering takeaway: fail-fast typed APIs beat hidden fallbacks for agent memory.

## Caveats

- This demo intentionally avoids LLM judging. It proves deterministic memory
  retrieval and traversal, not task-quality uplift from an LLM.
- `kernel_ask` is deterministic evidence retrieval. It should not be described
  as free-form generation.
- Temporal `raw_refs=true` and inspect `raw=true` are typed audit expansions,
  not semantic answer paths.
