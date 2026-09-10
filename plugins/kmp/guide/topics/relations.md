## Why the `why` matters

An entry records **what is true**. A relation records **how two entries are
connected**. Its `why` records **why that specific connection holds and what
a later reader should understand by traversing it**. Its `evidence` records
**the concrete observation or source that supports that explanation**.
`confidence` says how certain the writer is; it is not a relevance score.

### Read the endpoints before choosing the relation

In a writer packet, the memory containing `connect_to` is the source; `ref`
names the target. Read it as **source -> relation -> target**, not as a loose
association. Preview and accepted responses expose these triples with local
`@id` endpoints so you can check the compiled direction without a full receipt.

| Source -> relation -> target | Concrete example | Misleading use |
| --- | --- | --- |
| approval -> `authorizes` -> permitted action | Approval A permits planned deploy D: `@A -> authorizes -> @D`. | D authorizes A reverses permission. A does not prove D ran. |
| claim/outcome -> `verified_by` -> actual check | Recovery R verified by successful health check H: `@R -> verified_by -> @H`. | An approval or earlier failure report does not verify recovery. |
| excluded value/exclusion record -> `excluded_from` -> total/set | Void V is excluded from settlement T: `@V -> excluded_from -> @T`. | T points to V backwards; an original invoice is not the total. |
| corrected fact -> `corrects` -> earlier fact | Corrected invoice amount C changes amount I: `@C -> corrects -> @I`. | A correction to one amount does not replace every fact in its source report. |
| replacement -> `supersedes` -> entire old memory | Policy P2 replaces all of P1: `@P2 -> supersedes -> @P1`. | A partial change must not mark a multi-fact report wholly SUPERSEDED. |

Suppose report G says “invoice J1 is 18 USD; J2 is cancelled; the fee is 6 USD”.
Later C corrects **only J1** to 12 USD. Prefer separate memories for J1, J2 and
the fee, then `@C -> corrects -> @J1`. If G already exists as one memory,
`@C -> corrects -> G` can name the exact J1 correction in `why` and `evidence`
while preserving the unrelated facts. `corrects` does not change G's lifecycle
status; readers must still consider the correction. `@C -> supersedes -> G`
would mark **all G** SUPERSEDED and is too broad. Do not overwrite G to hide it.

For a fully replaced atomic fact, `supersedes` is appropriate when the source
proves whole-target replacement. For a state transition use the source-supported
state relation; an unknown onset stays unknown. Neither `observed_at` nor the
mere order of reports establishes `valid_from` or `valid_until`.

KMP validates endpoints, relation vocabulary and declared proof fields. It does
not infer whether their story is true. `rich`, `accepted` and these visible
triples are not a semantic audit. Compare both endpoints with the evidence.

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

- omit `memories[].ref` for new memory. Supply it
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
