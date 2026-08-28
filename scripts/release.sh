#!/usr/bin/env bash
set -euo pipefail

# Release helper for KMP.
#
# Three verbs:
#   version <X.Y.Z>   — rewrite every versioned artefact in the repo so
#                       Cargo, Helm, plugin and MCP Registry metadata stay in
#                       lockstep. Resets the MCPB hash until a matching bundle
#                       is built and stamped; idempotent and safe to re-run.
#
#   candidate <X.Y.Z> [RUN_ID]
#                     — dispatch (or reuse) the five-platform candidate build,
#                       wait for it, download and verify its twenty files, and
#                       stamp server.json with the exact MCPB digest. This is
#                       the only supported bridge from `version` to a green PR.
#
#   release <X.Y.Z>   — verify the tree is clean, versions already point at
#                       X.Y.Z and a successful workflow_dispatch candidate
#                       matches the release inputs, then create an annotated
#                       `vX.Y.Z` tag naming that candidate. The tag promotes
#                       its exact bytes and starts tag-only distribution.
#
# Typical flow:
#   bash scripts/release.sh version 0.2.0
#   git commit -am "chore: prepare v0.2.0" && git push
#   bash scripts/release.sh candidate 0.2.0
#   git commit -am "chore: seal v0.2.0" && git push
#   bash scripts/ci/quality-gate.sh
#   gh pr create --fill
#   # merge via CI
#   git checkout main && git pull
#   bash scripts/release.sh release 0.2.0

usage() {
    cat <<'USAGE' >&2
release.sh version <X.Y.Z>
release.sh candidate <X.Y.Z> [RUN_ID]
release.sh release <X.Y.Z>
USAGE
    exit 2
}

workspace_version() {
    python3 -c \
        'import tomllib; print(tomllib.load(open("Cargo.toml", "rb"))["workspace"]["package"]["version"])'
}

require_workspace_version() {
    local version="$1"
    local actual
    actual="$(workspace_version)"
    if [ "${actual}" != "${version}" ]; then
        echo "error: workspace version '${actual}' does not match target '${version}'" >&2
        echo "  hint: run 'bash scripts/release.sh version ${version}' first" >&2
        exit 1
    fi
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

    echo "next: commit and push this version branch, then run:" >&2
    echo "  bash scripts/release.sh candidate ${version}" >&2
}

cmd_candidate() {
    local version="$1"
    local run_id="${2:-}"
    semver_check "${version}"

    local root
    root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
    cd "${root}"
    require_workspace_version "${version}"

    # The helper must stamp one known tree. Uncommitted release inputs could
    # otherwise produce bytes which no commit — and therefore no tag — names.
    if [ -n "$(git status --porcelain)" ]; then
        echo "error: working tree is dirty — commit the version bump before building its candidate" >&2
        git status --short >&2
        exit 1
    fi

    local branch head_sha upstream_sha
    branch="$(git rev-parse --abbrev-ref HEAD)"
    head_sha="$(git rev-parse HEAD)"
    if [ "${branch}" = "HEAD" ]; then
        echo "error: candidate build requires a named version branch" >&2
        exit 1
    fi

    if [ -z "${run_id}" ]; then
        upstream_sha="$(git rev-parse --verify '@{upstream}' 2>/dev/null || true)"
        if [ -z "${upstream_sha}" ] || [ "${upstream_sha}" != "${head_sha}" ]; then
            echo "error: push ${branch} before building its candidate" >&2
            exit 1
        fi

        local known_runs
        known_runs="$(
            gh run list --workflow release.yml --event workflow_dispatch --limit 100 \
                --json databaseId --jq '.[].databaseId'
        )"
        gh workflow run release.yml --ref "${branch}"

        local attempt candidate_id
        for attempt in $(seq 1 60); do
            while IFS= read -r candidate_id; do
                [ -n "${candidate_id}" ] || continue
                if ! grep -qx "${candidate_id}" <<<"${known_runs}"; then
                    run_id="${candidate_id}"
                    break 2
                fi
            done < <(
                gh run list --workflow release.yml --event workflow_dispatch \
                    --branch "${branch}" --limit 20 \
                    --json databaseId,headSha \
                    --jq ".[] | select(.headSha == \"${head_sha}\") | .databaseId"
            )
            sleep 2
        done
        if [ -z "${run_id}" ]; then
            echo "error: release workflow dispatch did not appear for ${head_sha}" >&2
            exit 1
        fi
        echo "candidate run: ${run_id}"
    fi

    gh run watch "${run_id}" --exit-status

    mkdir -p "${root}/tmp"
    local candidate_root candidate_dir mcpb
    candidate_root="$(mktemp -d "${root}/tmp/release-candidate-stamp.XXXXXX")"
    trap 'rm -rf "${candidate_root}"' RETURN
    candidate_dir="${candidate_root}/candidate"
    mkdir -p "${candidate_dir}"
    gh run download "${run_id}" \
        --name "kmp-release-candidate-${version}" \
        --dir "${candidate_dir}"

    mcpb="${candidate_dir}/assets/kmp-mcp-v${version}.mcpb"
    bash scripts/release/stamp-server-mcpb.sh "${mcpb}"
    python3 scripts/release/release-candidate.py verify \
        --version "${version}" \
        --directory "${candidate_dir}" \
        --input-sha256 "$(python3 scripts/release/release-candidate.py inputs)" \
        --run-id "${run_id}"
    bash scripts/ci/mcp-registry.sh

    echo "candidate ${run_id} verified and server.json stamped." >&2
    echo "next: review, commit and push server.json; the PR registry check will turn green." >&2
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

    # Both hosts install from the separate underpass-ai/plugins snapshot.
    # Publish that reviewed mirror before the tag makes this release
    # discoverable; otherwise an updater can install the new engine beside
    # stale skills and launchers. The tag workflow repeats this same gate so a
    # manually pushed tag cannot bypass it.
    python3 scripts/release/verify-marketplace.py "${version}"

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
    echo "Plugin marketplace: verified kmp@underpass ${version} for Codex and Claude."
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
    candidate)
        [ $# -ge 1 ] && [ $# -le 2 ] || usage
        cmd_candidate "$@"
        ;;
    release)
        [ $# -eq 1 ] || usage
        cmd_release "$1"
        ;;
    *)
        usage
        ;;
esac
