---
name: kmp-guide
description: Synchronize KMP's versioned agent and human guides, learn the agent verbs from guide:kmp-agent, and open the human guide visually in ChronoLoom. Use for guide setup, guide updates, or when the user asks to open, show, or explain the KMP guide.
---

# KMP guide

Resolve the plugin root as two directories above this `SKILL.md`.

Name the data effect first: this command writes into KMP's currently selected
store. In a project it changes project memory and `.kmp/memory.jsonl`; it is
not part of software install or update. Then run
`<plugin-root>/scripts/kmp-guide-sync.sh sync`. It deterministically converges
`guide:kmp-agent` and `guide:kmp` to the content shipped with the installed
plugin and is idempotent on an already-current store.

The guides are different:

- `guide:kmp-agent` is for the agent. Wake it and use its verb entries as an
  operational glossary: when to use the verb, when not to, minimum input,
  expected result and usual next move. Its exact tool descriptions are
  generated from the installed engine's live `tools/list`.
- `guide:kmp` is for the person. Do not paste or paraphrase the whole guide
  into chat; open it visually.

After sync, call `kmp_wake` on `guide:kmp-agent`. Read the packet before
explaining or demonstrating KMP. If the packet is unexpectedly empty, report
that guide sync did not become visible to this MCP process; do not invent the
guide from repository prose.

Then perform `open:guide`:

1. Call `kmp_view_open` once with `about: "guide:kmp"`.
2. Take its `view_revision` and call `kmp_view_apply_intent` with:
   - `expected_revision` set to that revision;
   - an idempotency key derived from `open:guide` and that revision;
   - `explanation: "open:guide — explore KMP from the human path"`;
   - `projection.semantic_zoom: "atlas"`;
   - `projection.dimensions: ["audience", "depth"]`;
   - `selection: "guide:kmp:welcome"`.
3. Call `kmp_view_get_state` before continuing and hand the returned
   capability URL to the person.

If another participant moved the loom first, get state and rebase the intent
on the new revision. Never retry blind and never reopen the view merely to
navigate it.

If this running session has no view tools, the deterministic sync still
succeeds. Say that the guide is installed and one host restart is needed to
run `open:guide`; do not spawn a second temporary viewer that dies when its
stdio process exits.

Keep the handoff compact. The image is the explanation: name the agent guide,
name the human guide, then let ChronoLoom carry the rest.
