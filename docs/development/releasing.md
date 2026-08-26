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

The script deliberately clears the MCPB digest to a sentinel. Run
`release.yml` with `workflow_dispatch` on the version branch. That one run
builds the five binaries, four host plugin bundles and deterministic MCPB,
then seals them with one release-input digest and per-asset hashes. Download
its MCPB, stamp it and validate the registry metadata before the version
change is merged:

```bash
bash scripts/release/stamp-server-mcpb.sh path/to/kmp-mcp-vX.Y.Z.mcpb
bash scripts/ci/mcp-registry.sh
```

## Before tagging

```bash
bash scripts/ci/quality-gate.sh
bash scripts/ci/helm-lint.sh
```

Merge the reviewed version change and update local `main`. Before tagging,
mirror `plugins/kmp/` into the `underpass-ai/plugins` repository, excluding the
gitignored `bin/`, and merge that marketplace PR. Its Codex manifest must carry
the same SemVer core; build metadata may be used as a cachebuster.

This ordering matters: once GitHub exposes the new KMP release as `latest`, the
installed updater asks Codex for that version. Publishing the marketplace first
keeps the plugin, skills and launcher available before the matching engine.
The updater refreshes Git-backed marketplace snapshots and refuses to change
the engine if Codex still returns an older plugin.

Then let the release script create and push the tag:

```bash
bash scripts/release.sh release X.Y.Z
```

The release command checks the public `underpass-ai/plugins` manifest and
refuses to tag while its version is stale. It also locates the newest
successful candidate whose version and release-input digest match `main`,
verifies all twenty files and records that workflow run in the annotated tag.
If no candidate matches, tagging fails closed.

## Published artifacts

- `release.yml` builds one manual candidate; the version tag promotes those
  exact binaries, plugin packages and MCPB without compiling again;
- `plugin-package.yml` validates pull-request plugin changes and never
  publishes;
- `publish-distribution.yml` runs automatically only for a version tag and
  publishes the server image, Helm chart and crates.io dependency chain;
- `mcp-registry.yml` validates registry metadata and only publishes when the
  repository variable enables it.
- `underpass-ai/plugins` is the reviewed Codex marketplace mirror and is
  published before the tag rather than by a tag workflow.

[`scripts/ci/publish-crates.sh`](../../scripts/ci/publish-crates.sh) owns the
crate publication order and skips versions already present. If a publication
fails halfway, rerun it; never move a tag.

Contract files vendored inside publishable crates must match `api/`:

```bash
bash scripts/ci/check-vendored-contract.sh
bash scripts/ci/check-publish-chain.sh
```

Rollback means releasing a new patch that fixes or reverts the problem.
