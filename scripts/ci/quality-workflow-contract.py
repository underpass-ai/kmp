#!/usr/bin/env python3
"""Regression contract for the change-aware, single-pass quality workflow."""

from __future__ import annotations

import pathlib
import re


ROOT = pathlib.Path(__file__).resolve().parents[2]
WORKFLOW = ROOT / ".github/workflows/quality-gate.yml"
PRODUCERS = {
    "test": ("unit", ()),
    "integration-valkey": ("valkey", ("scripts/ci/integration-valkey.sh",)),
    "integration-neo4j": ("neo4j", ("scripts/ci/integration-neo4j.sh",)),
    "integration-nats": ("nats", ("scripts/ci/integration-nats.sh",)),
    "integration-agentic-context": (
        "agentic-context",
        ("scripts/ci/integration-agentic-context.sh",),
    ),
    "integration-agentic-event-context": (
        "agentic-event-context",
        ("scripts/ci/integration-agentic-event-context.sh",),
    ),
    "integration-kernel-full-journey": (
        "full-journey",
        (
            "scripts/ci/integration-kernel-full-journey.sh",
            "scripts/ci/integration-kernel-full-journey-tls.sh",
        ),
    ),
}
JOB_HEADER = re.compile(r"^  ([a-zA-Z0-9_-]+):\s*$")


def job_block(text: str, name: str) -> str | None:
    lines = text.splitlines()
    start = next(
        (
            number
            for number, line in enumerate(lines)
            if line == f"  {name}:"
        ),
        None,
    )
    if start is None:
        return None
    end = next(
        (
            number
            for number, line in enumerate(lines[start + 1 :], start + 1)
            if JOB_HEADER.fullmatch(line)
        ),
        len(lines),
    )
    return "\n".join(lines[start:end])


def validate(text: str) -> list[str]:
    failures: list[str] = []

    for job, (fragment, scripts) in PRODUCERS.items():
        block = job_block(text, job)
        if block is None:
            failures.append(f"missing coverage-producing job {job}")
            continue
        clauses = [
            "uses: ./.github/actions/install-coverage",
            "run: cargo llvm-cov clean --workspace",
            "uses: ./.github/actions/upload-coverage",
            f"          name: {fragment}",
        ]
        if not scripts:
            clauses.append("run: cargo llvm-cov --no-report")
        else:
            clauses.append("KMP_COLLECT_COVERAGE: true")
            clauses.extend(f"run: bash {script}" for script in scripts)
        for clause in clauses:
            if clause not in block:
                failures.append(f"{job} lost single-pass coverage clause: {clause}")

    coverage = job_block(text, "coverage")
    if coverage is None:
        failures.append("missing coverage reducer job")
        return failures

    reducer_clauses = [
        "always()",
        "needs.test.result == 'success'",
        "!contains(needs.*.result, 'failure')",
        "uses: actions/download-artifact@",
        "pattern: coverage-*",
        "merge-multiple: true",
        "timeout-minutes: 5",
        "run: bash scripts/ci/rust-coverage.sh dist/coverage",
    ]
    for producer in PRODUCERS:
        reducer_clauses.append(f"      - {producer}")
    for clause in reducer_clauses:
        if clause not in coverage:
            failures.append(f"coverage reducer lost clause: {clause}")

    forbidden = {
        r"uses:\s+\./\.github/actions/install-(?:rust|coverage)": "toolchain install",
        r"install-protoc": "protoc install",
        r"run:\s+.*cargo(?:\s|$)": "cargo execution",
        r"run:\s+.*docker(?:\s|$)": "Docker execution",
        r"KMP_COLLECT_COVERAGE": "test instrumentation",
    }
    for pattern, description in forbidden.items():
        if re.search(pattern, coverage):
            failures.append(f"coverage reducer contains forbidden {description}")

    return failures


def prove_mutation_guards(text: str) -> None:
    mutations = {
        "second test suite": text.replace(
            "run: bash scripts/ci/rust-coverage.sh dist/coverage",
            "run: cargo test --workspace",
            1,
        ),
        "missing producer dependency": text.replace(
            "      - integration-nats\n", "", 1
        ),
        "missing unit artifact": text.replace(
            "uses: ./.github/actions/upload-coverage",
            "run: echo artifact-disabled",
            1,
        ),
        "missing failure propagation": text.replace(
            "      always()\n", "      success()\n", 1
        ),
    }
    for name, mutation in mutations.items():
        if mutation == text:
            raise SystemExit(f"quality workflow self-test could not apply: {name}")
        if not validate(mutation):
            raise SystemExit(f"quality workflow contract missed mutation: {name}")


def main() -> None:
    text = WORKFLOW.read_text(encoding="utf-8")
    failures = validate(text)
    if failures:
        raise SystemExit("quality workflow contract failed:\n" + "\n".join(failures))
    prove_mutation_guards(text)

    aggregator = (ROOT / "scripts/ci/rust-coverage.sh").read_text(encoding="utf-8")
    if re.search(r"^\s*(?:cargo|docker|podman)\b", aggregator, re.MULTILINE):
        raise SystemExit("coverage reducer script must not build or execute tests")

    for _, scripts in PRODUCERS.values():
        for script in scripts:
            body = (ROOT / script).read_text(encoding="utf-8")
            for clause in ("coverage-test.sh", "run_cargo_test"):
                if clause not in body:
                    raise SystemExit(
                        f"{script} lost coverage-aware test clause: {clause}"
                    )

    print(
        "quality workflow contract passed: "
        f"{len(PRODUCERS)} single-pass producers, reducer isolation, "
        "4 mutation guards"
    )


if __name__ == "__main__":
    main()
