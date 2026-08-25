---
description: Checkpoint this project's maintained memory bundle and show what changed
argument-hint: "[no arguments]"
allowed-tools: Bash(kmp-mcp:*), Bash(git diff:*), Bash(git status:*), Bash(git add:*)
---

Checkpoint this project's memory bundle and show what is ready to commit.
Project-scoped embedded writes already maintain the file before they return;
this command is also the explicit repair path after `doctor` reports a pending
or stale export.

```bash
kmp-mcp export
```

With no argument that writes to `.kmp/memory.jsonl` at the project root,
creating the directory on the first save. If it refuses because the store is
not project-scoped, report exactly what it said — an explicit
`KMP_MCP_DATA_DIR` or the per-user store belongs to no repository, and
guessing one for it would put memory somewhere the user did not choose.

Then show what changed:

```bash
git diff --stat .kmp/memory.jsonl
git diff .kmp/memory.jsonl
```

The diff is the point of doing this at all. A bundle is one JSON object per
line in sequence order, so a session that added three decisions shows up as
three appended lines plus its identified header. Read them: each line
carries `requested_by`, and each change carries its `reason` — including the
rationale of any rich relation, verbatim. Tell the user **what was added, in
their words, not in JSON**: who wrote it, what was decided, and why.

Do not `git add` unless they ask. Committing memory is their call, and it is a
different judgement from committing code — this file is a durable record of
what the work concluded, and it will be read by whoever reviews the pull
request.

Two things to say out loud when they apply:

- **The diff is large and not append-only.** That means something rebuilt the
  log, not that a session was busy. Say so; it is worth understanding before
  committing.
- **There is nothing to commit.** The maintained bundle already matches git.
  That is a normal outcome and a complete answer.

<!-- kmp:voice -->
**Say it in the house voice.** One line per thing, and detail only where
something needs it. The fix goes next to the problem, never in a footer. Close
with a verdict in plain words and at most one next command.

Write it young, fresh and a little freak: short sentences, present tense,
talking to the person rather than reporting on the software. No emoji soup,
and never a joke inside a failure. If the personality costs an extra line, cut
the personality.
<!-- /kmp:voice -->
