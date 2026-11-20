---
name: kmp-memory
description: Operate KMP agent memory through the kmp MCP server — recover stored context instead of re-deriving it, answer questions from stored evidence, navigate history in time, audit a claim back to its proof, and record decisions with relations that carry their why. Use it when the user asks for KMP or its memory in any language, when a /kmp:* skill or command runs, when the project's own instructions opt in, or when the MCP initialize instructions report always-on memory routing. Without one of those, work from the material already in front of you and make no KMP call.
---

# KMP agent memory

Before a verb's first use, `kmp_guide {"topic":"<topic>"}` returns its worked
card and the meaning of each argument; `kmp_guide {}` lists the topics. It is
a plain lookup: nothing to register, nothing recorded. A refusal ends with a
`Help: kmp_guide {"topic":…}` line naming the card to read. Reuse cards still
in context.

[The installed entry](../../guide/AGENT.md) is an alternative entry, not a
second manual to load. Missing or stale guide assets belong to
[guide](../kmp-guide/SKILL.md); missing tools or an unexpected store belong to
[doctor](../kmp-doctor/SKILL.md).

For relation rationale (the former “Why the `why` matters” section), consult
`guide:kmp-agent:advanced:relations` in `guide:kmp-agent`. This skill is the
entry router; the rules live in the indexed guide bodies.
