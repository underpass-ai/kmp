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

Build and verify with the matching workspace binary:

```bash
cargo build --locked -p kmp-mcp
python3 plugins/kmp/guide/build-guide.py write --binary target/debug/kmp-mcp
python3 plugins/kmp/guide/build-guide.py check --binary target/debug/kmp-mcp
```
