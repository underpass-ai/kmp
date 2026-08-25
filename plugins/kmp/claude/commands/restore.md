---
description: Restore this project's memory from the copy committed in the repository
argument-hint: "[no arguments]"
allowed-tools: Bash(kmp-mcp:*), Bash(ls:*), Bash(git log:*)
---

Load the memory committed at `.kmp/memory.jsonl` into this project's store.
This is what makes a fresh clone arrive with the project's decisions instead
of an empty memory.

```bash
kmp-mcp import
```

**It requires an empty store, and that is deliberate.** Importing is restore,
not merge: replaying a bundle over memory that already holds events could
duplicate history or interleave two timelines, and neither has a correct
answer the kernel could pick for the user. If it refuses because the store is
not empty, say so plainly and stop. Do not delete their store to make the
command work — offer the choices instead:

- keep the local memory and leave the bundle alone;
- restore into a different data directory with `KMP_MCP_DATA_DIR` and compare;
- if they genuinely want the committed copy to win, they remove `.kernel/`
  themselves, deliberately.

After a successful import, report what arrived: the event count from the
command's output, and then `kmp_wake` on the about it restored, so the user
sees the memory rather than a number. If this session has no `kmp_*` tools,
say the import succeeded and that the tools appear after a restart — do not
simulate the wake.

<!-- kmp:voice -->
**Say it in the house voice.** One line per thing, and detail only where
something needs it. The fix goes next to the problem, never in a footer. Close
with a verdict in plain words and at most one next command.

Write it young, fresh and a little freak: short sentences, present tense,
talking to the person rather than reporting on the software. No emoji soup,
and never a joke inside a failure. If the personality costs an extra line, cut
the personality.
<!-- /kmp:voice -->
