# Pre-0.5.0 machine-history fixture

This is the final portable `project:kmp` bundle exported from the development
store used on this machine across the pre-0.5.0 product phases. It is archived
as a regression fixture rather than imported into the clean 0.5.0 project
store.

- Events: 149
- KMP content digest:
  `sha256:777ee3fc88030d5e79a98e711c92cbe09336052dd85cc09b0b7e6b47a6b25aad`
- File SHA-256:
  `ab1300b9254581b25f67650d648d2b445fa15adac65eb59e3862edd78df9b57f`

Restore only into an empty test data directory:

```bash
KMP_MCP_DATA_DIR=/absolute/path/to/empty-test-store \
  kmp-mcp import project-kmp-memory.jsonl
```
