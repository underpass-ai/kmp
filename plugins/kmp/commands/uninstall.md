---
description: Show what removing KMP from this machine would take — engines, stores, host wiring — saving the memory into the workspace first
argument-hint: "[--apply] [--purge] [--keep-memory]"
allowed-tools: Bash(kmp-mcp:*)
---

Show what an uninstall would take. **Nothing is removed unless the user passes
`--apply`**, and the dry run is what they read before they do:

```bash
kmp-mcp uninstall $ARGUMENTS
```

Then tell them, in a few lines:

- **where their memory will be saved.** Every store is exported into the
  working directory before it is removed, and the dry run names the file. That
  is the line to lead with: it is the difference between an uninstall and a
  loss;
- **how to bring it back** — `KMP_MCP_DATA_DIR=<a new directory> kmp-mcp import
  <the saved file>`. Import restores into an **empty** store; it does not
  merge, so it has to point somewhere new;
- **anything found twice.** Two engines is the shape that makes a live session
  answer from an older binary than the one holding the fix;
- **what this verb will not touch**: a committed bundle belongs to the
  repository, a registration lives inside a host's own configuration file, and
  a binary outside their home may be a package manager's. Each prints the
  command that removes it, and the user runs that themselves.

After `--apply`, say **how many events were saved and into which file**, by
name. A copy nobody can find is not a copy. If a store was kept because its
export failed — usually a live session still holding it — say that plainly:
nothing of that memory was removed.

Never run it with `--apply` or `--purge` on your own initiative — not when the
dry run looks harmless, and not to "finish the job". `--purge` skips the export
entirely, which is the one way this command can lose memory for good: the log
has no delete, and a store that is gone is gone. The user asks, or it does not
happen.

<!-- kmp:voice -->
**Say it in the house voice.** One line per thing, and detail only where
something needs it. The fix goes next to the problem, never in a footer. Close
with a verdict in plain words and at most one next command.

Write it young, fresh and a little freak: short sentences, present tense,
talking to the person rather than reporting on the software. No emoji soup,
and never a joke inside a failure. If the personality costs an extra line, cut
the personality.
<!-- /kmp:voice -->
