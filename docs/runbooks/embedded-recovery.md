# Runbook: recover or move an embedded store

This runbook protects the source first. Do not copy engine files between
layouts or import a bundle over a non-empty store.

## 1. Identify the exact store

```bash
kmp-mcp info
kmp-mcp doctor
```

Record the selected path, the `chosen by` rule, engine and durability verdict.
If `KMP_MCP_DATA_DIR` is set, resolve its exact value before continuing.

## 2. Stop writers

Stop agent sessions using that store. This is mandatory for an unsupported
store format and for a pending bundle-recovery condition.

## 3. Export and verify evidence

For a project store:

```bash
kmp-mcp export
git diff -- .kmp/memory.jsonl
```

If `doctor` says the project bundle is orphaned because writes are reaching a
shared fallback store, export only the public project's exact about and inspect
the result before committing it:

```bash
KMP_MCP_DATA_DIR=/exact/shared/store kmp-mcp export .kmp/memory.jsonl --about project:name
git diff -- .kmp/memory.jsonl
```

Never export the whole shared store into a repository bundle.

A write killed between its SQLite commit and its bundle publish leaves a
pending marker. The next session that opens the project store (or a full
`kmp-mcp export`) publishes the store and clears the marker on its own when no
other session holds the commit-native lock and the committed bundle is an
exact prefix of the store. If doctor still reports a pending export, the
bundle is not such a prefix: first stop all writers, run the normal export and
inspect its diff. Only then acknowledge the recovery:

```bash
kmp-mcp export --repair-pending
```

After a `git pull` or a fresh clone the committed bundle can be ahead of the
store. Opening the store appends the bundle's tail in the bundle's order;
there is nothing to run. When the two are different histories, nothing is
changed: reads carry a notice and writes are refused. Keep the committed
history by moving `.kernel/` aside and reopening (the store is rebuilt from
the bundle; an about only your store wrote comes back with `kmp-mcp import
--from <moved store> --about <about>` followed by `kmp-mcp export`, while what
you added to an about the bundle also changed cannot be merged and has to be
written again), or keep the store's history with
`kmp-mcp export --overwrite-committed`. `export` refuses to replace a
committed bundle whose events it would not keep unless that flag is given.

A store directory whose `store/kernel.sqlite3` is missing or empty, beside a
telemetry salt, a write-ahead log or an indexed lexical index, is refused
instead of being restarted empty. Restore the database file, or move the
directory aside; a project store is then rebuilt from its bundle.

For a recovery point before a risky operation on a project store:

```bash
kmp-mcp snapshot create pre-change
kmp-mcp snapshot verify pre-change
```

Named snapshots exist only for project stores. On a store chosen by
`KMP_MCP_DATA_DIR`, a saved selection or the per-user default, every
`snapshot` verb exits 2; write a bundle instead:

```bash
kmp-mcp export /safe/pre-change.jsonl
```

## 4A. An unsupported store format

Current KMP creates store format 4 and upgrades format-3 stores on open.
Formats 1 and 2 are refused and left untouched, and no tested path brings
them into 1.0: the bundles their releases exported use event format 1, which
1.0 refuses. Keep the source directory untouched and select another store.
See [Store format](../embedded/store-format.md).

## 4B. Move a portable bundle to a fresh directory

```bash
KMP_MCP_DATA_DIR=/exact/fresh-destination kmp-mcp import .kmp/memory.jsonl
```

The destination must not contain a store. Point one test session at the
destination with `KMP_MCP_DATA_DIR`, verify it, then update the intended host
configuration.

## 4C. Restore a repository bundle

Point KMP at an empty data directory and import:

```bash
KMP_MCP_DATA_DIR=/exact/empty/destination kmp-mcp import .kmp/memory.jsonl
```

`import <file>` is restore, not merge: it refuses a non-empty destination.

## 4D. Bring exact abouts into a store that holds memory

```bash
kmp-mcp import --from /exact/source-store-or-bundle --about project:name
```

Each named about is imported when absent, left unchanged when its events are
already here, or extended when this store holds an exact prefix of them. A
different history refuses the whole import and writes nothing.

## 5. Verify before resuming

```bash
KMP_MCP_DATA_DIR=/exact/destination kmp-mcp info
KMP_MCP_DATA_DIR=/exact/destination kmp-mcp doctor
```

Wake and inspect a known about from one agent host. If multiple hosts will
share SQLite, verify the same about from each before normal work resumes.

Keep the source or pre-change snapshot until the recovered store has been
accepted.
