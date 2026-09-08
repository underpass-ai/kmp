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

Long lessons may use `text_file` instead of inline `text`, relative to this
directory. The release loader includes the file's exact Markdown in the
ordinary guide memory, so agents receive it through KMP and do not need file
access. Use one source for the body; missing, empty, ambiguous and parent or
absolute paths are rejected. The files ship with the plugin and are covered
by the release input digest.

The first worked lesson, [decision history](examples/decision-history.md),
contains explicit fictional sources, source-based writing choices, call
arguments with bindings to actual returned refs, temporal reads and visual
inspection. Its JSON envelopes are teaching notation: the replay resolves
bindings and sends only `arguments` to the named public MCP tool.

Run it against a fresh temporary store under this repository's `tmp/`:

```bash
python3 scripts/guide_examples/replay.py --binary target/debug/kmp-mcp \
  --trace artifacts/guide-decision-history.jsonl \
  --result artifacts/guide-decision-history.json
```

Add `--hold-view` to review ChronoLoom before typing `quit`; the temporary
store is removed when the replay exits. The trace contains real MCP requests
and responses. Assertions check typed memory, relation direction and proof,
historical validity, inclusive/exclusive traversal and view state. No model
is invoked. This is an authored teaching case and contract check; measuring
LLM learning requires a separate unseen history and blind reader questions.

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
