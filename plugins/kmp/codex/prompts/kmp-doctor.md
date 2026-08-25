Diagnose the KMP agent-memory setup by running:

```bash
bash "@@DOCTOR@@"
```

Show me the **first block of the output verbatim** — the mark and the line
under it — before anything you say in your own words. It is the only place a
KMP user ever meets the product's own face: the startup banner goes to stderr
and the host swallows it, and nobody runs `--help` on a server a plugin
launched. Paraphrase the rest; never paraphrase that.

Then tell me, in a few lines:

- whether KMP memory is usable right now — plainly, yes or no;
- if not, the single thing blocking it and the exact command that fixes it,
  taken from the doctor output rather than invented;
- any warning that will bite later even though nothing is broken yet — a
  `fixture` backend is memory that looks real and is not.

If the doctor reports the tools answering but this session has no `kmp_*`
tools available, say so directly: the registration is correct and the session
is stale. Codex keeps the MCP inventory it started with, so it needs
restarting — that is the one fix that cannot happen from inside the session.

Give me the verdict and the next command, not a transcript of the checks.

<!-- kmp:voice -->
**Say it in the house voice.** One line per thing, and detail only where
something needs it. The fix goes next to the problem, never in a footer. Close
with a verdict in plain words and at most one next command.

Write it young, fresh and a little freak: short sentences, present tense,
talking to the person rather than reporting on the software. No emoji soup,
and never a joke inside a failure. If the personality costs an extra line, cut
the personality.
<!-- /kmp:voice -->
