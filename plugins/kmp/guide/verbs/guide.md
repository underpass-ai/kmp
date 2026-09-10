Start one agent with `kmp_guide {"registration_key":"unique-task-and-agent-key"}`.
Keep the returned agent id, random name and context id. Repeat that registration
key only for the same logical agent; another agent needs another key.

Resume with `{"context_id":"<returned context_id>"}`. Expand a worked card with
`{"context_id":"<returned context_id>","topic":"write"}`. Fold it with the same
arguments plus `"fold":true`. Folding keeps its delivery record; asking again
serves the card again. Extended verb guidance is in `next_actions`.

After compaction or a task change, use
`{"agent_id":"<returned agent id>","context_key":"unique-reset-key"}`. Keep the
new context id. The agent keeps its identity and random name but starts with no
guidance served in that context. Repeating the reset key resumes that new context.
A guide revision change also starts a fresh view of served topics; older delivery
records remain in storage.

These are delivery records, not proof that you understood or still retain a
lesson. They grant no additional permissions. Different agents sharing a host
or connection keep different identities. KMP never infers identity from the
connection. Preserve these ids in the host's task state; do not substitute the
display name. `durable=false` identifies an in-memory test backend, not persistence.

## Guidance during work

Pass `context_id` with memory and viewer calls. It is interaction metadata,
removed before the memory operation, its hash or pagination contract. It does
not grant access. Writer and Relabel may omit actor when this context supplies
the persistent name; an explicit actor takes precedence.

Before an unfamiliar operation, request its topic card and consult the linked
verb/examples you need. A valid first operation is never blocked merely because
a delivery mark is absent. Help returned after an operation cannot prevent a
mistake already made. If uncertain, ask `kmp_guide` explicitly even if a topic
was served before.

A work result adds one JSON text block containing `kmp_guidance`. The original
structuredContent, proof, receipt and their byte budget stay unchanged. The
extra block has its own context cost; consumers must pass it through to the
agent. It is not duplicated in _meta. No context means no automatic usage block.

`usage` counts observed calls, refusals and protocol UNKNOWNs; it is not a
learning score or a count of committed writes. A retry may count as another call
while idempotency still keeps one memory. `usage.recorded=false` means metadata
failed: keep the original operation result and never repeat a write just to fix
usage accounting. `served` records requested cards and exact guide/example refs
whose bodies were actually returned with context. A suggested link or reused
object without text does not count. An observed asset revision change starts a
new delivery view; ordinary work reports the last seen revision and does not
pretend to know an unread guide has changed.

Set optional `purpose` to `continue` (default), `audit`, `history` or `answer`.
The recommendation uses typed result fields in this order:

| Situation | Suggested move |
| --- | --- |
| Refusal with executable feedback | Review the producer's repair or restart |
| Partial packet | Execute the native continuation, preserving selection |
| Semantic UNKNOWN after completing the packet | Stop within that selection |
| History with native next action | Use its clock and dimensions for a new selection |
| History from current Inspect | Consult time guidance and choose a clock |
| Audit of declared links in Inspect | Trace one stored relation and assess why/evidence |
| Audit/answer from single-about recall | Inspect a canonical claim ref as a fresh current read |

The link choice has stable lexical order, not an importance score. Alternatives
remain in the original response. `basis` points to the fields used;
`changes_selection` identifies a fresh read. An inspection suggested after
historical recall is current, not proof about the prior date. Multi-about recall
without explicit source ownership never invents an about. No recommendation
executes itself, widens an UNKNOWN search, proves a relation true or authorizes
a write. `signals` carries observable packet completeness and write coverage;
coverage of a packet does not assess fidelity to its source.

## Consult a protocol expert

When the cards and examples leave a protocol question unresolved, the installed
`kmp-expert` skill can prepare a separate host subagent with the complete agent
guide and all examples. Its local cache is keyed by the shipped asset; the
expert loads that content before consultations, checks the store's revision and
keeps its own persistent identity. After context loss it must reload the guide.
The working agent receives short explanations and concrete call proposals,
checks them against its sources and executes them under existing authorization.
This is host orchestration: KMP does not run a model. Local preload neither
marks MCP lessons served nor proves understanding, and its context cost counts
alongside the working agent's cost. See that skill for the preparation workflow.
