#!/usr/bin/env bash
set -euo pipefail

# Release helper for KMP.
#
# Two verbs:
#   version <X.Y.Z>   — rewrite every versioned artefact in the repo so
#                       Cargo, Helm, plugin and MCP Registry metadata stay in
#                       lockstep. Resets the MCPB hash until a matching bundle
#                       is built and stamped; idempotent and safe to re-run.
#
#   release <X.Y.Z>   — verify the tree is clean, versions already point at
#                       X.Y.Z and a successful workflow_dispatch candidate
#                       matches the release inputs, then create an annotated
#                       `vX.Y.Z` tag naming that candidate. The tag promotes
#                       its exact bytes and starts tag-only distribution.
#
# Typical flow:
#   bash scripts/release.sh version 0.2.0
#   bash scripts/ci/quality-gate.sh
#   git commit -am "chore: v0.2.0" && gh pr create --fill
#   # merge via CI
#   git checkout main && git pull
#   bash scripts/release.sh release 0.2.0

usage() {
    cat <<'USAGE' >&2
release.sh version <X.Y.Z>
release.sh release <X.Y.Z>
USAGE
    exit 2
}

semver_check() {
    local version="$1"
    if ! echo "${version}" | grep -Eq '^[0-9]+\.[0-9]+\.[0-9]+(-[A-Za-z0-9.-]+)?$'; then
        echo "error: version '${version}' is not valid semver" >&2
        exit 1
    fi
}

cmd_version() {
    local version="$1"
    semver_check "${version}"

    local root
    root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
    cd "${root}"

    python3 - "${version}" <<'PY'
import json, pathlib, re, sys

version = sys.argv[1]

# Cargo.toml: the [workspace.package] version (first occurrence only, so
# dependency requirements further down are not caught by this pattern).
cargo = pathlib.Path("Cargo.toml")
text = cargo.read_text()
new_text, count = re.subn(
    r'(^version = )"[^"]+"',
    rf'\1"{version}"',
    text,
    count=1,
    flags=re.MULTILINE,
)
if count == 0:
    sys.exit("Cargo.toml: no workspace version line matched")

# Internal path dependencies carry a literal version next to their path —
# cargo has no way to inherit it — and a published crate whose sibling
# requirement still points at the previous release cannot resolve on
# crates.io. They move with the workspace or the release is broken.
new_text, pinned = re.subn(
    r'(^kmp-[a-z-]+ = \{ path = "crates/[^"]+", version = )"[^"]+"',
    rf'\1"{version}"',
    new_text,
    flags=re.MULTILINE,
)
if pinned == 0:
    sys.exit("Cargo.toml: no internal dependency pins matched")
cargo.write_text(new_text)

# Chart.yaml: both `version:` (chart) and `appVersion:` (app) track the
# release. CI overrides them when packaging from a tag; keeping them
# correct here is what makes `helm lint` and a local `helm package` tell
# the truth.
chart = pathlib.Path("distribution/charts/kmp/Chart.yaml")
text = chart.read_text()
text, c1 = re.subn(r'^version:.*$', f'version: {version}', text, count=1, flags=re.MULTILINE)
text, c2 = re.subn(r'^appVersion:.*$', f'appVersion: "{version}"', text, count=1, flags=re.MULTILINE)
if c1 == 0 or c2 == 0:
    sys.exit("Chart.yaml: version / appVersion line missing")
chart.write_text(text)

# The plugin host manifests. Claude reads this repository directly; the Codex
# marketplace mirrors this directory in underpass-ai/plugins. `release` checks
# that the public Codex mirror already advertises this version before tagging.
manifests = [
    pathlib.Path("plugins/kmp/.claude-plugin/plugin.json"),
    pathlib.Path("plugins/kmp/.codex-plugin/plugin.json"),
]
for manifest in manifests:
    text = manifest.read_text()
    text, c = re.subn(
        r'(^  "version": )"[^"]+"',
        rf'\1"{version}"',
        text,
        count=1,
        flags=re.MULTILINE,
    )
    if c == 0:
        sys.exit(f"{manifest}: no version line matched")
    manifest.write_text(text)

# MCP Registry metadata. A release bump deliberately invalidates the previous
# MCPB hash: keeping a syntactically valid stale hash is more dangerous than a
# placeholder which the registry gate refuses. Build the workflow-dispatch
# MCPB and run stamp-server-mcpb.sh before the version PR can go green.
server_json = pathlib.Path("server.json")
body = json.loads(server_json.read_text())
previous_server_version = body.get("version")
body["version"] = version
mcpb_hash_reset = False
for package in body.get("packages", []):
    if package.get("registryType") == "cargo":
        package["version"] = version
    elif package.get("registryType") == "mcpb":
        expected_identifier = (
            f"https://github.com/underpass-ai/kmp/releases/download/v{version}/"
            f"kmp-mcp-v{version}.mcpb"
        )
        if previous_server_version != version or package.get("identifier") != expected_identifier:
            package["fileSha256"] = "0" * 64
            mcpb_hash_reset = True
        package["identifier"] = expected_identifier
server_json.write_text(json.dumps(body, indent=2) + "\n")

mcpb_manifest = pathlib.Path("distribution/mcpb/manifest.json")
body = json.loads(mcpb_manifest.read_text())
body["version"] = version
mcpb_manifest.write_text(json.dumps(body, indent=2) + "\n")

print(
    f"bumped to {version}: Cargo.toml ({pinned} internal pins), "
    f"distribution/charts/kmp/Chart.yaml, {len(manifests)} plugin manifests, "
    "server.json and the MCPB manifest; "
    + ("MCPB hash needs stamping" if mcpb_hash_reset else "MCPB hash retained")
)
PY

    # Cargo.lock records the workspace members' own versions.
    cargo metadata --format-version 1 >/dev/null

    # Surface what changed — the caller reviews before committing.
    git --no-pager diff --stat -- Cargo.toml Cargo.lock distribution/charts/kmp/Chart.yaml \
        plugins/kmp/.claude-plugin/plugin.json plugins/kmp/.codex-plugin/plugin.json \
        server.json distribution/mcpb/manifest.json
}

cmd_release() {
    local version="$1"
    semver_check "${version}"

    local root
    root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
    cd "${root}"

    # A dirty tree would tag a commit that does not contain what was built.
    if [ -n "$(git status --porcelain)" ]; then
        echo "error: working tree is dirty — commit or stash first" >&2
        git status --short >&2
        exit 1
    fi

    # Versions must already match: `version` is where you bump, `release`
    # only tags. The plugin package job enforces the same equality, and
    # finding out there instead of here means a failed release.
    local cargo_version chart_version chart_app_version
    cargo_version="$(grep -m1 '^version = ' Cargo.toml | sed -E 's/version = "([^"]+)"/\1/')"
    chart_version="$(grep -m1 '^version:' distribution/charts/kmp/Chart.yaml | awk '{print $2}')"
    chart_app_version="$(grep -m1 '^appVersion:' distribution/charts/kmp/Chart.yaml | awk '{print $2}' | tr -d '"')"

    for field in cargo_version chart_version chart_app_version; do
        if [ "${!field}" != "${version}" ]; then
            echo "error: ${field}='${!field}' does not match target '${version}'" >&2
            echo "  hint: run 'bash scripts/release.sh version ${version}' and commit first" >&2
            exit 1
        fi
    done

    # This also proves the Cargo marker, MCPB hash, package URLs and manifest
    # all name this exact version. A tag never carries a listing that points
    # at a different or not-yet-built artifact.
    bash scripts/ci/mcp-registry.sh

    # Codex installs from the separate underpass-ai/plugins snapshot. Publish
    # that reviewed mirror before the tag makes this release discoverable;
    # otherwise an existing updater can install the new engine beside stale
    # skills and launchers. Build metadata is allowed as a cachebuster, but
    # its SemVer core must be the release being tagged.
    local marketplace_manifest marketplace_version
    marketplace_manifest="$(curl --proto '=https' --tlsv1.2 --connect-timeout 5 --max-time 20 \
        -fsSL "https://raw.githubusercontent.com/underpass-ai/plugins/main/plugins/kmp/.codex-plugin/plugin.json")" || {
        echo "error: could not verify the public Codex marketplace" >&2
        exit 1
    }
    marketplace_version="$(printf '%s' "${marketplace_manifest}" | python3 -c \
        'import json,sys; print(json.load(sys.stdin)["version"])')" || {
        echo "error: public Codex marketplace manifest is invalid" >&2
        exit 1
    }
    case "${marketplace_version}" in
        "${version}"|"${version}"+*) ;;
        *)
            echo "error: kmp@underpass is '${marketplace_version}', not '${version}'" >&2
            echo "  hint: merge the underpass-ai/plugins mirror PR before tagging" >&2
            exit 1
            ;;
    esac

    # Building once means the tag must name one already-reviewed candidate,
    # rather than quietly falling back to another five-platform compile. The
    # digest covers every tracked input that can change the release bytes but
    # deliberately excludes server.json: stamping the candidate MCPB hash is
    # the final release-branch edit and cannot invalidate the bytes it names.
    local candidate_input candidate_run candidate_root candidate_dir
    candidate_input="$(python3 scripts/release/release-candidate.py inputs)"
    mkdir -p "${root}/tmp"
    candidate_root="$(mktemp -d "${root}/tmp/release-candidate-verify.XXXXXX")"
    candidate_run=""
    while IFS= read -r run_id; do
        [ -n "${run_id}" ] || continue
        candidate_dir="${candidate_root}/${run_id}"
        mkdir -p "${candidate_dir}"
        if gh run download "${run_id}" \
            --name "kmp-release-candidate-${version}" \
            --dir "${candidate_dir}" >/dev/null 2>&1 \
            && python3 scripts/release/release-candidate.py verify \
                --version "${version}" \
                --directory "${candidate_dir}" \
                --input-sha256 "${candidate_input}" \
                --run-id "${run_id}" >/dev/null 2>&1; then
            candidate_run="${run_id}"
            break
        fi
    done < <(
        gh run list --workflow release.yml --event workflow_dispatch \
            --status success --limit 50 --json databaseId \
            --jq '.[].databaseId'
    )
    rm -rf "${candidate_root}"
    if [ -z "${candidate_run}" ]; then
        echo "error: no successful release candidate matches ${version} and inputs ${candidate_input}" >&2
        echo "  hint: run the release workflow manually on the reviewed version branch" >&2
        exit 1
    fi

    local tag="v${version}"
    if git rev-parse -q --verify "refs/tags/${tag}" >/dev/null; then
        echo "error: tag ${tag} already exists" >&2
        exit 1
    fi

    # Release tags come off reviewed history only.
    local branch
    branch="$(git rev-parse --abbrev-ref HEAD)"
    if [ "${branch}" != "main" ]; then
        echo "error: not on main (currently '${branch}')" >&2
        exit 1
    fi

    git tag -a "${tag}" \
        -m "Release ${tag}" \
        -m "candidate-run: ${candidate_run}" \
        -m "candidate-inputs: ${candidate_input}"
    git push origin "${tag}"
    echo "tagged ${tag} and pushed; candidate run ${candidate_run} approved."
    echo "publish-distribution: image + chart + crates.io chain."
    echo "release: promotes the candidate binaries, host bundles and MCPB without rebuilding."
    echo "Codex marketplace: verified kmp@underpass ${marketplace_version}."
    echo "mcp-registry: validates the tag; production publish remains gated."
}

if [ $# -lt 1 ]; then
    usage
fi

verb="$1"
shift

case "${verb}" in
    version)
        [ $# -eq 1 ] || usage
        cmd_version "$1"
        ;;
    release)
        [ $# -eq 1 ] || usage
        cmd_release "$1"
        ;;
    *)
        usage
        ;;
esac
