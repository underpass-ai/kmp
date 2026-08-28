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

Stop agent sessions using that store. This is mandatory for a legacy redb
store and for a pending bundle-recovery condition.

## 3. Export and verify evidence

For a project store:

```bash
kmp-mcp export
git diff -- .kmp/memory.jsonl
```

If doctor reports a pending export after an interrupted commit, first stop all
writers, run the normal export and inspect its diff. Only then acknowledge the
recovery:

```bash
kmp-mcp export --repair-pending
```

For a recovery point before a risky operation:

```bash
kmp-mcp snapshot create pre-change
kmp-mcp snapshot verify pre-change
```

## 4A. Bridge an existing format-1 redb store to SQLite

```bash
cargo install kmp-mcp --version 0.3.2 --locked --root /tmp/kmp-0.3.2
KMP_MCP_DATA_DIR=/exact/source /tmp/kmp-0.3.2/bin/kmp-mcp export /safe/memory.jsonl
KMP_MCP_DATA_DIR=/exact/fresh-destination kmp-mcp import /safe/memory.jsonl
```

Current KMP deliberately contains no redb reader. Version 0.3.2 is the last
bridge: it opens the legacy source and writes the engine-independent event
bundle; the current binary restores that bundle into SQLite. Keep the source
untouched. Point one test session at the destination, verify it, then rerun
`kmp-mcp info` from every intended host and confirm that all resolve to the
same SQLite data directory.

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

Import is restore, not merge. It must refuse a non-empty destination.

## 5. Verify before resuming

```bash
KMP_MCP_DATA_DIR=/exact/destination kmp-mcp info
KMP_MCP_DATA_DIR=/exact/destination kmp-mcp doctor
```

Wake and inspect a known about from one agent host. If multiple hosts will
share SQLite, verify the same about from each before normal work resumes.

Keep the source or pre-change snapshot until the recovered store has been
accepted.
