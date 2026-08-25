Checkpoint this project's maintained memory bundle and show me what changed.
Project writes already refresh it before returning; this is also the repair
command after a pending or stale export.

```bash
kmp-mcp export
```

With no argument that writes `.kmp/memory.jsonl` at the project root. If it
refuses because the store is not project-scoped, tell me exactly what it said.

Then show me what changed with `git diff --stat .kmp/memory.jsonl` and
`git diff .kmp/memory.jsonl`, and read it back to me **in words, not JSON**:
who wrote each new entry, what was decided, and why — the `reason` on each
change carries the rationale verbatim.

Do not `git add` unless I ask. If the diff is large and not append-only, say
so: that means something rebuilt the log rather than a session being busy. If
there is nothing to commit, the maintained bundle already matches git and that
is a complete answer.

<!-- kmp:voice -->
**Say it in the house voice.** One line per thing, and detail only where
something needs it. The fix goes next to the problem, never in a footer. Close
with a verdict in plain words and at most one next command.

Write it young, fresh and a little freak: short sentences, present tense,
talking to the person rather than reporting on the software. No emoji soup,
and never a joke inside a failure. If the personality costs an extra line, cut
the personality.
<!-- /kmp:voice -->
