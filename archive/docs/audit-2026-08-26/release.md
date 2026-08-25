# Release process

A release is a `v*` tag pushed to this repository. Everything else follows
from it:

| Workflow | What it publishes on a tag |
|:--|:--|
| `publish-distribution.yml` | `ghcr.io/underpass-ai/kmp:vX.Y.Z`, `oci://ghcr.io/underpass-ai/charts/kmp:X.Y.Z`, and the crates.io chain |
| `plugin-package.yml` | the Codex / Claude Code plugin bundles for linux-x86_64, linux-arm64, macos-arm64 and windows-x86_64, with checksums |
| `release.yml` | `kmp-mcp` binaries for five host targets plus one checksummed, multiplatform MCPB |
| `mcp-registry.yml` | validates `server.json`; publishes it only on a tag when `MCP_REGISTRY_PUBLISH=true` |

## Versioning

Semver. Five things must stay in lockstep, and one script keeps them there:

- `Cargo.toml` → `[workspace.package].version`
- `Cargo.toml` → the `version` next to each internal crate's `path`
- `distribution/charts/kmp/Chart.yaml` → `version` + `appVersion`
- both plugin host manifests
- `server.json` and `distribution/mcpb/manifest.json`

```bash
bash scripts/release.sh version 0.2.0
```

The bump replaces the old MCPB hash with an all-zero sentinel. That sentinel
cannot pass the registry gate. Push the version branch, run `release.yml` with
`workflow_dispatch`, download its `kmp-mcpb-X.Y.Z` artifact and stamp the
actual deterministic bundle before opening or merging the version PR:

```bash
bash scripts/release/stamp-server-mcpb.sh path/to/kmp-mcp-v0.2.0.mcpb
bash scripts/ci/mcp-registry.sh
```

The internal pins are the part that is easy to forget and expensive to get
wrong: cargo cannot inherit a version into a path dependency, and a crate
published while its siblings still require the previous release does not
resolve on crates.io.

## Checklist

1. **Sync main** — releases come off reviewed history:
   ```bash
   git checkout main && git pull --ff-only
   ```
2. **Bump versions** and review the diff:
   ```bash
   bash scripts/release.sh version 0.2.0
   git diff
   ```
3. **Build and stamp the MCPB** using the workflow-dispatch sequence above.
4. **Gates green locally**:
   ```bash
   bash scripts/ci/quality-gate.sh   # contract + vendored copies + fmt + clippy + tests
   bash scripts/ci/helm-lint.sh
   ```
5. **Commit and merge**:
   ```bash
   git commit -am "chore: v0.2.0"
   gh pr create --fill
   # wait for CI green; merge
   ```
6. **Tag from merged main**:
   ```bash
   git checkout main && git pull --ff-only
   bash scripts/release.sh release 0.2.0
   ```
7. **Watch the publish**:
   ```bash
   gh run watch $(gh run list --workflow publish-distribution.yml --json databaseId -q '.[0].databaseId')
   ```

The official Registry listing is intentionally independent from creating the
release artifacts. Namespace `io.github.underpass-ai/kmp` is permanent, Cargo
ownership comes from the visible `mcp-name:` line in the published crate
README, and the MCPB hash is checked against the GitHub Release before any
publish attempt. Keep the repository variable `MCP_REGISTRY_PUBLISH` unset or
false while the public listing is held. Setting it to `true` arms OIDC publish
for subsequent `v*` tags; it does not retroactively submit an older tag.

## What reaches crates.io

`kmp-mcp` is installable from the registry, and it carries the embedded
kernel, so cargo requires its whole chain to be there too. The release
publishes, in this order:

```
kmp-plugin-api  kmp-domain  kmp-ports  kmp-observability  kmp-memory-api
kmp-adapter-embedded  kmp-application  kmp-embedded  kmp-proto
kmp-proto-mapping  kmp-viewer  kmp-mcp
```

`scripts/ci/publish-crates.sh` owns that order;
`scripts/ci/check-publish-chain.sh` fails a pull request that breaks it.
Everything outside the chain — the server, its transport, the deployed
adapters, the test crates — is `publish = false` in its own manifest.

Three operational notes:

- The step is **idempotent**. Versions already on the registry are skipped,
  so a release that failed halfway is resumed by re-running the job — never
  by moving the tag, which is not a thing we do.
- crates.io throttles **new crate names** to a burst of five and then one
  every ten minutes. A first release introduces twelve, so the job sits
  waiting for over an hour rather than failing; its timeout is sized for
  that. Later releases publish new *versions* of existing crates, which is
  a far looser limit, and take minutes.
- It needs the repository secret `CARGO_REGISTRY_TOKEN`, on an account with
  a **verified email**. crates.io rejects the upload otherwise, and the
  error arrives at publish time, not at token creation time.

## Two copies of the contract

`kmp-proto` compiles the kernel `.proto` files from `crates/kmp-proto/proto`,
and `kmp-mcp` embeds its fixture responses from `crates/kmp-mcp/fixtures` —
both vendored copies of what lives under `api/`. This is not a preference:
`cargo publish` packages only what is inside the crate directory, so a crate
reading `../../api` builds here and fails for everyone who installs it.

`scripts/ci/check-vendored-contract.sh` diffs the copies against `api/` on
every pull request. When the contract changes, copy it over and let the gate
confirm they agree.

## Hotfix flow

Same checklist, from a branch off the tag you want to fix:

```bash
git checkout -b hotfix/v0.2.1 v0.2.0
# ... fix ...
bash scripts/release.sh version 0.2.1
git commit -am "chore: v0.2.1"
gh pr create --fill
# merge; then from main:
bash scripts/release.sh release 0.2.1
```

## Rolling back

Do **not** delete or move a tag, and do not expect to unpublish a crate:
released versions are immutable, on ghcr and on crates.io alike. Cut a new
patch version that fixes or reverts the commit and release again.
