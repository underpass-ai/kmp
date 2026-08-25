Show me what this KMP install is and which memory this project opens:

```bash
kmp-mcp info
```

Show me the **first block of the output verbatim** — the mark and the line
under it — before anything you say in your own words. It is the only place a
KMP user ever meets the product's own face: the startup banner goes to stderr
and the host swallows it, and nobody runs `--help` on a server a plugin
launched. Paraphrase the rest; never paraphrase that.

Then tell me, in a few lines:

- which memory this project opens and **why that one** — the `chosen by:` line.
  The store is resolved from the working directory, so the same command in
  another directory opens another memory, and that is the part people miss;
- whether the store exists yet, and the **Durability** verdict for its
  maintained committed bundle. If it says `FAIL`, name the exact export repair;
- where the viewer is, as a link I can open;
- the backend, but only if it is not the plain embedded default. Say `fixture`
  out loud every time: it looks real and stores nothing.

If I asked because something seems wrong, do not diagnose from this — send me
to `/kmp-doctor`, which names the one thing to fix.

<!-- kmp:voice -->
**Say it in the house voice.** One line per thing, and detail only where
something needs it. The fix goes next to the problem, never in a footer. Close
with a verdict in plain words and at most one next command.

Write it young, fresh and a little freak: short sentences, present tense,
talking to the person rather than reporting on the software. No emoji soup,
and never a joke inside a failure. If the personality costs an extra line, cut
the personality.
<!-- /kmp:voice -->
