---
description: Show what this KMP install is and which memory this project opens — version, store, engine, tools and the viewer
allowed-tools: Bash(kmp-mcp:*)
---

Show the user what they are pointed at. This is the identity card, not a
diagnosis: nothing has to be wrong for it to be worth reading.

```bash
kmp-mcp info
```

Then tell them, in a few lines:

- **which memory this project opens, and why that one.** The `chosen by:` line
  is the part that matters and the part people miss — the store is resolved
  from the working directory, so the same command run somewhere else opens a
  different memory. Say which rule won: an explicit `KMP_MCP_DATA_DIR`, the
  enclosing project, or the per-user default;
- whether that store exists yet, and whether there is a committed bundle
  beside it — `(not exported yet)` means the only copy of this memory is on
  this disk;
- **where the viewer is**, as a link they can open. Most people never find out
  it exists;
- the backend, but only if it is not the plain embedded default. `fixture` is
  worth saying out loud every time: it looks real and stores nothing.

If they asked because something seems wrong, do not diagnose from this output
— point them at `/kmp:doctor`, which is built for that and names the one thing
to fix.

<!-- kmp:voice -->
**Say it in the house voice.** One line per thing, and detail only where
something needs it. The fix goes next to the problem, never in a footer. Close
with a verdict in plain words and at most one next command.

Write it young, fresh and a little freak: short sentences, present tense,
talking to the person rather than reporting on the software. No emoji soup,
and never a joke inside a failure. If the personality costs an extra line, cut
the personality.
<!-- /kmp:voice -->
