# Releasing

KMP uses semantic versions. A `vX.Y.Z` tag triggers the release and publishing
workflows; released tags and registry artifacts are immutable.

## Version sources

Keep these in lockstep with the release script:

- workspace and internal crate dependency versions in `Cargo.toml`;
- a non-empty version section promoted from `[Unreleased]` in `CHANGELOG.md`;
- the marketplace's marked public overview synchronized into the GitHub and
  crates.io READMEs;
- Helm `version` and `appVersion`;
- Codex and Claude plugin manifests;
- `server.json` and the MCPB manifest.

```bash
bash scripts/release.sh version X.Y.Z
```

The script first turns the reviewed `[Unreleased]` entries into a dated
`[X.Y.Z]` section and refuses to bump anything when those notes are empty.
It also copies the marked public overview from `plugins/kmp/README.md` into
`README.md` and `crates/kmp-mcp/README.md`; surface-specific sections stay
separate. It deliberately clears the MCPB digest to a sentinel. Commit and push
the version branch, then let the release helper dispatch and watch
`release.yml`, download and verify its twenty-file candidate, stamp the exact
MCPB digest into `server.json`, and validate the registry metadata:

```bash
bash scripts/release.sh candidate X.Y.Z
git add server.json
git commit -m "chore(release): seal X.Y.Z MCPB"
git push
```

Pass an existing workflow run ID as the optional second argument to resume a
candidate whose build was already dispatched. The helper fails closed when the
working tree is dirty, the branch was not pushed, the candidate inputs differ,
any of its twenty files is invalid, or the resulting Registry metadata fails.
There is no separate digest-stamping step to remember.

## Before tagging

```bash
bash scripts/ci/quality-gate.sh
bash scripts/ci/helm-lint.sh
```

Merge the reviewed version change and update local `main`. Mirror
`plugins/kmp/` into a PR in `underpass-ai/plugins`, excluding the gitignored
`bin/`, but keep that PR unmerged. Its Codex manifest must carry the same SemVer
core; build metadata may be used as a cachebuster. The marketplace check proves
the unpublished tag would name KMP `main` and that the Codex mirror is the same
tree. Record the green marketplace PR's exact commit.

This ordering keeps every advertised version installable. Public marketplace
`main` continues to serve the previous release while the new annotated tag and
its checksummed assets become available. Publishing the catalog first would
make Claude Code run `git clone --branch` against a tag that does not exist and
would make the updater request engine assets that are not public yet.

Let the release script create and push the tag, binding the reviewed marketplace
commit into its provenance:

```bash
bash scripts/release.sh release X.Y.Z MARKETPLACE_COMMIT
```

The release command checks that commit's successful `kmp-contract`, verifies
its catalogs and both plugin trees, and refuses to use a different snapshot.
It also locates the newest successful candidate whose version and release-input
digest match `main`, verifies all twenty files and records both reviewed commits
in the annotated tag. If no candidate matches, tagging fails closed.

Wait until the GitHub release and its checksummed engine and plugin assets are
public. Only then merge the already-reviewed marketplace PR. The marketplace
`main` check repeats the literal Claude `git clone --branch vX.Y.Z` operation
against the now-published annotated tag.

## Published artifacts

- `release.yml` builds one manual candidate; the version tag promotes those
  exact binaries, plugin packages and MCPB without compiling again;
- `plugin-package.yml` validates pull-request plugin changes and never
  publishes;
- `publish-distribution.yml` runs automatically only for a version tag and
  publishes the server image, Helm chart and crates.io dependency chain;
- `mcp-registry.yml` validates registry metadata and only publishes when the
  repository variable enables it.
- `underpass-ai/plugins` is the reviewed Codex marketplace mirror and is made
  public only after the tag and matching release assets are reachable.

[`scripts/ci/publish-crates.sh`](../../scripts/ci/publish-crates.sh) owns the
crate publication order and skips versions already present. If a publication
fails halfway, rerun it; never move a tag.

Contract files vendored inside publishable crates must match `api/`:

```bash
bash scripts/ci/check-vendored-contract.sh
bash scripts/ci/check-publish-chain.sh
```

Rollback means releasing a new patch that fixes or reverts the problem.
