# Store-lease accumulation and safety (#774)

The store-use lease is a machine-local file under
`<data-home>/kmp/store-leases`. Its name is the SHA-256 of the canonical store
path. `StoreSessionLease::acquire` creates or opens exactly that pathname and
takes a shared OS file lock. It does not enumerate the lease directory.

The current policy deliberately retains an empty lock file after the last
owner exits. A future host, including a host from an older release, can still
have an open descriptor for that pathname. Unlinking the file and later
recreating the same name would let the two generations lock different inodes,
breaking mutual exclusion. A PID or process-name check cannot establish that
the old inode is no longer owned, especially across crashes and PID reuse.

Removal takes an exclusive lock on the stable pathname first. On Linux it also
checks open descriptors under `/proc`, which covers a live host from before
the lease protocol was introduced. If either check finds a live owner, the
store remains in place. A crashed owner releases its OS lock automatically;
the empty pathname remains available for the next owner.

The startup path remembers the selected store in `known-stores.jsonl` and
filters that index against the selected store catalog. Neither that index nor
the lease adapter scans `store-leases`. The only cost of accumulated lease
files is directory storage and filesystem metadata; the selected store's
claim remains one exact open-and-lock operation.

The reproducible control in
`scripts/performance/store_lease_cardinality.py` creates synthetic directories
with 0, 1,000, and 10,000 unrelated empty lock files under `tmp/`, then
measures one first-use claim separately from 23 repeated claims for one
canonical store. It records directory metadata size, file count, source and
runner hashes, host, and toolchain in `artifacts/performance-774`.

The safety regression in `crates/kmp-mcp/tests/store_lease_safety.rs` verifies:

- shared current hosts can coexist while exclusive removal is refused;
- a live pre-lease host holding the SQLite file also blocks removal;
- a crashed lease owner releases the OS lock without PID-based cleanup;
- canonical aliases reuse one pathname and, on Unix, the same inode, while
  distinct stores get distinct files.

The evidence supports retaining lease files. It does not justify a bounded
cleanup policy that unlinks them while any current or older host could still
hold the inode.
