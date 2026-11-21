Compare explicitly selected abouts with `kmp_relate`. For example:

```json
{"about":"project:sample","dimensions":{"scope":"abouts","abouts":["project:sample","project:other"]}}
```

Use actual about ids from the task or prior results. Expect the facts of the
selected abouts with the state they had at the interval's end; their declared
relations, cross-about equivalences included; coordinate relations between
facts of different abouts inside a scope both use (before, after, during,
concurrent, same sequence or rank, shared scope); tensions, standing facts a
declared `contradicts` joins; and proposed links, pairs from different abouts
sharing a rare identifier, a matching English summary or a proper name, each
with its signals and weight, never stored. Nothing is ranked or generated, and
each item says why in one checkable sentence. Pages run over facts, declared,
coordinate, tensions and proposed, in that order. Co-occurrence and
similar names are not identity. A returned identity proposal is not yet a link.

Declare a cross-about equivalence only through the semantic writer, with the
Relate proposal, inspected refs and source-backed why/evidence. An account can
change hands; that does not make its holders the same person.

Preserve the chosen clock, interval and dimensions; complete pending pages.
The default current-about scope does not compare every about. `all_abouts`
is a real scope expansion and cost, not a fallback for every empty response.

More: `guide:kmp-agent:example:distributed-incident`.
