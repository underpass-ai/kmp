---
name: kmp-expert
description: Consult a dedicated KMP protocol adviser with the complete installed agent guide and examples preloaded. Use when an agent needs help choosing or repairing KMP calls beyond the progressive cards.
---

# KMP expert

The expert explains KMP's contract. The working agent keeps responsibility for
the source interpretation, evidence and actions. Run the expert in a separate
host subagent context and reuse it for this working agent while its guide stays
current. Prefer the host's inexpensive model; preserve an explicit user choice.
No additional model service or API key is needed. If the host cannot delegate,
use the relevant progressive card and say that a separate expert is unavailable.

## Prepare the expert

Resolve the plugin root from this skill. Run its bundled helper once, choosing
a writable task cache directory:

```text
python3 <skill-dir>/scripts/prepare.py --cache-dir <task-cache>
```

It returns paths to a manifest and a complete guide assembled from the shipped
`guide.requests.json`, including every agent lesson and example. Repeating the
same asset reuses the same files. This caches preparation, not model context or
tokens. Do not load the full guide into the working agent too.

Give the expert this skill, those paths and access to the live tool schemas.
Before sending the first consultation, require it to read the entire guide,
including examples, in bounded ranges without truncated output. The manifest
lists every node's ref, hash and line range. Its readiness reply names the asset
hash, guide revision and nodes read; that is a delivery record, not proof of
understanding. Missing content means it is not ready yet. Do not execute the
sample writes while preloading.

The expert registers its own stable `registration_key` through `kmp_guide` and
keeps the returned random agent name, `agent_id` and `context_id`. Compare the
returned guide revision with the manifest before advice. A mismatch requires
matching installed assets and store, using `kmp-guide`; do not silently mix them.
Local preload does not mark the MCP's per-node `served` ledger. Do not call every
node again merely to populate that ledger.

After context loss, keep `agent_id`, open a fresh `context_key` and reload the
complete guide before advice. A stored name or prior readiness message does not
restore the guide. When its revision changes, prepare and load the matching
asset again. Reuse bodies only while they are still present and current.

## Consult

Send only the protocol problem: intended operation, exact arguments, complete
error or relevant response, guide revision and known selection (about, clock,
interval, dimensions and continuation). Include needed source excerpts, marked
as data; avoid transferring the working agent's whole conversation.

The expert returns a short explanation and, when justified, a concrete
`{tool, arguments}` proposal with its guide refs. It states missing inputs and
any change of selection. Preserve existing executable continuations verbatim.
On a refusal, repair the indicated fields without inventing evidence or dropping
required labels to bypass permissions. A complete semantic `UNKNOWN` may mean
stop; a new selection requires the caller's actual goal to justify it.

The expert may inspect the guide and schemas. It does not write task memory,
change permissions, decide domain facts, infer a person's identity from an alias,
or treat an accepted packet as proof of faithful writing. The working agent
checks the proposal against the sources and decides whether to execute it under
the authorization already provided by the user.

Record the expert's preload, questions and replies alongside the writer's calls
when evaluating this workflow. Count both contexts; compare direct use and
assisted use before claiming a reduction in time, errors or tokens.
