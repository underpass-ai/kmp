---
description: Load an example KMP memory — a real incident with a wrong turn in it — into its own data directory, so memory can be seen before it is written
allowed-tools: Bash(bash:*), Bash(kmp-mcp:*)
---

Load the example memory and then show the user what is in it:

```bash
bash "${CLAUDE_PLUGIN_ROOT}/scripts/kmp-demo.sh"
```

The script imports into a data directory of its own and never touches the
memory of the project the user is in. If it reports the demo was already
loaded, that is fine — nothing was re-imported.

Then, **without asking first**, actually use the memory. The point of the demo
is to watch the moves answer, not to read about them. Set
`KMP_MCP_DATA_DIR` to the directory the script printed and walk three moves
against `incident:checkout-latency`:

1. `kernel_wake` — where the incident stood and how it closed.
2. `kernel_ask` with *"why did the rollback not fix the latency"* — and show
   that the answer arrives with its evidence attached, not as prose.
3. `kernel_trace` from `incident:checkout-latency:obs:p99-tripled` to
   `incident:checkout-latency:constraint:retry-budget` — six hops from the
   first symptom to the rule that ended it.

If this session has no `kernel_*` tools, say so plainly and stop after the
import: the memory is loaded and ready, the session simply cannot reach it
until it is restarted. Do not simulate the answers.

Then tell the user, in a few lines:

- what the incident was, in one sentence;
- the thing worth noticing — the rollback at 15:05 looked right with the
  evidence available at 15:05, and the memory can still show you that state;
- how to look at the graph themselves:
  `KMP_MCP_DATA_DIR=<dir> kmp-mcp viewer 127.0.0.1:7318` — port 7318 and not
  7317, because 7317 is already serving their own memory;
- that removing it is deleting one directory.

Do not paraphrase the whole incident. Let the moves do the talking.

<!-- kmp:voice -->
**Say it in the house voice.** One line per thing, and detail only where
something needs it. The fix goes next to the problem, never in a footer. Close
with a verdict in plain words and at most one next command.

Write it young, fresh and a little freak: short sentences, present tense,
talking to the person rather than reporting on the software. No emoji soup,
and never a joke inside a failure. If the personality costs an extra line, cut
the personality.
<!-- /kmp:voice -->
