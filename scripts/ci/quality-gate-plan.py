#!/usr/bin/env python3
"""Plan the smallest fail-closed quality gate for a repository change."""

from __future__ import annotations

import argparse
import json
import os
import pathlib
import subprocess
import sys
import tomllib
from collections import defaultdict, deque


ROOT = pathlib.Path(__file__).resolve().parents[2]
GATES = (
    "actions",
    "vendored",
    "publish",
    "rustfmt",
    "rust",
    "docs",
    "valkey",
    "neo4j",
    "nats",
    "embedded_binary",
    "embedded_sqlite",
    "conformance",
    "agentic_context",
    "agentic_event_context",
    "full_journey",
    "mcp_real",
    "container",
    "helm",
    "coverage",
    "codeql",
)


def workspace_graph() -> tuple[dict[str, pathlib.Path], dict[str, set[str]]]:
    manifests = sorted((ROOT / "crates").glob("*/Cargo.toml"))
    packages: dict[str, pathlib.Path] = {}
    documents: dict[pathlib.Path, dict[str, object]] = {}
    for manifest in manifests:
        body = tomllib.loads(manifest.read_text(encoding="utf-8"))
        name = str(body["package"]["name"])
        packages[name] = manifest.parent.relative_to(ROOT)
        documents[manifest] = body

    root = tomllib.loads((ROOT / "Cargo.toml").read_text(encoding="utf-8"))
    workspace_dependencies = root.get("workspace", {}).get("dependencies", {})
    local_names = set(packages)
    dependencies: dict[str, set[str]] = {name: set() for name in packages}

    def collect(table: object) -> set[str]:
        found: set[str] = set()
        if not isinstance(table, dict):
            return found
        for key, spec in table.items():
            candidate = key
            if isinstance(spec, dict):
                candidate = str(spec.get("package", key))
                if spec.get("workspace") is True:
                    inherited = workspace_dependencies.get(key, {})
                    if isinstance(inherited, dict):
                        candidate = str(inherited.get("package", key))
            if candidate in local_names:
                found.add(candidate)
        return found

    for manifest, body in documents.items():
        package = str(body["package"]["name"])
        for section in ("dependencies", "dev-dependencies", "build-dependencies"):
            dependencies[package].update(collect(body.get(section)))
        targets = body.get("target", {})
        if isinstance(targets, dict):
            for target in targets.values():
                if not isinstance(target, dict):
                    continue
                for section in ("dependencies", "dev-dependencies", "build-dependencies"):
                    dependencies[package].update(collect(target.get(section)))

    return packages, dependencies


def reverse_closure(changed: set[str], dependencies: dict[str, set[str]]) -> set[str]:
    reverse: dict[str, set[str]] = defaultdict(set)
    for package, package_dependencies in dependencies.items():
        for dependency in package_dependencies:
            reverse[dependency].add(package)
    affected = set(changed)
    queue = deque(sorted(changed))
    while queue:
        dependency = queue.popleft()
        for dependent in sorted(reverse[dependency]):
            if dependent not in affected:
                affected.add(dependent)
                queue.append(dependent)
    return affected


def empty_plan() -> dict[str, object]:
    return {
        "full": False,
        "reason": "path-specific",
        "changed_packages": [],
        "affected_packages": [],
        "cargo_packages": "",
        **{gate: False for gate in GATES},
    }


def full_plan(packages: dict[str, pathlib.Path], reason: str) -> dict[str, object]:
    names = sorted(packages)
    return {
        "full": True,
        "reason": reason,
        "changed_packages": names,
        "affected_packages": names,
        "cargo_packages": " ".join(f"-p {name}" for name in names),
        **{gate: True for gate in GATES},
    }


def is_markdown_or_docs(path: str) -> bool:
    return (
        path.startswith("docs/")
        or path.endswith(".md")
        or path in {"README.md", "CHANGELOG.md", "CODE_OF_CONDUCT.md", "CONTRIBUTING.md", "SECURITY.md"}
    )


def plan_for(paths: list[str], force_full: bool = False) -> dict[str, object]:
    packages, dependencies = workspace_graph()
    if force_full:
        return full_plan(packages, "explicit full run")
    if not paths:
        return full_plan(packages, "no changed paths could be established")

    normalized = sorted({path.removeprefix("./") for path in paths if path})
    full_roots = {
        "Cargo.toml",
        "Cargo.lock",
        "rust-toolchain.toml",
        ".github/workflows/quality-gate.yml",
        "scripts/ci/quality-gate-plan.py",
    }
    if any(
        path in full_roots
        or path.startswith(".github/actions/install-rust/")
        or path.startswith(
            (
                ".github/actions/install-coverage/",
                ".github/actions/upload-coverage/",
            )
        )
        or path in {"scripts/ci/install-protoc.sh", "scripts/ci/install-rust-toolchain.sh"}
        for path in normalized
    ):
        return full_plan(packages, "workspace, toolchain, or routing contract changed")

    plan = empty_plan()
    changed_packages: set[str] = set()
    known: set[str] = set()

    for name, directory in packages.items():
        prefix = directory.as_posix() + "/"
        if any(path.startswith(prefix) for path in normalized):
            changed_packages.add(name)
            known.update(path for path in normalized if path.startswith(prefix))

    if changed_packages:
        affected = reverse_closure(changed_packages, dependencies)
        plan["changed_packages"] = sorted(changed_packages)
        plan["affected_packages"] = sorted(affected)
        plan["cargo_packages"] = " ".join(f"-p {name}" for name in sorted(affected))
        for gate in ("rustfmt", "rust", "coverage", "codeql"):
            plan[gate] = True

        plan["valkey"] = "kmp-adapter-valkey" in affected
        plan["neo4j"] = "kmp-adapter-neo4j" in affected
        plan["nats"] = "kmp-adapter-nats" in affected
        plan["embedded_binary"] = bool({"kmp-mcp", "kmp-embedded"} & affected)
        plan["embedded_sqlite"] = bool(
            {"kmp-adapter-embedded", "kmp-conformance", "kmp-embedded", "kmp-mcp"}
            & affected
        )
        kernel_integration = "kmp-tests-kernel" in affected
        for gate in (
            "conformance",
            "agentic_context",
            "agentic_event_context",
            "full_journey",
            "mcp_real",
        ):
            plan[gate] = kernel_integration
        plan["container"] = bool(
            {"kmp-server", "kmp-mcp-http", "kmp-transport-grpc"} & affected
        )
        plan["publish"] = any(
            path.endswith("/Cargo.toml") and path in known for path in normalized
        )
        plan["vendored"] = any(
            path.startswith(
                (
                    "crates/kmp-proto/proto/",
                    "crates/kmp-mcp/fixtures/kernel/v1beta1/kmp/",
                )
            )
            for path in normalized
        )

    for path in normalized:
        if path in known:
            continue

        if path.startswith("api/"):
            known.add(path)
            plan["vendored"] = True
            plan["codeql"] = True
        elif path.startswith(".github/"):
            known.add(path)
            plan["actions"] = True
            plan["codeql"] = True
        elif path.startswith(("plugins/kmp/", "tests/plugin/", "scripts/plugin/")):
            known.add(path)
            plan["docs"] = plan["docs"] or path.endswith(".md")
        elif path.startswith("distribution/charts/"):
            known.add(path)
            plan["helm"] = True
        elif path.startswith("distribution/mcpb/"):
            known.add(path)
            plan["actions"] = True
        elif path.startswith("distribution/lexical-bridge/"):
            # The shipped table is proved readable by the kernel in a Rust test.
            known.add(path)
            plan["rust"] = True
            plan["docs"] = plan["docs"] or path.endswith(".md")
        elif path == "Dockerfile" or path == ".dockerignore" or path.startswith("e2e/"):
            known.add(path)
            plan["container"] = True
        elif path in {"server.json", "scripts/release.sh"} or path.startswith("scripts/release/"):
            known.add(path)
            plan["actions"] = True
            plan["publish"] = True
        elif path in {"scripts/ci/check-vendored-contract.sh"}:
            known.add(path)
            plan["vendored"] = True
        elif path in {"scripts/ci/check-publish-chain.sh", "scripts/ci/publish-crates.sh"}:
            known.add(path)
            plan["publish"] = True
        elif path in {"scripts/ci/github-actions-contract.py", "scripts/ci/publish-workflow-contract.py"}:
            known.add(path)
            plan["actions"] = True
        elif path == "scripts/ci/documentation-spine.sh" or is_markdown_or_docs(path):
            known.add(path)
            plan["docs"] = True
        elif path in {"scripts/ci/integration-valkey.sh"}:
            known.add(path)
            plan["valkey"] = True
        elif path in {"scripts/ci/integration-neo4j.sh"}:
            known.add(path)
            plan["neo4j"] = True
        elif path in {"scripts/ci/integration-nats.sh"}:
            known.add(path)
            plan["nats"] = True
        elif path in {"scripts/ci/integration-conformance.sh"}:
            known.add(path)
            plan["conformance"] = True
        elif path in {"scripts/ci/integration-agentic-context.sh"}:
            known.add(path)
            plan["agentic_context"] = True
        elif path in {"scripts/ci/integration-agentic-event-context.sh"}:
            known.add(path)
            plan["agentic_event_context"] = True
        elif path in {
            "scripts/ci/integration-kernel-full-journey.sh",
            "scripts/ci/integration-kernel-full-journey-tls.sh",
        }:
            known.add(path)
            plan["full_journey"] = True
        elif path in {"scripts/ci/integration-mcp-real-kernel.sh"}:
            known.add(path)
            plan["mcp_real"] = True
        elif path in {"scripts/ci/embedded-binary-gates.sh"}:
            known.add(path)
            plan["embedded_binary"] = True
        elif path in {"scripts/ci/embedded-sqlite-gates.sh"}:
            known.add(path)
            plan["embedded_sqlite"] = True
        elif path in {"scripts/ci/container-image.sh"}:
            known.add(path)
            plan["container"] = True
        elif path in {"scripts/ci/helm-lint.sh", "scripts/ci/install-helm.sh"}:
            known.add(path)
            plan["helm"] = True
        elif path in {
            "scripts/ci/coverage-test.sh",
            "scripts/ci/export-coverage-fragment.sh",
            "scripts/ci/merge-coverage.py",
            "scripts/ci/quality-workflow-contract.py",
            "scripts/ci/rust-coverage.sh",
        }:
            return full_plan(packages, "coverage collection contract changed")
        elif path == "scripts/ci/testcontainers-runtime.sh":
            return full_plan(packages, "shared container-test runtime changed")
        elif path.startswith(("scripts/docs/", "docs/assets/")):
            known.add(path)
            plan["docs"] = True
        elif path.startswith(("scripts/mcp/", "scripts/install/")):
            known.add(path)
            plan["embedded_binary"] = True
        elif path.startswith("scripts/e2e/"):
            known.add(path)
            plan["rust"] = True
            plan["codeql"] = True
        elif path.startswith("scripts/ci/kmp-") or path.startswith("scripts/ci/pitch-"):
            known.add(path)
            plan["codeql"] = True
        elif path.startswith(".kmp/") or path.startswith(".kernel/"):
            known.add(path)
        elif path in {".gitignore", ".gitattributes", ".github/pull_request_template.md", "LICENSE", "NOTICE", "THIRD_PARTY_NOTICES.md"}:
            known.add(path)
            plan["docs"] = True

    unknown = sorted(set(normalized) - known)
    if unknown:
        return full_plan(packages, "unknown paths: " + ", ".join(unknown))

    if plan["docs"]:
        plan["reason"] = "documentation and path-specific gates"
    elif changed_packages:
        plan["reason"] = "changed crates and reverse dependencies"
    return plan


def changed_paths(base: str, head: str) -> list[str]:
    result = subprocess.run(
        ["git", "diff", "--name-only", "--diff-filter=ACMR", base, head, "--"],
        cwd=ROOT,
        check=True,
        text=True,
        stdout=subprocess.PIPE,
    )
    return [line for line in result.stdout.splitlines() if line]


def write_outputs(plan: dict[str, object], destination: pathlib.Path) -> None:
    with destination.open("a", encoding="utf-8") as handle:
        for key, value in plan.items():
            if isinstance(value, bool):
                rendered = str(value).lower()
            elif isinstance(value, list):
                rendered = json.dumps(value, separators=(",", ":"))
            else:
                rendered = str(value)
            print(f"{key}={rendered}", file=handle)
        print(f"test={str(bool(plan['rust'] or plan['docs'])).lower()}", file=handle)


def self_test() -> None:
    cases = [
        ("docs", ["docs/architecture/index.md"], {"docs": True, "rust": False, "coverage": False}),
        ("helm", ["distribution/charts/kmp/values.yaml"], {"helm": True, "rust": False}),
        ("container", ["Dockerfile"], {"container": True, "coverage": False}),
        ("release workflow", [".github/workflows/release.yml"], {"actions": True, "rust": False}),
        ("plugin policy", ["plugins/kmp/skills/kmp-memory/SKILL.md"], {"docs": True, "rust": False}),
        ("valkey crate", ["crates/kmp-adapter-valkey/src/lib.rs"], {"valkey": True, "neo4j": False, "rust": True}),
        ("nats crate", ["crates/kmp-adapter-nats/src/lib.rs"], {"nats": True, "valkey": False, "rust": True}),
        ("kernel journey", ["crates/kmp-tests-kernel/tests/kernel_full_journey_integration.rs"], {"full_journey": True, "rust": True}),
        ("coverage contract", ["scripts/ci/merge-coverage.py"], {"full": True, "coverage": True}),
        ("coverage action", [".github/actions/install-coverage/action.yml"], {"full": True, "coverage": True}),
        ("container runtime", ["scripts/ci/testcontainers-runtime.sh"], {"full": True, "coverage": True}),
        ("workspace lock", ["Cargo.lock"], {"full": True, "coverage": True}),
        ("router", ["scripts/ci/quality-gate-plan.py"], {"full": True}),
        ("unknown", ["new-top-level.bin"], {"full": True}),
    ]
    for name, paths, expected in cases:
        actual = plan_for(paths)
        for key, value in expected.items():
            if actual[key] != value:
                raise SystemExit(f"{name}: expected {key}={value!r}, got {actual[key]!r}")
    print(f"quality gate plan self-test passed: {len(cases)} routing cases")


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--base")
    parser.add_argument("--head")
    parser.add_argument("--path", action="append", default=[])
    parser.add_argument("--full", action="store_true")
    parser.add_argument("--github-output", type=pathlib.Path)
    parser.add_argument("--self-test", action="store_true")
    args = parser.parse_args()

    if args.self_test:
        self_test()
        return
    if args.path:
        paths = args.path
    elif args.base and args.head:
        paths = changed_paths(args.base, args.head)
    elif args.full:
        paths = []
    else:
        parser.error("provide --base/--head, --path, --full, or --self-test")

    plan = plan_for(paths, force_full=args.full)
    if args.github_output:
        write_outputs(plan, args.github_output)
    print(json.dumps({"paths": sorted(paths), "plan": plan}, indent=2))


if __name__ == "__main__":
    main()
