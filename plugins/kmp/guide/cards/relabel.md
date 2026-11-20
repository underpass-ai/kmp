`kmp_relabel` changes the labels of one stored memory without rewriting it.
Name the `about`, the memory `ref`, the `observed_at` of the change, a `why`,
and `add` and/or `remove` as `{key:[values]}`. Omit `actor` to be recorded
under the MCP client's name.

```json
{"about":"project:sample","ref":"<returned-ref>","observed_at":"2026-09-02T10:00:00Z","why":"R2 assigns the failure to the storage team.","add":{"team":["storage"]}}
```

The change is dated: a read at an earlier position does not see a label added
later. Text, evidence, clocks and relations stay as they were. Reuse an
existing key/value from the wake catalogue; `options` confirms a new resembling
label. Replay with the same `idempotency_key`.

More: `guide:kmp-agent:example:dimensional-memberships`.
