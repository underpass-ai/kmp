# Running E2E tests

Use `./scripts/e2e/regen.sh` before any live E2E, replay validation, or
infra-touching run. The script is the executable preflight: it checks git state,
refreshes local binaries when relevant, verifies cluster/model reachability, and
fails fast on stale or drifted configuration. Deployment authority is indexed
in [Docker and Kubernetes operations](../operations/deployment/README.md).

Default mode is read-only except for user-local `cargo install` steps in Rust
repos. Cluster mutations require explicit runbooks and must not be inferred from
this preflight.

Example output:

```text
[OK  ] git state                            clean tree at <sha> <subject>
[OK  ] cargo release build                  workspace release build completed
[OK  ] binary freshness                     local binary is newer than or equal to HEAD
[WARN] values drift <release>               inspect /tmp/<release>-values.diff
12/13 checks passed (1 warn, 0 fail)
```

Exit codes:

- `0`: all checks passed;
- `1`: at least one check failed;
- `2`: warnings only, no failures.
