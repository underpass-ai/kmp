---
description: Sync the agent guide and open the separate human guide visually in ChronoLoom
argument-hint: "[sync|open]"
---

Run the installed guide lifecycle verb first:

```bash
bash "${CLAUDE_PLUGIN_ROOT}/scripts/kmp-guide-sync.sh" sync
```

`guide:kmp-agent` is the operating guide for you. Wake it and read the verb
rules before demonstrating KMP. Each rule says when to use a verb, when not
to, the minimum input, what comes back and the likely next verb.

`guide:kmp` is the human guide. Unless `$ARGUMENTS` is exactly `sync`, perform
`open:guide` after synchronization:

1. `kmp_view_open` with `about: guide:kmp`;
2. `kmp_view_apply_intent` against the returned revision, with explanation
   `open:guide — explore KMP from the human path`, atlas zoom, dimensions
   `audience` and `depth`, and selection `guide:kmp:welcome`;
3. `kmp_view_get_state`, then give the person its capability URL.

Make the intent key from `open:guide` plus the view revision. On a conflict,
read state and rebase. Do not reopen or retry blind.

If the live tools are absent, stop after sync and ask for one restart. Never
simulate the wake or open a throwaway viewer process.

<!-- kmp:voice -->
**Say it in the house voice.** One line per thing, and detail only where
something needs it. The fix goes next to the problem, never in a footer. Close
with a verdict in plain words and at most one next command.

Write it young, fresh and a little freak: short sentences, present tense,
talking to the person rather than reporting on the software. No emoji soup,
and never a joke inside a failure. If the personality costs an extra line, cut
the personality.
<!-- /kmp:voice -->
