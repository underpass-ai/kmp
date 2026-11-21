## Memory can live in the repository

Project-scoped embedded writes maintain `.kmp/memory.jsonl` automatically and
atomically before returning success. The store itself (`.kernel/`) is machine
state and stays gitignored — the bundle is a different thing, and committing
it is what makes a fresh clone arrive with the project's decisions instead of
an empty memory.

When a session opens a project store it compares the store with the bundle,
event by event:

- **The bundle is ahead** (a fresh clone, or a `git pull` that brought a
  teammate's memory): the store is caught up by appending the bundle's tail
  in the bundle's own order. Nothing to run; the first read already sees it.
  A write made in a session that was open before the pull catches up the
  same way first.
- **The store is ahead** after an interrupted write (a pending marker, the
  bundle an exact prefix of the store, no other session writing): the store
  is published to the bundle and the marker cleared.
- **The two diverge** (both sides hold events the other lacks): nothing is
  changed. Reads carry a notice and writes are refused with both ways out:
  move `.kernel/` aside and reopen to rebuild from the bundle (an about only
  your store wrote comes back with `kmp-mcp import --from <moved store>
  --about <about>` and then `kmp-mcp export`; what you added to an about the
  bundle also changed cannot be merged and has to be written again); or keep
  the store's history with `kmp-mcp export --overwrite-committed`.

Do not catch a project store up with one `import --from .kmp/memory.jsonl
--about` per about: a tail that interleaves several abouts would be appended
grouped by about, which is a different history from the bundle's, and every
later write would refuse it.

`kmp-mcp export` is the explicit checkpoint command. With no file it writes
the project head; a full export catches the store up first, and it refuses to
replace a committed bundle whose events it would not keep unless you pass
`--overwrite-committed`. `kmp-mcp import <file>` reads a bundle into an empty
store. If `doctor` still reports a pending export (the bundle is not a prefix
of the store), stop other KMP sessions, run the normal export and inspect its
diff, then use `kmp-mcp export --repair-pending` to acknowledge recovery.

A store directory whose database file is gone, beside files that prove it
held memory (the telemetry salt, a write-ahead log, an indexed lexical
index), is refused rather than restarted empty. Restore the database, or move
the directory aside; a project store is then rebuilt from its bundle.

The bundle contains a header followed by one JSON event per line in sequence
order. An event can contain several memories and relations, so the number of
new lines is not the number of decisions. The payload preserves stored
authorship and relation rationale. The format-3 header (`bundle_format: 3`,
`event_format: 2` for memory-only histories, `3` when authored card events are
present) names the snapshot, creation time, event range, about
coverage and SHA-256, so a saved copy can be verified before restore. The
portable event format and the on-disk store format are separate contracts.
The reader accepts both event formats. New SQLite stores use layout format 4;
format-3 stores upgrade on open, fencing older binaries before card adoption.
Stop older processes before upgrading a shared store. Card-bearing bundles
require an engine that supports event format 3; older engines reject them. On first open, surviving
pre-event cards are adopted as baseline events. Their earlier overwritten
versions cannot be recovered. Export the adopted history to the project's
maintained bundle before resuming guarded writes if its existing bundle is
now behind the store.

Use named snapshots when a release or risky change needs a recovery point:

```bash
kmp-mcp snapshot create pre-release
kmp-mcp snapshot verify pre-release
kmp-mcp snapshot read pre-release kmp_time \
  '{"about":"project:kmp","move":"goto","at":{"sequence":12}}'
```

The read happens in an isolated temporary store and cannot call either writer.
For two branches, `kmp-mcp snapshot merge <left> <right> <new-name>` only
fast-forwards an exact prefix. A divergence is a semantic conflict: never
hand-interleave the JSONL lines.

Two limits to state rather than discover. **`kmp-mcp import <file>` requires
an empty store**: it is restore, not merge, because replaying a bundle over
existing memory could duplicate history or interleave two timelines and
neither has an answer the kernel could pick. `import --from … --about …`
brings exact abouts into a store that holds memory only when they are absent
there or this store holds an exact prefix of them. And **a bundle carries the payloads as written** — a
secret in memory is a secret in the committed file, so the hygiene of the
bundle is the hygiene of the store.
