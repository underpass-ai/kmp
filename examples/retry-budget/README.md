# Retry budget

One fictional checkout service and the two months in which its retry policy
changed. This is the memory `kmp-mcp demo` writes under the about
`example:kmp-demo`; the packet lives at
[`crates/kmp-mcp/fixtures/demo/kmp-demo.requests.json`](../../crates/kmp-mcp/fixtures/demo/kmp-demo.requests.json)
so the binary ships it, and [`memory.jsonl`](memory.jsonl) is its export.

## What is inside

| When | Memory | Kind |
|:--|:--|:--|
| 2026-07-15 | Checkout retries provider calls until they succeed. | decision |
| 2026-08-04 | Checkout must answer within 800 ms at p99; 300 ms are checkout-api's own. | constraint |
| 2026-08-20 | Every payment call carries an idempotency key. | constraint |
| 2026-09-02 | INC-2041: a provider slowdown became a retry storm. | observation |
| 2026-09-03 | Retries are capped at two with jittered backoff. | decision |
| 2026-09-10 | p99 fell from 1,400 ms to 620 ms. | success path |
| 2026-09-20 | A circuit breaker opens after five failures; the cap stays. | decision |

The relations are the point: the cap was `chosen_because` of the incident, it
`supersedes` retry-until-success, it `satisfies_constraint` the latency
budget, it `depends_on` the idempotency key, and it was `verified_by` the
dashboard a week later. Each carries its `why` and a quote from the source.

## Ask

- "Why are retries to the payment provider capped at two?" The agent asks
  memory and cites ADR-014 and INC-2041.
- "What did we believe about retries in August 2026?" The same question asked
  standing in August returns the earlier decision, not the cap.
- "What changed about retries in September 2026?" Temporal navigation lists
  the incident, the cap and the measurement in order.
- "Show me the memory behind the retry cap decision." ChronoLoom opens on the
  decision and lights up its proof path.

## Load

```bash
kmp-mcp demo                                   # writes it where this directory's memory lives
KMP_MCP_DATA_DIR=/tmp/kmp-demo kmp-mcp import examples/retry-budget/memory.jsonl
```

The demo about is machine-local, like the installed guides: a project's
committed `.kmp/memory.jsonl` never carries it.
