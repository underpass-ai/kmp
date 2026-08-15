# kmp-embedded

The in-process edition of [KMP by Underpass](https://github.com/underpass-ai/kmp),
the Kernel Memory Protocol kernel.

The composition root: it wires the application services over the redb store
with synchronous in-process projection and resolves the data directory. No
transport, no infrastructure clients, no cluster — the same kernel semantics
the deployed service exposes over gRPC, running inside your process.

```rust
use kmp_embedded::{EmbeddedKernel, resolve_data_dir_from_env};

// Data directory from KMP_MCP_DATA_DIR, or an explicit path.
let dir = resolve_data_dir_from_env()?;
let kernel = EmbeddedKernel::open(dir.path())?;
```

Projection is synchronous on purpose: when a write returns, what it
materialized is already readable. There is no queue to drain and no eventual
window where memory disagrees with itself.

The store is single-writer, so one process owns one data directory at a time.
That is why anything that wants to watch a live session — the
[viewer](https://crates.io/crates/kmp-viewer), for instance — mounts inside
the same process instead of opening the file a second time.

For the consumer-facing surface, see
[`kmp-memory-api`](https://crates.io/crates/kmp-memory-api). For an MCP server
over this kernel, see [`kmp-mcp`](https://crates.io/crates/kmp-mcp).

## License

Apache-2.0.
