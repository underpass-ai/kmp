# Preserve argument shapes across conditional call forms

Codex metadata captured on 15 September 2026 exposes thirteen KMP inputs as
`unknown` or unions of generic dictionaries. The native catalogue still
contains their named properties. Root `oneOf`, `anyOf` and an unnecessary
`allOf` wrapper let a host render only constraint branches, losing the shared
argument shape. Complete captures are retained in
`artifacts/agent-surface-544-20260915/`. They describe one running host,
not billed context.

## Change

Keep the shared object and properties at the schema root. Express the existing
call alternatives with conditionals:

- With `continuation`, require the same valid handle and exactly one property.
  Otherwise apply all the original initial-call requirements.
- Without `actor`, require `context_id`. Supplying both remains valid.
- Canonical ingest retains its provenance condition directly. Empty entry
  arrays still require at least one relation.
- Condense still accepts exactly one of `expect.absent` and
  `expect.card_revision`.

Runtime parsers and output schemas are unchanged. Named properties retain
their types and descriptions. JSON Schema evaluates the conditions; a host's
optional TypeScript field display cannot replace validation. The routing
guide explains initial arguments and handle-only calls.

For continuations, the old schema is `(not H and I) or (H and M)`, where H is
presence of the handle, I is the initial-call constraints and M limits the
object to one property. The new `if H then M else I` has the same acceptance
set. Common type, property and additional-property validation still applies to
both branches. Presence conditions likewise preserve actor/context and
Condense exclusivity. Ingest removes a one-element intersection and expresses
its empty-entry condition directly.

## Evidence

The informational comparator validates 4,623 accepted/rejected inputs using
`jsonschema 4.25.1` (Draft 2020-12), including writer variants, missing fields,
actor/context combinations, handle mixtures, ingest provenance and Condense CAS.
It checks preserved property shapes/descriptions and exact output schemas.
Native captures equal the before/after reviewed fixtures. Existing native
tests cover short/full actions, every retained read family, stale handles,
restart, mixed-argument refusal, writes and complete returned proof. Surface
parity passes without changing response fixtures. No size or editorial CI
gate is added.

Separately serialized compact results, with `tiktoken 0.12.0` / `o200k_base`:

| Representation | Before | After |
| --- | ---: | ---: |
| Native model catalogue bytes | 247,138 | 246,956 |
| Native model catalogue tokens | 51,284 | 51,225 |
| Captured host metadata tokens | 31,223 | Not refreshed |

The small native difference is incidental. Restoring fields hidden by a host
may **increase** its displayed context. This change keeps fields discoverable
in a flat object; a new installed-host capture is still needed to verify that
host's renderer. Compilation does not refresh a running Codex session. No
installation, release or billing saving is claimed.

`schema-comparison.json` accounts separately for complete initialization,
catalogues with/without MCP Apps, resource discovery and the App response,
including JSON-RPC envelopes. The unchanged App resource alone is 983,795
response bytes / 316,594 reference tokens. This is an explicitly requested
App resource, not evidence that a model receives that HTML. Complete native
captures and binary hashes accompany the report. The reference catalogue
matches fresh main `7f783e05`; the candidate matches this change's fixture.

## Reproduce

Capture a built binary without touching personal memory:

```bash
python3 scripts/performance/agent_surface_capture.py \
  --binary /absolute/path/kmp-mcp --output tmp/capture.json.gz
```

Compare the retained baseline with the reviewed current catalogue:

```bash
uv run --with tiktoken==0.12.0 --with jsonschema==4.25.1 python \
  scripts/performance/agent_surface_compare.py \
  --before artifacts/agent-surface-544-20260915/baseline-tools.json.gz \
  --after crates/kmp-mcp/fixtures/contract/tools_list.json \
  --host artifacts/agent-surface-544-20260915/host-capture.json.gz \
  --before-native artifacts/agent-surface-544-20260915/native-before.json.gz \
  --after-native artifacts/agent-surface-544-20260915/native-after.json.gz \
  --output tmp/schema-comparison.json
```

This runs no model. Broader #544 acceptance still needs a refreshed-host
observation and complete independent workflow evaluation. Response reductions
and evidence-selection quality remain separately measured work.
