# E2E Version Preflight

This checklist prevents replay or deployment validation from using stale local
binaries or drifted cluster configuration.

Run the repository-local automation before any live E2E, replay, Helm, or KMP
validation:

```bash
./scripts/e2e/regen.sh
```

Default mode is read-only except for local `cargo build` / `cargo install` steps
that refresh user-local binaries. Mutating operations are gated behind explicit
flags and existing deployment runbooks.

The preflight verifies, where applicable:

- git branch, HEAD, and clean working tree state;
- freshly built local CLIs are newer than the source HEAD;
- mTLS certs and API key files are present;
- live Helm releases and Kubernetes pods match expected configuration;
- vLLM endpoints list the expected model ids;
- KMP/MCP smoke calls work against a known real anchor;
- vLLM profiles render without deprecated flags such as
  `--guided-decoding-backend=xgrammar`.

If any check fails, stop and fix the reported issue before running E2E replay or
cluster operations. E2E artifacts must use timestamped output directories and
must not overwrite previous evidence.
