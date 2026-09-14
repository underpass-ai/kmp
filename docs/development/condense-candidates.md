# Condense candidates

Issue #784 adds deterministic card targets to compact Trace proof reads. The
domain uses the selected object descriptors, card presentations and admitted
support relations from the operation snapshot. It loads no extra canonical body.
Destination mode counts selected routes; seek counts structurally complete
solver groups with known relation clocks and all binding witnesses, independently
of canonical body delivery. Groups retained for review do not add sharing.
A shared source counts
once per route/group even when it supports several of that group's entries.

`proof.condense_candidates.items` contains ref, body/record bytes, card status,
sharing count and exact `source`/`expect` objects for Condense. Absent and stale
cards with bodies of at least 1024 UTF-8 bytes are eligible. Targets sort by
sharing descending, size descending and ref; eight survive. `omitted_count`
reports the eligible tail. `valid`, `after_cut` and `below_floor` are disjoint
exclusions, with card status taking precedence over size. Missing bodies are
outside this inventory. The floor is a heuristic, not a measured break-even.

The list is present on every page, including empty lists and pages with none of
its objects. It is part of the whole-selection fingerprint, not the canonical
manifest identity. Existing stale-cursor detection covers card/body changes.
Noncompact reads and refused expansions do not carry the list. No writer field,
tool or storage format changes. Individual inspection and expansion remain usable.

The reader expands a chosen body under the returned manifest, copies `source`
and `expect`, and writes its own card. Source revision/digest and card revision
are independently checked by the existing writer. Recommendations contain no
prose, never close a proof group and are not required continuation actions.
New cards cannot be backdated into a past historical cutoff.

## Native replay, 2026-09-14

The replay starts with a compact descriptor read, completes all its pages,
expands the same shared source, writes one fixed illustrative card, then
completes a fresh compact read. Both variants use identical originals, page
size 2 and a 200000-byte response ceiling. The baseline script scans all objects
to locate the same source; it does not simulate agent choice or transcription
errors. The small negative control deliberately cards a source below the floor
to compare the same work, although the new list would not recommend it.

Baseline: main `4b97b15f79b450b8e3f589eb68d6fc8a20bbaab6` (v0.18.4). Local debug
builds, default workspace profiles, native stdio, fresh isolated SQLite stores.
Bytes include complete JSON-RPC requests and responses, excluding line endings.
Setup, seed and tools/list are recorded separately. This is a deterministic
contract/cost replay, with no model, latency, token or comprehension claim.

| Fixture | Calls before / after | Journey bytes before | After | Change |
| --- | ---: | ---: | ---: | ---: |
| Large bodies | 21 / 21 | 260579 | 280645 | +7.70% |
| Small bodies | 21 / 21 | 101016 | 102876 | +1.84% |

The large list value costs 1072 bytes on its initial pages, with 19606 bytes
of list values over the complete journey. Empty list values cost 70 bytes each
(1400 over the small journey). Field names/separators add a further 460 bytes
per journey. Native tools/list grows from 244994 to 247172 response bytes
(+2178); initialize remains 1805 bytes. Body hashes match between variants.
The new field increases this scripted journey's cost; it provides targeting and
copyable identity, not a demonstrated reduction in calls or context.

Raw phase counts and binary/source hashes are in the local generated
`artifacts/condense-candidates-784/replay-main.json` (not tracked in Git).
Reproduce with binaries built from the baseline and candidate, keeping scratch
inside the workspace:

```bash
python3 scripts/performance/condense-candidates.py \
  --baseline /absolute/path/to/baseline-kmp-mcp \
  --candidate /absolute/path/to/candidate-kmp-mcp \
  --scratch tmp/condense-candidates \
  --output artifacts/condense-candidates-784/replay-main.json
```

Behavioral controls cover ordering, tie/cap/floor boundaries, source sharing,
zero additional body reads, both Trace modes, real SQLite source mutation,
stale-card CAS, language/cut exclusions, page invariance and real gRPC byte
parity. The installed agent and human guides and tool fixtures are regenerated
from the same candidate. Native controls do not establish agent understanding.
