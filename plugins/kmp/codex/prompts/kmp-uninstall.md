Show me what removing KMP from this machine would take:

```bash
kmp-mcp uninstall $ARGUMENTS
```

Nothing is removed unless I pass `--apply`. Then tell me, in a few lines:

- where my memory will be saved — every store is exported into the working
  directory before it goes, and the dry run names the file. Lead with that;
- how to bring it back: `KMP_MCP_DATA_DIR=<a new directory> kmp-mcp import <the
  saved file>`. Import restores into an empty store and does not merge, so it
  has to point somewhere new;
- anything found twice, especially two engines: that is how a live session
  answers from an older binary than the one holding the fix;
- the scope: to remove one memory, require its absolute path and use `--store
  <absolute-path>`. That mode leaves every other memory, engine, plugin and
  host registration alone;
- what the verb will not touch — a committed bundle, a registration inside a
  host's config, a binary outside my home — each with the command I run myself.

After `--apply`, tell me how many events were saved and into which file, by
name. If a store was kept because its export failed, say so plainly — none of
that memory was removed — and name the reported cause. If another process is
using the store, name the owning host reported by the command. Never kill it.
Tell me to stop or restart that host, then retry the same scoped command from a
plain shell.

Never add `--apply` or `--purge` on your own initiative. `--purge` skips the
export, which is the one way this command loses memory for good.

<!-- kmp:voice -->
**Say it in the house voice.** One line per thing, and detail only where
something needs it. The fix goes next to the problem, never in a footer. Close
with a verdict in plain words and at most one next command.

Write it young, fresh and a little freak: short sentences, present tense,
talking to the person rather than reporting on the software. No emoji soup,
and never a joke inside a failure. If the personality costs an extra line, cut
the personality.
<!-- /kmp:voice -->
