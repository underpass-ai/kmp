#!/usr/bin/env python3
"""Mutation-tested contract for the path-scoped embedded launch workflow."""

from __future__ import annotations

import pathlib


ROOT = pathlib.Path(__file__).resolve().parents[2]
WORKFLOW = ROOT / ".github" / "workflows" / "embedded-launch-campaign.yml"
GATE = ROOT / "scripts" / "ci" / "embedded-launch-campaign.sh"
CAMPAIGN = ROOT / "campaign" / "embedded-launch"

WORKFLOW_CLAUSES = (
    "name: embedded-launch-campaign",
    "  pull_request:",
    "  push:",
    "      - main",
    "  workflow_dispatch:",
    '      - "campaign/embedded-launch/**"',
    '      - "docs/assets/campaign/kmp-embedded/**"',
    '      - ".github/workflows/embedded-launch-campaign.yml"',
    "runs-on: ubuntu-24.04",
    "permissions:\n      contents: read",
    "      - name: Install pinned campaign toolchain\n        timeout-minutes: 16",
    "actions/checkout@3d3c42e5aac5ba805825da76410c181273ba90b1",
    'FFMPEG_DEB_VERSION: "7:6.1.1-3ubuntu5"',
    'CSOUND_DEB_VERSION: "1:6.18.1+dfsg-1ubuntu4"',
    'JSONSCHEMA_DEB_VERSION: "4.10.3-2ubuntu1"',
    "sudo timeout --kill-after=15s 300 apt-get update",
    "-o Acquire::Retries=3",
    "-o Acquire::http::Timeout=30",
    "-o Acquire::https::Timeout=30",
    "sudo timeout --kill-after=15s 600 env DEBIAN_FRONTEND=noninteractive",
    "timeout --kill-after=1s 5s csound --version </dev/null",
    'head -n 2 "${RUNNER_TEMP}/kmp-csound-version.txt"',
    '"ffmpeg=${FFMPEG_DEB_VERSION}"',
    '"csound=${CSOUND_DEB_VERSION}"',
    '"python3-jsonschema=${JSONSCHEMA_DEB_VERSION}"',
    "python3 scripts/ci/embedded-launch-workflow-contract.py",
    "python3 scripts/ci/github-actions-contract.py",
    "run: bash scripts/ci/embedded-launch-campaign.sh",
)

GATE_CLAUSES = (
    "campaign/embedded-launch/scripts/validate-campaign.py",
    "campaign/embedded-launch/scripts/test_panel_contract.py",
    "campaign/embedded-launch/scripts/test_prepare_audio_panel.py",
    "campaign/embedded-launch/scripts/test_capture_portability.py",
    "campaign/embedded-launch/scripts/test_final_media_contract.py",
    "campaign/embedded-launch/scripts/test_final_regeneration_gate.py",
    "campaign/embedded-launch/obs-harness/scripts/test-obs-websocket-auth.mjs",
    "campaign/embedded-launch/obs-harness/scripts/test-obs-schedule.mjs",
    "campaign/embedded-launch/scripts/freeze-product-evidence.py check",
    "campaign/embedded-launch/obs-harness/scripts/validate-scenario.py",
    "campaign/embedded-launch/scripts/render-campaign.py --audio-only",
    "campaign/embedded-launch/scripts/test_audio_contract.py",
    'if [[ ! -f "${manifest}" ]]',
    "SOURCE VERIFIED; FINAL EVIDENCE NOT RUN",
    "campaign/embedded-launch/scripts/build-evidence-manifest.py check",
    "campaign/embedded-launch/scripts/panel_contract.py check",
    "campaign/embedded-launch/scripts/verify-final-media.py",
    'if [[ ! -x "${final_hook}" ]]',
    "FINAL BLOCKED",
    '"${final_hook}" --scratch "${scratch}"',
    "FINAL EVIDENCE VERIFIED, including deterministic regeneration",
)


def validate(workflow: str, gate: str) -> list[str]:
    failures: list[str] = []
    for clause in WORKFLOW_CLAUSES:
        if clause not in workflow:
            failures.append(f"workflow lost clause: {clause}")
    for clause in GATE_CLAUSES:
        if clause not in gate:
            failures.append(f"gate lost clause: {clause}")
    if workflow.count('      - "campaign/embedded-launch/**"') != 2:
        failures.append("campaign path scope must exist once for PR and once for main")
    if gate.count("campaign/embedded-launch/scripts/render-campaign.py --audio-only") != 2:
        failures.append("source audio must render twice before PCM comparison")
    if "continue-on-error" in workflow or "|| true" in gate:
        failures.append("campaign workflow contains a fail-open clause")
    return failures


def prove_mutation_guards(workflow: str, gate: str) -> None:
    mutations = {
        "floating checkout": (
            workflow.replace(
                "actions/checkout@3d3c42e5aac5ba805825da76410c181273ba90b1",
                "actions/checkout@v4",
                1,
            ),
            gate,
        ),
        "missing toolchain step timeout": (
            workflow.replace("        timeout-minutes: 16\n", "", 1),
            gate,
        ),
        "unbounded csound version probe": (
            workflow.replace(
                "timeout --kill-after=1s 5s csound --version </dev/null",
                "csound --version",
                1,
            ),
            gate,
        ),
        "unscoped pull requests": (
            workflow.replace('      - "campaign/embedded-launch/**"\n', "", 1),
            gate,
        ),
        "missing source test": (
            workflow,
            gate.replace(
                '"${python}" campaign/embedded-launch/scripts/test_capture_portability.py\n',
                "",
                1,
            ),
        ),
        "missing audio-panel preparer test": (
            workflow,
            gate.replace(
                '"${python}" campaign/embedded-launch/scripts/test_prepare_audio_panel.py\n',
                "",
                1,
            ),
        ),
        "missing OBS schedule contract": (
            workflow,
            gate.replace(
                "node campaign/embedded-launch/obs-harness/scripts/test-obs-schedule.mjs\n",
                "",
                1,
            ),
        ),
        "missing final regeneration source test": (
            workflow,
            gate.replace(
                '"${python}" campaign/embedded-launch/scripts/test_final_regeneration_gate.py\n',
                "",
                1,
            ),
        ),
        "missing manifest guard": (
            workflow,
            gate.replace('if [[ ! -f "${manifest}" ]]', "if false", 1),
        ),
        "missing deterministic hook": (
            workflow,
            gate.replace('"${final_hook}" --scratch "${scratch}"', "true", 1),
        ),
        "missing repeated source-audio comparison": (
            workflow,
            gate.replace(
                '"${python}" campaign/embedded-launch/scripts/test_audio_contract.py \\\n'
                '  "${audio_first}" "${audio_repeat}"',
                "true",
                1,
            ),
        ),
        "missing second source-audio render": (
            workflow,
            gate.replace(
                '"${python}" campaign/embedded-launch/scripts/render-campaign.py '
                '--audio-only "${audio_repeat}"',
                "true",
                1,
            ),
        ),
    }
    for name, (mutated_workflow, mutated_gate) in mutations.items():
        if mutated_workflow == workflow and mutated_gate == gate:
            raise SystemExit(f"embedded launch workflow self-test could not apply: {name}")
        if not validate(mutated_workflow, mutated_gate):
            raise SystemExit(f"embedded launch workflow contract missed mutation: {name}")


def validate_portable_sources() -> list[str]:
    failures: list[str] = []
    checked = (
        CAMPAIGN / "scripts" / "validate-campaign.py",
        CAMPAIGN / "scripts" / "build-critic-input.py",
        CAMPAIGN / "scripts" / "build-publication-manifest.py",
    )
    for path in checked:
        body = path.read_text(encoding="utf-8")
        if "/home/" in body or "kmp-campaign-agents" in body:
            failures.append(f"{path.relative_to(ROOT)} retains a machine-local dependency")
    critic = (CAMPAIGN / "scripts" / "build-critic-input.py").read_text(encoding="utf-8")
    for relative in (
        "campaign/embedded-launch/roles/marketing-director.md",
        "campaign/embedded-launch/roles/audio-director.md",
    ):
        if relative not in critic or not (ROOT / relative).is_file():
            failures.append(f"critic input does not bind versioned role {relative}")
    for name in (
        "campaign-brief.schema.json",
        "launch-critic-input.schema.json",
        "launch-critic-output.schema.json",
    ):
        if not (CAMPAIGN / "schema" / name).is_file():
            failures.append(f"missing repository schema {name}")
    return failures


def main() -> None:
    workflow = WORKFLOW.read_text(encoding="utf-8")
    gate = GATE.read_text(encoding="utf-8")
    failures = validate(workflow, gate) + validate_portable_sources()
    if failures:
        raise SystemExit(
            "embedded launch workflow contract failed:\n"
            + "\n".join(f"- {failure}" for failure in failures)
        )
    prove_mutation_guards(workflow, gate)
    print(
        "embedded launch workflow contract passed: path scope, immutable tools, "
        "source/final split, deterministic hook, portable roles/schemas, 12 mutation guards"
    )


if __name__ == "__main__":
    main()
