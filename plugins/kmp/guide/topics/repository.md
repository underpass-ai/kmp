## Memory can live in the repository

Project-scoped embedded writes maintain `.kmp/memory.jsonl` automatically and
atomically before returning success. The store itself (`.kernel/`) is machine
state and stays gitignored — the bundle is a different thing, and committing
it is what makes a fresh clone arrive with the project's decisions instead of
an empty memory. `kmp-mcp export` is the explicit repair/checkpoint command;
`kmp-mcp import` reads the bundle into an empty store.

If `doctor` reports a pending export, stop other KMP sessions, run the normal
export and inspect its diff, then use `kmp-mcp export --repair-pending` to
acknowledge recovery. The normal export intentionally does not clear an
in-flight marker that could belong to another SQLite writer.

It is one JSON object per line in sequence order, so an append-only log
appears in `git diff` as appended lines. A session that recorded three
decisions is three new lines plus the header update, and each line carries who
wrote it and the rationale of every relation, verbatim. The format-2 header
names the snapshot, creation time, event range, about coverage and SHA-256, so
a saved copy can be verified before restore.

Use named snapshots when a release or risky change needs a recovery point:

```bash
kmp-mcp snapshot create pre-release
kmp-mcp snapshot verify pre-release
kmp-mcp snapshot read pre-release kmp_goto \
  '{"about":"project:kmp","at":{"sequence":12}}'
```

The read happens in an isolated temporary store and cannot call either writer.
For two branches, `kmp-mcp snapshot merge <left> <right> <new-name>` only
fast-forwards an exact prefix. A divergence is a semantic conflict: never
hand-interleave the JSONL lines.

Two limits to state rather than discover. **Import requires an empty store**:
it is restore, not merge, because replaying a bundle over existing memory
could duplicate history or interleave two timelines and neither has an answer
the kernel could pick. And **a bundle carries the payloads as written** — a
secret in memory is a secret in the committed file, so the hygiene of the
bundle is the hygiene of the store.
