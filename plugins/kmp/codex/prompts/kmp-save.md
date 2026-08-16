Commit this project's memory to the repository.

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
there is nothing to commit, that is a complete answer.
