# Examples

Real memories you can load, ask about and open in ChronoLoom. Each example is
one directory with the packets that wrote it, the exported bundle, and a
README that says what is inside and what to ask.

| Example | What it holds | Try |
|:--|:--|:--|
| [`retry-budget/`](retry-budget/README.md) | A checkout service, an incident, the decision that replaced an earlier one and the measurement that verified it. The same memory `kmp-mcp demo` writes. | "Why are retries to the payment provider capped at two?" |
| [`kmp-project/`](kmp-project/README.md) | KMP's own memory: the decisions behind this repository, each with its source. It is what the committed [`.kmp/memory.jsonl`](../.kmp/memory.jsonl) holds. | "Why does a release advance the marketplace branch last?" |

## Load one

Into an empty store, replaying the bundle as it was exported:

```bash
KMP_MCP_DATA_DIR=/tmp/kmp-example kmp-mcp import examples/retry-budget/memory.jsonl
KMP_MCP_DATA_DIR=/tmp/kmp-example kmp-mcp viewer
```

Into the store a project already uses, bringing one about in beside what is
there:

```bash
kmp-mcp import --from examples/retry-budget/memory.jsonl --about example:kmp-demo
```

Then ask your agent, naming KMP and the about. The guides teach the agent
which move answers which question; the examples give it something to answer
about.

## How an example is written

`requests.json` is a JSON array of `kmp_ingest` arguments: explicit ids,
coordinates on named dimensions, evidence that supports each entry, and
relations that carry a `why` and the evidence for it. It is the same shape the
installed guide ships in, and the same boundary an agent's `kmp_write_memory`
compiles to.

`memory.jsonl` is generated from it by replaying the packets through the
public MCP writer of a temporary store and exporting the result:

```bash
cargo build --locked -p kmp-mcp
bash scripts/examples/refresh.sh
```

Edit the requests, not the bundle. The refresh regenerates every example and
the repository's own `.kmp/memory.jsonl`; a bundle written by an older engine
is replaced by one this engine reads. The crate test `example_bundles` imports
every shipped bundle into an empty store.

## Add one

1. Create `examples/<name>/requests.json` with one or more packets about
   `example:<name>`. Every id inside the about starts with it.
2. Give each entry evidence with a `source` a reader could check, and give
   each relation a `why` and an `evidence` quote. KMP validates the shape; the
   reader judges the content.
3. Add the bundle line to `scripts/examples/refresh.sh`, run it, and write the
   README with three questions the memory answers.
