# KMP shipped guides

KMP ships two deliberately different memories:

- `guide:kmp-agent` is an operating guide for the agent. Its editorial entries
  explain when to choose each verb, when not to, the minimum input, the
  expected result and the usual next move. Its tool-reference entries are
  generated from the live `tools/list` surface.
- `guide:kmp` is the shorter human story. `/kmp:guide` opens it visually in
  ChronoLoom through the `open:guide` intent.

Both are derived from `editorial.json`, carry stable refs and use
content-derived idempotency keys. An exact sync is a no-op. A changed guide
gets a new logical key and updates those stable refs through ordinary
`kmp_ingest`.

`memory.jsonl` is a regular format-2 bundle for an empty first install.
Existing stores use the exact same requests through the public MCP writer; the
bundle loader remains restore-only.

Build the assets with the matching workspace binary. Runtime behavior and the
shipped bundle are covered by focused crate tests:

```bash
cargo build --locked -p kmp-mcp
cargo run --locked --quiet -p kmp-release -- guide assets write --binary target/debug/kmp-mcp
cargo test --locked -p kmp-adapter-embedded --test guide_bundle
cargo test --locked -p kmp-mcp --test guide_sync
```
