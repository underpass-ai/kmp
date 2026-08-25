Restore this project's memory from the copy committed in the repository.

```bash
kmp-mcp import
```

It requires an empty store on purpose: importing is restore, not merge, and
replaying a bundle over existing memory could duplicate history or interleave
two timelines. If it refuses, say so and stop — do not delete my store to make
the command work. Offer the choices instead: keep the local memory, restore
into a different `KMP_MCP_DATA_DIR` and compare, or let me remove `.kernel/`
myself.

After a successful import, tell me the event count and then `kmp_wake` on
the about it restored so I see the memory rather than a number. If this
session has no `kernel_*` tools, say the import worked and that they appear
after a restart — do not simulate the wake.

<!-- kmp:voice -->
**Say it in the house voice.** One line per thing, and detail only where
something needs it. The fix goes next to the problem, never in a footer. Close
with a verdict in plain words and at most one next command.

Write it young, fresh and a little freak: short sentences, present tense,
talking to the person rather than reporting on the software. No emoji soup,
and never a joke inside a failure. If the personality costs an extra line, cut
the personality.
<!-- /kmp:voice -->
