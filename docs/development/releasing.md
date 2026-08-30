# Releasing

KMP uses semantic versions. A `vX.Y.Z` tag triggers the release and publishing
workflows; released tags and registry artifacts are immutable.

## Preflight

Every gate in the release path fails closed, which is right, but each one runs
only at the step that needs it — so a tree with two problems reports them one
release attempt at a time, and a release attempt costs a fifteen-minute
candidate build. Preflight answers everything knowable from the tree at once,
without building anything:

```bash
bash scripts/release.sh preflight X.Y.Z
```

It checks the changelog section, that every version source listed below reads
`X.Y.Z`, that both marketplace catalogs describe the reviewed tree, that the
working tree is clean and the branch is pushed, and that the vendored contract
and publish chain gates pass. It reports every failure instead of the first, and
prints the candidate input digest a candidate has to be bound to.

`candidate` and `release` run it before doing anything expensive, so a build
cannot start against a tree that could never be tagged.

## Version sources

Keep these in lockstep with the release script:

- workspace and internal crate dependency versions in `Cargo.toml`;
- a non-empty version section promoted from `[Unreleased]` in `CHANGELOG.md`;
- the marketplace's marked public overview synchronized into the GitHub and
  crates.io READMEs;
- Helm `version` and `appVersion`;
- Codex and Claude plugin manifests;
- `server.json` and the MCPB manifest;
- the `ref` in `.claude-plugin/marketplace.json`, which pins the immutable tag
  Claude Code clones.

```bash
bash scripts/release.sh version X.Y.Z
```

The script first turns the reviewed `[Unreleased]` entries into a dated
`[X.Y.Z]` section and refuses to bump anything when those notes are empty.
It also copies the marked public overview from `plugins/kmp/README.md` into
`README.md` and `crates/kmp-mcp/README.md`; surface-specific sections stay
separate. It deliberately clears the MCPB digest to a sentinel.

The Claude catalog `ref` moves here, with the manifests, and not later. It is
one of the candidate's input files, so bumping it after a candidate is built
necessarily invalidates that candidate: the tag must bind a candidate built
from the tree it is tagging. A catalog fixed at tagging time costs two
fifteen-minute builds.

Commit and push the version branch, then let the release helper dispatch and
watch `release.yml`, download and verify its twenty-file candidate, stamp the
exact MCPB digest into `server.json`, and validate the registry metadata:

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

Merge the reviewed version change and update local `main`. KMP owns both
marketplace catalogs in the same repository: `.agents/plugins/marketplace.json`
points Codex at `plugins/kmp` and carries no ref, while
`.claude-plugin/marketplace.json` points Claude Code at the annotated release
tag and the same subdirectory. Its `ref` already reads `vX.Y.Z` — the version
change owns it — so what happens here is verification, not a second bump: the
marketplace check proves that the future tag dereferences to the reviewed KMP
commit and that both hosts resolve byte-identical plugin trees.

This ordering keeps every advertised version installable. The public
`marketplace` branch continues to serve the previous release while the new
annotated tag and its checksummed assets become available. Publishing the catalog first would
make Claude Code run `git clone --branch` against a tag that does not exist and
would make the updater request engine assets that are not public yet.

Let the release script create and push the tag, binding the reviewed candidate
run into its provenance:

```bash
bash scripts/release.sh release X.Y.Z
```

The release command verifies both co-located catalogs and refuses a tag whose
tree differs from the reviewed snapshot. It also locates the newest successful
candidate whose version and release-input digest match `main`, verifies all
twenty files and records the reviewed candidate in the annotated tag. If no
candidate matches, tagging fails closed and names the release inputs that moved
since that candidate was built.

Wait until the GitHub release and its checksummed engine and plugin assets are
public. Only then advance the protected `marketplace` branch to that exact
release commit. Its check repeats Claude's literal `git clone --branch vX.Y.Z`
operation against the now-public annotated tag and verifies Codex/Claude tree
parity before the branch can move.

## Published artifacts

- `release.yml` builds one manual candidate; the version tag promotes those
  exact binaries, plugin packages and MCPB without compiling again;
- `plugin-package.yml` validates pull-request plugin changes and never
  publishes;
- `publish-distribution.yml` runs automatically only for a version tag and
  publishes the server image, Helm chart and crates.io dependency chain;
- `mcp-registry.yml` validates registry metadata and only publishes when the
  repository variable enables it.
- KMP's protected `marketplace` branch is the single public catalog for Codex
  and Claude Code and advances only after the matching tag and assets exist.

[`scripts/ci/publish-crates.sh`](../../scripts/ci/publish-crates.sh) owns the
crate publication order and skips versions already present. If a publication
fails halfway, rerun it; never move a tag.

Contract files vendored inside publishable crates must match `api/`:

```bash
bash scripts/ci/check-vendored-contract.sh
bash scripts/ci/check-publish-chain.sh
```

Rollback means releasing a new patch that fixes or reverts the problem.
