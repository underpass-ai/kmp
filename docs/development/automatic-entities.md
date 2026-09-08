# Optional cited entity resolution

Experimental component in `scripts/kmp_entities`, outside the deterministic
kernel. It proposes and verifies identity between bounded mentions, then produces
a canonical ingest over a frozen source/formed payload. It does not run on
ordinary writes or change a personal store. Retrieval utility must be measured
separately on each evaluation corpus.
Version `entities-v2-distinct-referents` explicitly separates identity from
ownership, employment and account use. A transferred account does not make its
users the same person. Those relationships need their own representation.

The motivating evaluation case has a stored Elena Vega/Nora equivalence that
neither source-only nor V7 top-five retrieval admits for a full-name question.
Connecting entire statements with `same_entity_as` would assert that different
facts are identical. This component instead creates literal mention nodes and
declares equivalence only between those nodes, with source proof and clocks.

## Run locally

```bash
PYTHONPATH=scripts python3 -m kmp_entities \
  --context entity-context.json --base source-or-formed-ingest.json \
  --output artifacts/entity-candidate --model nemotron-local \
  --model-revision <pinned-local-weights-revision>
```

`context` admits exactly `about`, `context_id`, `as_of` and `sources`.
Source records have the formation fields `id`, `role`, `text`, `observed_at`.
Queries, labels, generated memories and later sources must not enter this file.
The hard boundary rejects unknown fields, repeated source IDs and any source
after the explicit cutoff. Its limit is 128 sources and 24,000 source characters;
overflow is refused. A caller must explicitly plan larger histories.

The base ingest must contain exactly those original sources with unchanged
text, role and report clocks. Existing formed memories are preserved. A model
first proposes up to 32 mention-to-anchor identities, then judges every admitted
proposal against the complete source context. At most two generation calls occur;
an empty or wholly invalid extraction needs no verifier call. No retry or repair
follows a failed verdict. The existing loopback-only client records requests,
responses and generation settings; a budgeted external adapter can implement
the same `generate(messages, schema, phase)` interface explicitly.

Each endpoint identifies a literal named expression, source ID and zero-based
occurrence. The compiler resolves Unicode character offsets itself. Quotes must
cover both exact occurrences. Shared spelling is not an admissible identity
basis by itself. The model must preserve homonyms, ambiguous references,
hypothetical/document scope and negation. These instructions require evaluation:
literal admission and same-model verification cannot guarantee identity.

## Representation and retrieval boundary

- Source and formed records remain byte-for-byte in the payload prefix.
- Mention nodes contain their literal expression and source offsets, roles,
  hashes and model provenance. They stand in an `entity_mention` dimension.
- `derived_from` links a mention to its source. `same_entity_as` links mentions
  to mentions, with `why`, quoted evidence and medium confidence.
- An identity relation's occurred coordinate is the latest cited source report,
  while its observed coordinate is resolution time. This records availability
  of the proof; it does not infer when the real-world identity began.
- Literal proof items support both endpoints and name original source refs.
  The saved plan lists the exact proof IDs and their availability per binding.
- There is no transitive closure, cross-about resolution, source rewrite,
  supersession or consolidation. A false verdict remains a rejected proposal.

Primary claim retrieval can select entries with a `conversation` label and
inspect the separate mention graph from returned source refs. An integration
must fetch the actual graph and quotes, respect the query clock and a fixed
inspection budget, and report omissions. It must not fill missing proof from a
local registry or assume that the presence of an identity edge ensures top-k
admission.

`kmp_entities.expand.expand` implements one identity hop from original source
refs already admitted by primary retrieval. It fetches actual source-to-mention
links, both bounded mentions, their identity link and every proof quote. The
frozen plan validates those responses but never fills missing text or edges.
All inspections, source validation and pagination share a fixed call budget.
Incomplete or stalled paths remain rejected; their partial proof is not admitted.
The returned trace reports calls and rejected paths.

This integration explicitly uses the occurred/report clock: source coordinates,
mention links, identity relation and proof timestamps must fit the cutoff.
`kmp_inspect` does not apply that cutoff itself. Other axes and online resolution
as known at an earlier ingestion time are not supported by this helper. It has
contract tests and an actual-MCP synthetic check; these checks do not measure
automatic identity quality or end-to-end answer improvement.

```bash
PYTHONPATH=scripts PYTHONDONTWRITEBYTECODE=1 \
  python3 -m unittest discover -s scripts/kmp_entities -p 'test_*.py' -v
```

Tests cover source isolation, literal occurrences, missing/negative verdicts,
duplicate pairs, relation clocks and preservation of the base. They are contract
tests, not an entity-resolution score. Test aliases are independent of the model
and must not be counted as successful automatic resolution.
Expansion tests also remove proof, change returned objects, cross the cutoff and
exhaust or stall pagination to ensure that the local plan cannot fill a gap.
