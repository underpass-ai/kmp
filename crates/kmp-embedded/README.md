# kmp-embedded

[KMP](https://github.com/underpass-ai/kmp) is local-first agent memory that
preserves what happened, when and why. This crate is its in-process edition.

The composition root wires application services over a stamped local store
with synchronous in-process projection and resolves the data directory. No
transport, no infrastructure clients, no cluster — the same kernel semantics
the deployed service exposes over gRPC, running inside your process.

```rust
use kmp_embedded::{
    EmbeddedKernel, default_engine_for_data_dir, resolve_data_dir_from_env,
};

// Data directory from KMP_MCP_DATA_DIR, or an explicit path.
let dir = resolve_data_dir_from_env()?;
let engine = default_engine_for_data_dir(dir.path());
let kernel = EmbeddedKernel::open_with_engine(dir.path(), engine)?;
```

Fresh stores always use shareable SQLite. The `sqlite` feature name remains
as a compatibility alias for downstream manifests. An existing store opens
from its `FORMAT_VERSION`; format-1 redb memory remains readable only for the
legacy migration promise. KMP never guesses or silently changes that engine.

Projection is synchronous on purpose: when a write returns, what it
materialized is already readable. There is no queue to drain and no eventual
window where memory disagrees with itself.

SQLite stores support multiple agent processes. The
[viewer](https://crates.io/crates/kmp-viewer) still mounts in-process so it
shares the exact live kernel and adds no daemon, network hop or second read
model.

For the consumer-facing surface, see
[`kmp-memory-api`](https://crates.io/crates/kmp-memory-api). For an MCP server
over this kernel, see [`kmp-mcp`](https://crates.io/crates/kmp-mcp).

## License

Apache-2.0.
