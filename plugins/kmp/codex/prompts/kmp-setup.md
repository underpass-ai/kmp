Get KMP memory working for Codex on this machine. Diagnose first, fix only
what is broken:

```bash
bash "@@DOCTOR@@"
```

Read the doctor before changing anything. If it says the memory is usable,
say so and stop — there is nothing to install.

If the session-start notice says a newer release exists, catch up the Codex
prompts, memory doctrine and engine together:

```bash
bash "@@UPDATE@@" --codex
```

That is the one update command. Re-run the doctor afterwards and ask for one
restart so Codex loads the refreshed MCP inventory.

If the engine is missing or older than the plugin, install the matching one:

```bash
bash "@@SETUP@@"
```

If Codex is not wired, register the server. The binary needs no configuration
— an unconfigured `kmp-mcp` runs the embedded kernel:

```toml
# ~/.codex/config.toml
[mcp_servers.kmp]
command = "kmp-mcp"
```

Then re-run the doctor and tell me whether it is usable now. One line for what
was already fine, one for what you changed, and the next command if anything
is still missing.

**Codex keeps the MCP inventory it started with.** If the wiring is right and
the `kernel_*` tools are still absent, the session is stale — say so plainly.
Restarting is the fix, and it cannot happen from in here.

<!-- kmp:voice -->
**Say it in the house voice.** One line per thing, and detail only where
something needs it. The fix goes next to the problem, never in a footer. Close
with a verdict in plain words and at most one next command.

Write it young, fresh and a little freak: short sentences, present tense,
talking to the person rather than reporting on the software. No emoji soup,
and never a joke inside a failure. If the personality costs an extra line, cut
the personality.
<!-- /kmp:voice -->
