# Releasing

KMP uses semantic versions. A `vX.Y.Z` tag triggers the release and publishing
workflows; released tags and registry artifacts are immutable.

## Version sources

Keep these in lockstep with the release script:

- workspace and internal crate dependency versions in `Cargo.toml`;
- Helm `version` and `appVersion`;
- Codex and Claude plugin manifests;
- `server.json` and the MCPB manifest.

```bash
bash scripts/release.sh version X.Y.Z
```

The script deliberately clears the MCPB digest to a sentinel. Build the MCPB
through `release.yml`, then stamp and validate the real artifact before the
version change is merged:

```bash
bash scripts/release/stamp-server-mcpb.sh path/to/kmp-mcp-vX.Y.Z.mcpb
bash scripts/ci/mcp-registry.sh
```

## Before tagging

```bash
bash scripts/ci/quality-gate.sh
bash scripts/ci/helm-lint.sh
```

Merge the reviewed version change, update local `main`, then let the release
script create and push the tag:

```bash
bash scripts/release.sh release X.Y.Z
```

## Published artifacts

- `release.yml` builds checksummed `kmp-mcp` binaries and the deterministic
  multiplatform MCPB;
- `plugin-package.yml` builds host plugin packages;
- `publish-distribution.yml` publishes the server image, Helm chart and the
  crates.io dependency chain;
- `mcp-registry.yml` validates registry metadata and only publishes when the
  repository variable enables it.

[`scripts/ci/publish-crates.sh`](../../scripts/ci/publish-crates.sh) owns the
crate publication order and skips versions already present. If a publication
fails halfway, rerun it; never move a tag.

Contract files vendored inside publishable crates must match `api/`:

```bash
bash scripts/ci/check-vendored-contract.sh
bash scripts/ci/check-publish-chain.sh
```

Rollback means releasing a new patch that fixes or reverts the problem.
