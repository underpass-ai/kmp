---
name: kmp-memory
description: Operate KMP agent memory through the kmp MCP server — recover stored context instead of re-deriving it, answer questions from stored evidence, navigate history in time, audit a claim back to its proof, and record decisions with relations that carry their why. Use it when the user asks for KMP or its memory in any language, when a /kmp:* skill or command runs, when the project's own instructions opt in, or when the MCP initialize instructions report always-on memory routing. Without one of those, work from the material already in front of you and make no KMP call.
---

# KMP agent memory

Start guidance once with `kmp_guide` and a unique `registration_key` for this
logical agent. Keep its returned agent and context ids in the host task state.
Pass `context_id` with work calls. Before an unfamiliar verb, add `topic` to
`kmp_guide` to read its worked card and follow the extended guide when needed.
Read the optional `kmp_guidance` text block alongside the original result. Reuse current bodies already in context.
After compaction use `agent_id` and a new `context_key`; the agent keeps its
identity and random name. Served guidance does not prove understanding.

[The installed entry](../../guide/AGENT.md) is an alternative entry, not a
second manual to load. Missing or stale guide assets belong to
[guide](../kmp-guide/SKILL.md); missing tools or an unexpected store belong to
[doctor](../kmp-doctor/SKILL.md).

For relation rationale (the former “Why the `why` matters” section), consult
`guide:kmp-agent:advanced:relations` in `guide:kmp-agent`. This skill is the
entry router; the rules live in the indexed guide bodies.
