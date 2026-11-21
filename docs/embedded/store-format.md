# Store format

What is on disk, which versions open it, and how to go back.

## The data directory

`kmp-mcp info` names the data directory it selected and the rule that chose
it. Inside it:

| Path | What | Safe to delete? |
|:--|:--|:--|
| `FORMAT_VERSION` | The format stamp: `4`. | No. |
| `store/kernel.sqlite3` (and `-wal`, `-shm`) | All memory: the event log and its projections. SQLite in WAL mode. | **No.** This is the memory. |
| `lexical-index.sqlite3` | Search index derived from the store. | Yes. The next ask rebuilds what it needs. |
| `continuations.sqlite3` | Retained reads behind continuation handles, kept 24 hours. | Yes. Old handles then answer `CONTINUATION_UNAVAILABLE`. |
| `judgements.sqlite3` | Jev verdict book (only with a Jev opt-in). | Yes. Verdicts are asked again. |
| `telemetry/`, `logs/`, `telemetry-salt` | Local diagnostics. | Yes. Deleting the salt makes new fingerprints incomparable with old ones. |
| `*.json`, `lexical-bridge.kmpb` | Optional configuration ([reference](configuration.md#optional-files-beside-the-store)). | Yes; the default applies. |

A project store is `.kernel/` at the project root and is ignored by Git. Its
portable copy is `.kmp/memory.jsonl`, which you may commit.

## Formats

| Format | Engine | Written by | Opened by 1.0 |
|:--|:--|:--|:--|
| 1 | redb | 0.3.x and earlier | **No.** Refused, bytes untouched. |
| 2 | SQLite, old label identity | 0.2.0 – 0.14.x | **No.** Refused, bytes untouched. 0.15.0 changed the identity of labels and shipped no migration. |
| 3 | SQLite | 0.15.0 – 0.18.7 | **Yes, upgraded to 4 on first open.** |
| 4 | SQLite with card history | 0.18.8 and later, 1.0 | Yes. |

No tested path brings a format-1 or format-2 store into 1.0. Bundles exported
by those releases use event format 1, which 1.0 does not read.

A format newer than the binary is refused with "upgrade the binary".

**The 1.x promise:** every 1.x release opens a format-4 store written by 1.0.

## Upgrading from format 3

Opening a format-3 store with a current engine rewrites its stamp to 4 and
adopts existing reader cards as baseline events. It is **one way**: older
engines then refuse the store.

Before you open it:

1. Stop every older KMP process that uses the store.
2. Take a copy you can go back to:
   - any store: `kmp-mcp export /path/before-upgrade.jsonl`;
   - a project store: `kmp-mcp snapshot create before-upgrade` also works.
3. Open it with the new engine (`kmp-mcp doctor` does not upgrade; the first
   session or command that opens the store for use does).

## Rolling back

There is no downgrade of a store. To go back to an older engine:

1. Stop the new engine.
2. Move the data directory aside (for example `mv .kernel .kernel.new`).
3. With the older engine, import the bundle you exported before the upgrade
   into the empty store: `kmp-mcp import /path/before-upgrade.jsonl`.

Anything written after the upgrade is only in the new store. Bring it across
with `kmp-mcp import --from <store-dir> --about <about>` once you upgrade
again.

## Bundles

A bundle is one JSON line of header, then one line per event.

| Field | Value at 1.0 |
|:--|:--|
| `bundle_format` | `3` |
| `event_format` | `2` for memory only; `3` when the bundle carries reader-card events |

Every 1.x reads both event formats. A bundle carries the event log of the
abouts it names: memories, relations, labels, reader cards. It does **not**
carry the derived index, verdicts, continuations, logs, telemetry or the
salt, and `kmp-mcp demo`'s example never enters a project's committed bundle.

| Command | Use |
|:--|:--|
| `kmp-mcp export [file] [--about <about>]...` | Write a bundle: the whole log, or exact abouts. With no file in a project, the committed `.kmp/memory.jsonl`. |
| `kmp-mcp import <file>` | Restore a bundle into an **empty** store. |
| `kmp-mcp import --from <store-dir\|bundle> --about <about>...` | Bring exact abouts into a store that already holds memory: each is imported, left unchanged, or extended when this store holds an exact prefix. A different history refuses the whole import and writes nothing. |

## Several processes, one store

SQLite lets several local hosts share one store. Writers on the same about
are serialised: a write that lost the race is re-planned and retried by the
server (up to 12 attempts) and answers `retryable_conflict` only when every
attempt lost. A project store's write waits up to 10 s for another session
that is publishing the committed bundle.
