# Concepts

The words KMP uses, in one page. Each tool's arguments are explained by
`kmp_guide {"topic": "<topic>"}`; the cards it serves are in
[`plugins/kmp/guide/cards`](../plugins/kmp/guide/cards/).

| Word | Meaning |
|:--|:--|
| **About** | The subject a memory belongs to, as an opaque string: `project:kmp`, `example:kmp-demo`. Every read and write names one (or several, for `kmp_relate`). |
| **Memory** | One fact: a decision, constraint, observation or outcome, with its `kind`, a short `summary` and the **evidence** it rests on. Written by `kmp_write_memory` (topic `write`). |
| **Evidence** | The source text that supports a memory or a relation. KMP stores it byte for byte, never translates it, and answers only from it. |
| **Label** | A `key = value` pair that makes memories navigable (`component = cache`). Changed with `kmp_relabel` (topic `relabel`). |
| **Relation** | A typed link between two memories, with a `why` (what connects them) and `evidence` (what proves it). KMP never invents a missing `why`. Topic `relations`. |
| **Clocks** | Four: `occurred` (when it happened), `observed` (when it was known), `ingested` (when KMP recorded it), `validity` (when a state held). Reads name the clock they move on (`axis`). Topic `time`. |
| **Supersession** | A newer decision replaces an older one without deleting it. Both stay readable in time; `kmp_wake` and `kmp_ask` serve the one in force. |
| **Wake** | `kmp_wake`: recover the current state of an about before continuing work. Topic `wake`. |
| **Ask** | `kmp_ask`: a semantic question answered from stored evidence. Every answer says `answer_status`: `answered`, `partial` or `unknown`. Topic `ask`. |
| **UNKNOWN** | An honest answer: the selected memory holds no eligible evidence. `unknown_reason` says why. It is not an error. |
| **Proof** | What a read returns besides its answer: the cited memories, the path that connects them, and what is missing. `kmp_trace` and `kmp_inspect` audit it (topics `trace`, `audit`). |
| **Continuation** | A handle a partial read returns. Send it back alone to get the next page; it is kept 24 hours. |
| **Store** | The SQLite database that holds the memory. Which one opens is decided per directory: [where memory lives](embedded/README.md#where-memory-lives). |
| **Project bundle** | `.kmp/memory.jsonl`: the portable event log of a project store, which you may commit. [Store format](embedded/store-format.md). |
| **ChronoLoom** | The viewer inside the binary. It shows an about in time, and the agent can steer it with the view tools (topic `view`). |
| **Lexical bridge** | An optional word table that lets a question in one language reach a memory in another. [Languages](embedded/languages.md). |
| **TypeSafe Jev** | An optional external judge a store can opt into. Off by default. [What it sends](security-privacy.md#typesafe-jev-opt-in-per-store). |

## The tools

`tools/list` of the running server is the authority. At 1.0 it lists 16:

| Tool | What it does | Topic |
|:--|:--|:--|
| `kmp_guide` | Explain one topic: a worked card and the arguments of its tools. | `guide` |
| `kmp_wake` | Recover compact state before continuing work. | `wake` |
| `kmp_ask` | Retrieve evidence for a question, or `UNKNOWN`. | `ask` |
| `kmp_relate` | Compare explicitly selected abouts: shared scopes, declared relations, tensions. | `relate` |
| `kmp_time` | Move through memory on one clock: `rewind`, `forward`, `goto`, `near`. | `time` |
| `kmp_trace` | Find the evidenced path from a stored memory to known destinations, or the evidence for required roles from that seed. | `audit`, `trace` |
| `kmp_inspect` | Read one stored memory or write receipt in full, with its evidence and links. | `audit` |
| `kmp_write_memory` | Validate and record decisions, constraints and outcomes. | `write`, `relations`, `review` |
| `kmp_ingest` | Ingest one exact canonical memory document. | `ingest` |
| `kmp_relabel` | Change the labels of a stored memory without rewriting its text. | `relabel` |
| `kmp_condense` | Write a compact reader card or a search summary for a stored body. | `condense` |
| `kmp_summaries_audit` | List memories whose English search summary is missing, refused or weak. | `condense` |
| `kmp_curate` | Review relations with TypeSafe Jev as an opt-in second reader. | `curate` |
| `kmp_view_open` | Open a ChronoLoom view over an about. | `view` |
| `kmp_view_apply_intent` | Move that view by meaning: focus, clock, zoom, filters, selection. | `view` |
| `kmp_view_get_state` | Read the view's semantic state. | `view` |

Reads never write memory. The view tools change only the view, and a person
at the loom has right of way.
