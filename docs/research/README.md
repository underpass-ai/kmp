# Research Summary

Last reviewed: 2026-08-25

KMP's exploratory research phase produced benchmark adapters, evaluation
harnesses, demos, paper drafts, incident reports, and several product decisions.
Those working documents are now preserved under
[`archive/docs/research/`](../../archive/docs/research/). This page records the
current conclusions without presenting the old research backlog as a product
commitment.

## What was completed

| Area | Result |
| --- | --- |
| Product direction | KMP was validated as temporal, multidimensional and auditable process memory. The kernel owns evidence, relations, traversal, proof and known-at-time state; domain operations such as sums, dates, money and deduplication belong in plugins. |
| Benchmark strategy | MemoryArena and MemoryAgentBench were selected as the best agentic-memory tracks. LongMemEval remains a secondary conversational-memory regression rather than the definition of the product. |
| MemoryArena | KMP gained an adapter, stage-aware live runner, replay artifacts, known-at-time checks, MCP navigation probes, a smart-writer harness and a paper-aligned local scorecard. The work proved evidence fidelity and exposed the remaining reader/agent gap. |
| MemoryAgentBench | KMP gained a feasibility adapter and live runner for inject-once/query-many memory, initially focused on conflict resolution and stale facts. |
| LongMemEval | KMP gained artifact generation, live and embedded runners, evidence and embedding probes, an external reader and an official-evaluator path. The embedded 500-item parity run recovered all 470 answer-bearing evidence sets and all 30 abstention items. |
| Relation quality | The experiments showed that typed, evidence-backed relations improve explanation and audit paths, while relations alone cannot replace domain-specific derivation. On the recorded 121-item oracle slice, a rich graph with the standard reader scored 105/121 versus 101/121 for the flat graph. |
| Demonstrations and operations | The work produced graph exploration, incident replay, benchmark diagnostics, judge calibration, deterministic artifacts and post-mortems for failed or misleading runs. |

These are research results, not guarantees for every workload. The detailed
methodology, caveats, raw measurements and historical paths remain in the
[research archive](../../archive/docs/research/README.md).

## Is more research planned?

Yes. The next phase will investigate what can be learned from KMP's temporal
and multidimensional memory model, rather than extending the old paper backlog.
Its explicit objective is to keep KMP at the state of the art of agentic
context: continuously review new techniques and benchmarks, test them against
real agent workflows, and adopt only the ideas that improve useful,
inspectable, and evidence-backed memory.

The active research questions are:

1. **Temporal and multidimensional analysis** — determine how dimensions,
   episodes, relations, and known-at-time state can reveal patterns that are
   lost in a flat transcript or similarity search.
2. **Decisions and outcomes** — trace decisions that may have contributed to
   errors, recovery, or successful outcomes, and identify the evidence and
   intermediate steps that make each path auditable.
3. **Recurring conversations** — detect repeated questions, themes, incidents,
   unresolved loops, and decisions that recur across sessions, then study how
   to prioritize them by frequency, recency, impact, and evidence quality.
4. **Memory quality** — find contradictions, isolated decisions, missing
   rationale, weak evidence, and repeated failed paths that should be surfaced
   to an agent or human reviewer.
5. **Useful delivery** — determine which analyses belong in deterministic KMP
   retrieval, which need a plugin or skill, and which require a model above the
   kernel. New core verbs require evidence that the existing moves cannot
   express the need.
6. **State-of-the-art tracking** — follow new agentic-context architectures,
   memory benchmarks, retrieval and reasoning methods, then compare them with
   KMP on task success, temporal correctness, auditability, latency, and cost.

The analysis must not present correlation as causation. A decision is only
labelled causal when stored relations and evidence support that conclusion;
otherwise the output remains a ranked hypothesis with a traceable proof path.
Evaluation will therefore measure false positives, missed evidence, temporal
leakage, prioritization quality, and whether a reviewer can audit the result.

The existing MemoryArena, MemoryAgentBench, and LongMemEval adapters remain
useful regression and evaluation infrastructure for this work. The paper is
paused, not abandoned: its sources remain under
[`docs/research/paper/`](paper/README.md), and we intend to recover the
publication effort when the new roadmap and research evidence justify a
maintained paper. External comparisons, human studies, and that future paper
are not release promises. Release commitments remain in the current code,
tests, release process and
[documentation catalog](../documentation-catalog.md), not in research notes.

## Archive map

- [Research index and all original notes](../../archive/docs/research/README.md)
- [Benchmark and experiment artifacts](../../archive/artifacts/)
- [Historical Kubernetes manifests](../../archive/k8s/)
- [Current testing and quality gates](../testing.md)
- [Operator model research](operator.md)
- [Experimental graph-aware reranker](../development/graph-aware-answer-reranker.md)
- [Paused paper package, retained for later recovery](paper/README.md)
- [ACL paper sources](paper/acl/README.md)
