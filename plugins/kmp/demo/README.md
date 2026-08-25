# Demo memory

`checkout-latency.jsonl` is a KMP bundle: one header line and eight events of
an incident, in sequence order. `/kmp:demo` imports it into a data directory
of its own so you can see a populated memory — graph, evidence, timeline,
traces — before writing any of your own.

It is deliberately an incident with a **wrong turn in it**. Latency triples
after a deploy, the pool looks saturated, the obvious change is rolled back,
and the rollback does not help. Only then does the real cause appear: a client
timeout cut in the same release train, turning every slow request into six.

That shape is the point. "Which attempt introduced this assumption" and "what
did we believe at 15:05, before we knew better" are questions memory can
answer and a transcript cannot. A demo where everything goes right would show
none of it.

## What it exercises

| Move | What it finds here |
| --- | --- |
| `kmp_wake` | the incident's state, its decisions and what closed it |
| `kmp_ask` | "why did the rollback not fix the latency" — answered from evidence |
| `kmp_trace` | six-hop path from the first symptom to the constraint that ended it |
| `kmp_rewind` / `kmp_goto` | the store at 15:05, when the rollback still looked right |
| `kmp_inspect` | any node with its links and the evidence behind them |

The graph carries every rich relation class: `supports` (evidential),
`chosen_because` (causal), `contradicts` (evidential — the rollback being
disproved), `derived_from`, `verified_by` and `satisfies_constraint`.

## Regenerating it

The bundle is exported from a real store rather than hand-written, so it is
always a bundle the current binary can read. `tests/plugin/demo_bundle.rs`
imports it on every run and asserts the shape, so a bundle that stops loading
fails the build rather than the demo.
