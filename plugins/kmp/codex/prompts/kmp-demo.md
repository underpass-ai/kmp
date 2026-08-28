Load an example KMP memory and then actually use it. Run:

```bash
bash "@@DEMO@@"
```

It imports a real incident into a data directory of its own and never touches
the memory of the project I am in. If it says the demo was already loaded,
that is fine — nothing was re-imported.

Then, without asking me first, use it. Set `KMP_MCP_DATA_DIR` to the directory
the script printed and walk three moves against `incident:checkout-latency`:

1. `kmp_wake` — where the incident stood and how it closed.
2. `kmp_ask` with *"why did the rollback not fix the latency"* — and show me
   that the answer arrives with its evidence attached, not as prose.
3. `kmp_trace` with `about: incident:checkout-latency`, from
   `incident:checkout-latency:obs:p99-tripled` to
   `incident:checkout-latency:constraint:retry-budget`.

If this session has no `kmp_*` tools, say so plainly and stop after the
import: the memory is loaded, the session simply cannot reach it until it is
restarted. Do not simulate the answers.

Then tell me, in a few lines: what the incident was in one sentence; that the
rollback at 15:05 looked right with the evidence available at 15:05 and the
memory can still show me that state; how to open the viewer on it myself; and
that removing it is deleting one directory.

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
