---
description: Diagnose the KMP agent-memory setup — binary, backend, data directory, tool surface and host wiring
allowed-tools: Bash(bash:*), Bash(claude mcp list:*), Bash(kmp-mcp:*)
---

Run the KMP doctor and report what it found:

```bash
bash "${CLAUDE_PLUGIN_ROOT}/scripts/kmp-doctor.sh"
```

Show them the **first block of the output verbatim** — the mark and the line
under it — before anything you say in your own words. It is the only place a
KMP user ever meets the product's own face: the startup banner goes to stderr
and the host swallows it, and nobody runs `--help` on a server a plugin
launched. Paraphrase the rest; never paraphrase that.

Then tell the user, in a few lines:

- whether KMP memory is usable right now, plainly — yes or no;
- if not, the **single** thing blocking it and the exact command that fixes
  it, taken from the doctor output rather than invented;
- any warning that will bite later even though nothing is broken yet — a
  `fixture` backend (memory that looks real and is not), or a registration
  that exists but has not been picked up because the session predates it.

If the doctor reports the tools answering but this session still has no
`kernel_*` tools in its inventory, say so directly: the registration is
correct and the session is stale, so it needs restarting. That is the one
failure the doctor cannot fix from inside the session.

Do not paraphrase the whole output — the mark and the verdict are what to show
verbatim, and the rest is what to say in your own words. The user wants the
verdict and the next command, not a transcript of the checks.

<!-- kmp:voice -->
**Say it in the house voice.** One line per thing, and detail only where
something needs it. The fix goes next to the problem, never in a footer. Close
with a verdict in plain words and at most one next command.

Write it young, fresh and a little freak: short sentences, present tense,
talking to the person rather than reporting on the software. No emoji soup,
and never a joke inside a failure. If the personality costs an extra line, cut
the personality.
<!-- /kmp:voice -->
