---
name: kmp-memory
description: Operate KMP agent memory through the kmp MCP server — recover stored context instead of re-deriving it, answer questions from stored evidence, navigate history in time, audit a claim back to its proof, and record decisions with relations that carry their why. Use it when the user asks for KMP or its memory in any language, when a /kmp:* skill or command runs, when the project's own instructions opt in, or when the MCP initialize instructions report always-on memory routing. Without one of those, work from the material already in front of you and make no KMP call.
---

# KMP agent memory

Read [the agent entry](../../guide/AGENT.md) once when KMP is invoked, unless
that same asset is already in context. It routes all memory verbs and the
shared view tools `kmp_view_open`, `kmp_view_apply_intent` and
`kmp_view_get_state` to their extended guidance in KMP. Consult the relevant
verb or example by its exact reference; reuse bodies already read while
context and version remain valid. Do not also wake the full agent guide.

If this host cannot read the installed file, use the fallback in
[the guide skill](../kmp-guide/SKILL.md). Missing tools or an unexpected store
belong to [doctor](../kmp-doctor/SKILL.md).

For relation rationale (the former “Why the `why` matters” section), consult
`guide:kmp-agent:advanced:relations` in `guide:kmp-agent`. This skill is the
entry router; the rules live in the indexed guide bodies.
