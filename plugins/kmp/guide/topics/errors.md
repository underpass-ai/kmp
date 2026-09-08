## When the tools are not there

If the kmp tools are missing from your inventory, do not silently
fall back to re-deriving everything — say so. The usual causes are specific
and fixable, and `/kmp:doctor` distinguishes them:

- the `kmp-mcp` binary is not installed or not on `PATH`;
- the session started before the binary, plugin, or MCP registration changed,
  so it is still carrying the old tool inventory — restart the session;
- the configured binary or data directory no longer exists — run
  `/kmp:doctor` to identify the stale path.

## Errors

Tool failures set `isError=true` and carry
`structuredContent.error.{code,message}`. Read the code; it is produced where
the failure happened, never inferred from the words, and `tools/list` carries
the closed set under `_meta."kmp/errorCodes"` with what each one means.

| code | what to do |
| --- | --- |
| `invalid_argument` | fix the arguments — retrying unchanged cannot work |
| `not_found` | the requested about or ref was not found; check the exact address and selected store before inferring absence |
| `conflict` | inspect the message: retryable concurrency conflicts are safe to replay with the same key; a key accepted with different content must not be reused |
| `unavailable` | the kernel was unreachable; the same call may work later |
| `unknown_tool` | no such tool here |
| `backend_error` | the kernel failed for a reason no argument can fix |

An unknown argument is refused rather than dropped: every tool declares
`additionalProperties: false` and the boundary enforces it, so a misspelling
comes back naming the key instead of being answered from defaults.
