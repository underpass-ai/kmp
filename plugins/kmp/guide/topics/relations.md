## Why the `why` matters

An entry records **what is true**. A relation records **how two entries are
connected**. Its `why` records **why that specific connection holds and what
a later reader should understand by traversing it**. Its `evidence` records
**the concrete observation or source that supports that explanation**.
`confidence` says how certain the writer is; it is not a relevance score.

Those fields are deliberately separate:

| Field | Durable meaning | Good test |
| --- | --- | --- |
| entry | What is true | Can it stand alone a year later? |
| `rel` + `class` | How the endpoints connect | Is this the most specific supported relation? |
| `why` | Why this connection holds | Does it explain both endpoints, not merely repeat one? |
| `evidence` | What proves that rationale | Is it an observed fact or named source rather than an interpretation? |
| `confidence` | Certainty in the relation | Does it reflect the strength of the proof? |

KMP preserves and uses this context; it does not invent it. During recall,
direct evidence determines whether a candidate is eligible. A typed,
evidence-backed relation and its `why` can then improve the ordering and
explain the match, but relation prose cannot promote unrelated evidence into
an answer. The selected relation types are exposed in
`proof.matched_relations`, and the original rationale remains auditable in
`proof.path`, `kmp_trace`, and `kmp_inspect`.

That gives the agent a complete read path:

1. `kmp_wake` recovers the durable state and its causal or motivational
   spine.
2. `kmp_ask` answers a paraphrased question from direct evidence and uses
   the graph context to keep the right citation in the core.
3. `kmp_trace` proves the path between two refs; `kmp_inspect` shows the
   stored object, links, and evidence verbatim. Inspect uses the owning
   `about`; trace starts inside its declared about and can cross a proven
   equivalence. Neither call silently opens an arbitrary all-abouts scope.

### Write the rationale and its proof as a pair

For a decision selected from an observation:

```json
{
  "ref": "incident:login:observation:refresh-race",
  "rel": "chosen_because",
  "class": "motivational",
  "why": "Retry was chosen because it addresses the refresh race without weakening the timeout policy.",
  "evidence": "Auth logs show refresh success followed by a 401 on the next request.",
  "confidence": "high"
}
```

For an outcome governed by a constraint:

```json
{
  "ref": "project:kmp:constraint:shared-store",
  "rel": "satisfies_constraint",
  "class": "constraint",
  "why": "SQLite WAL satisfies the requirement that independent agent processes share one embedded store.",
  "evidence": "The two-process integration test completed concurrent reads and writes without a single-writer lock failure.",
  "confidence": "high"
}
```

For a lifecycle change:

```json
{
  "ref": "project:kmp:decision:single-writer-store",
  "rel": "supersedes",
  "class": "evidential",
  "why": "SQLite WAL replaces the single-writer layout because the architecture now requires multi-process access to one store.",
  "evidence": "The shared-store test failed at the previous process lock and passed under SQLite WAL.",
  "confidence": "high"
}
```

In all three, swapping `why` and `evidence` would lose information. The
rationale interprets the edge; the evidence anchors that interpretation in
something observed.

Before committing a rich relation:

- omit `current.ref` and `semantic_delta.ref` for new memory. Supply either
  only to update an existing entry inside the same `about`; it must begin with
  that exact about plus `:`, so never reuse a foreign ref surfaced by a
  cross-about read;
- inspect or traverse every existing target and name it in `read_context`;
- choose the most specific relation supported by what you read;
- make `why` mention the actual relationship between the two endpoints;
- make `evidence` concrete and independent enough to audit;
- call `kmp_write_memory` once with `options.dry_run=false` (or omit that
  option) and a stable `idempotency_key`. The planner validates the complete
  write before ingest, so an invalid request writes nothing.

Set `options.dry_run=true` only when the user explicitly asks for a preview,
when debugging the compiled ingest payload, or when a deliberate human review
must happen before mutation. A preview writes nothing and does not imply a
follow-up commit.

A relation is **rich** or **anemic**. Rich relations — causal, motivational,
evidential, constraint — require both `why` and `evidence`. If the context
does not justify one, use a narrow fallback and say only what is known:

| What the evidence supports | Honest fallback |
| --- | --- |
| Only temporal sequence | `follows` / procedural |
| A response to a prior turn | `answers` / evidential |
| Background was consulted | `uses_background` / evidential |

Declaring that two memories say one thing — `restates`, `same_event_as` or
`same_entity_as`, with `why` and `evidence` — is what lets `kmp_ask` cite the
second beside the first when a question matches only one of them. It is the
one route to a retrievable paraphrase that needs no table and no model, and
it exists only because somebody wrote it down. Write it when you know it.

Never use a vague relation like `related_to`, invent a causal link, or dress
one of these fallbacks in motivational or constraint language. A weak honest
edge is safer than a rich false one.

**The vocabulary is self-documenting.** `tools/list` carries a catalog
generated from the kernel's own writer spec on `connect_to.rel` and the
ingest `rel` field: every relation type with its quality tier, its allowed
semantic classes, and when to use it. Read the schema in front of you rather
than guessing from these examples — the schema is the authority, and it moves
with the kernel.
