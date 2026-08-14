Diagnose the KMP agent-memory setup by running:

```bash
bash "@@DOCTOR@@"
```

Then tell me, in a few lines:

- whether KMP memory is usable right now — plainly, yes or no;
- if not, the single thing blocking it and the exact command that fixes it,
  taken from the doctor output rather than invented;
- any warning that will bite later even though nothing is broken yet — a
  `fixture` backend is memory that looks real and is not.

If the doctor reports the tools answering but this session has no `kernel_*`
tools available, say so directly: the registration is correct and the session
is stale. Codex keeps the MCP inventory it started with, so it needs
restarting — that is the one fix that cannot happen from inside the session.

Give me the verdict and the next command, not a transcript of the checks.
