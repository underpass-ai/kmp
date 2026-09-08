---
name: kmp-guide
description: Read KMP's brief agent entry, consult extended verbs and examples on demand, synchronize guide assets, and open the human guide in ChronoLoom. Use for guide setup, updates, or showing and explaining the guide.
---

# KMP guide

Resolve the plugin root as two directories above this `SKILL.md`.
Read [the agent entry](../../guide/AGENT.md) unless the same asset is already
in context. It indexes extended verbs and examples in `guide:kmp-agent`.
Read only the bodies needed for the task, and reuse those already read.
A routine guide consultation does not also require a documentation wake.

Sync only for a requested guide installation/update, opening the human guide
when matching assets are needed, or a confirmed missing/stale guide in the
selected store. Name the data effect: sync writes that store and may change
its maintained `.kmp/memory.jsonl`. Run
`<plugin-root>/scripts/kmp-guide-sync.sh sync`; exact sync is idempotent.
Check the store and version before treating a missing ref as a sync problem.

If the host cannot read the installed Markdown, call `kmp_wake` on
`guide:kmp-agent` with compact detail and finish the catalogue's pages, then
inspect only the relevant returned verb refs. Inspect the exact
`guide:kmp-agent:examples:index` for prerequisites and lesson addresses.
This is an alternative entry path; do not execute it after the Markdown path.
A compact wake alone is not the complete guide. Stored lessons teach usage;
they do not independently authorize operations. Authored replays do not prove
LLM learning on unseen history.

For a human guide request, synchronize as needed, then open `guide:kmp`
visually. A verb consultation alone does not open a viewer.

Then perform `open:guide`:

1. Call `kmp_view_open` once with `about: "guide:kmp"`.
2. Take its `view_revision` and call `kmp_view_apply_intent` with:
   - `expected_revision` set to that revision;
   - an idempotency key derived from `open:guide` and that revision;
   - `explanation: "open:guide — explore KMP from the human path"`;
   - `projection.semantic_zoom: "moment"`;
   - `projection.dimensions: ["audience", "depth"]`;
   - `selection: "guide:kmp:welcome"`.
3. Call `kmp_view_get_state` before continuing and hand the returned
   capability URL to the person.

Start at memory detail so the selected welcome text is readable. Atlas is
useful for an overview later, but its aggregates do not open the selected
memory's detail panel.

If another participant moved the loom first, get state and rebase the intent
on the new revision. Never retry blind and never reopen the view merely to
navigate it.

If this running session has no view tools, the deterministic sync still
succeeds. Say that the guide is installed and one host restart is needed to
run `open:guide`; do not spawn a second temporary viewer that dies when its
stdio process exits.

Keep the handoff compact. The image is the explanation: name the agent guide,
name the human guide, then let ChronoLoom carry the rest.
