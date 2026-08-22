#!/usr/bin/env python3
"""Build and validate the pull-request evolution record for KMP.

The generated record follows both repositories in the code lineage:

* underpass-ai/rehydration-kernel (archived predecessor)
* underpass-ai/kmp (current repository)

Only merged pull requests are included. Repository evidence is calculated from
the merge commit's first-parent integration diff, not inferred from titles.
"""

from __future__ import annotations

import argparse
import dataclasses
import json
import os
import posixpath
import re
import subprocess
import sys
import urllib.error
import urllib.request
from collections import Counter
from pathlib import Path
from typing import Sequence


ROOT = Path(__file__).resolve().parents[1]
HISTORY_ROOT = ROOT / "docs" / "project-history"
INITIAL_DOC = HISTORY_ROOT / "0000-initial-foundation.md"
INDEX_DOC = HISTORY_ROOT / "README.md"
USER_AGENT = "kmp-pr-history-agent/1.0"


@dataclasses.dataclass(frozen=True)
class RepositorySpec:
    slug: str
    directory: str
    label: str


REPOSITORIES = (
    RepositorySpec(
        slug="underpass-ai/rehydration-kernel",
        directory="rehydration-kernel",
        label="Rehydration Kernel (archived predecessor)",
    ),
    RepositorySpec(
        slug="underpass-ai/kmp",
        directory="kmp",
        label="KMP",
    ),
)


@dataclasses.dataclass(frozen=True)
class PullRequest:
    repository: RepositorySpec
    number: int
    title: str
    body: str
    url: str
    author: str
    created_at: str
    merged_at: str
    base_ref: str
    base_sha: str
    head_ref: str
    head_sha: str
    merge_commit_sha: str

    @classmethod
    def from_api(cls, repository: RepositorySpec, payload: dict[str, object]) -> "PullRequest":
        user = payload.get("user") or {}
        base = payload.get("base") or {}
        head = payload.get("head") or {}
        if not isinstance(user, dict) or not isinstance(base, dict) or not isinstance(head, dict):
            raise ValueError(f"Malformed GitHub payload for {repository.slug}#{payload.get('number')}")

        merged_at = payload.get("merged_at")
        merge_commit_sha = payload.get("merge_commit_sha")
        if not merged_at or not merge_commit_sha:
            raise ValueError("PullRequest.from_api requires a merged pull request")

        return cls(
            repository=repository,
            number=int(payload["number"]),
            title=str(payload["title"]),
            body=str(payload.get("body") or "").strip(),
            url=str(payload["html_url"]),
            author=str(user.get("login") or "unknown"),
            created_at=str(payload["created_at"]),
            merged_at=str(merged_at),
            base_ref=str(base.get("ref") or "unknown"),
            base_sha=str(base.get("sha") or ""),
            head_ref=str(head.get("ref") or "unknown"),
            head_sha=str(head.get("sha") or ""),
            merge_commit_sha=str(merge_commit_sha),
        )

    @property
    def document_path(self) -> Path:
        return HISTORY_ROOT / self.repository.directory / f"pr-{self.number:04d}.md"


@dataclasses.dataclass(frozen=True)
class ChangedFile:
    status: str
    path: str
    additions: int | None
    deletions: int | None


@dataclasses.dataclass(frozen=True)
class IntegrationEvidence:
    merge_commit: str
    first_parent: str
    parent_count: int
    commits: tuple[tuple[str, str], ...]
    files: tuple[ChangedFile, ...]

    @property
    def additions(self) -> int:
        return sum(item.additions or 0 for item in self.files)

    @property
    def deletions(self) -> int:
        return sum(item.deletions or 0 for item in self.files)

    @property
    def binary_files(self) -> int:
        return sum(item.additions is None or item.deletions is None for item in self.files)

    @property
    def top_level_areas(self) -> tuple[str, ...]:
        areas = Counter(item.path.split("/", maxsplit=1)[0] for item in self.files)
        return tuple(name for name, _ in sorted(areas.items(), key=lambda item: (-item[1], item[0])))


def git(*args: str, check: bool = True) -> str:
    result = subprocess.run(
        ["git", *args],
        cwd=ROOT,
        check=False,
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
    )
    if check and result.returncode != 0:
        detail = result.stderr.strip() or result.stdout.strip()
        raise RuntimeError(f"git {' '.join(args)} failed: {detail}")
    return result.stdout.rstrip("\n")


def github_headers() -> dict[str, str]:
    headers = {
        "Accept": "application/vnd.github+json",
        "User-Agent": USER_AGENT,
        "X-GitHub-Api-Version": "2022-11-28",
    }
    token = os.environ.get("GITHUB_TOKEN")
    if token:
        headers["Authorization"] = f"Bearer {token}"
    return headers


def fetch_merged_pull_requests(repository: RepositorySpec) -> list[PullRequest]:
    records: list[PullRequest] = []
    page = 1
    while True:
        url = (
            f"https://api.github.com/repos/{repository.slug}/pulls"
            f"?state=closed&per_page=100&page={page}&sort=created&direction=asc"
        )
        request = urllib.request.Request(url, headers=github_headers())
        try:
            with urllib.request.urlopen(request, timeout=30) as response:
                payload = json.load(response)
        except urllib.error.HTTPError as error:
            remaining = error.headers.get("x-ratelimit-remaining", "unknown")
            raise RuntimeError(
                f"GitHub request failed for {repository.slug} (HTTP {error.code}, "
                f"rate-limit remaining: {remaining})"
            ) from error

        if not isinstance(payload, list):
            raise RuntimeError(f"GitHub returned a non-list payload for {repository.slug}")

        for item in payload:
            if isinstance(item, dict) and item.get("merged_at"):
                records.append(PullRequest.from_api(repository, item))

        if len(payload) < 100:
            break
        page += 1

    return sorted(records, key=lambda pr: (pr.merged_at, pr.number))


def commit_exists(sha: str) -> bool:
    result = subprocess.run(
        ["git", "cat-file", "-e", f"{sha}^{{commit}}"],
        cwd=ROOT,
        check=False,
        stdout=subprocess.DEVNULL,
        stderr=subprocess.DEVNULL,
    )
    return result.returncode == 0


def is_ancestor(sha: str, descendant: str = "main") -> bool:
    result = subprocess.run(
        ["git", "merge-base", "--is-ancestor", sha, descendant],
        cwd=ROOT,
        check=False,
        stdout=subprocess.DEVNULL,
        stderr=subprocess.DEVNULL,
    )
    return result.returncode == 0


def parse_numstat(raw: str) -> dict[str, tuple[int | None, int | None]]:
    stats: dict[str, tuple[int | None, int | None]] = {}
    for line in raw.splitlines():
        if not line:
            continue
        additions_raw, deletions_raw, path = line.split("\t", maxsplit=2)
        additions = None if additions_raw == "-" else int(additions_raw)
        deletions = None if deletions_raw == "-" else int(deletions_raw)
        stats[path] = (additions, deletions)
    return stats


def parse_numstat_z(raw: bytes) -> dict[str, tuple[int | None, int | None]]:
    """Parse `git diff --numstat -z`, including rename/copy triplets."""
    fields = raw.decode("utf-8", errors="surrogateescape").split("\0")
    stats: dict[str, tuple[int | None, int | None]] = {}
    index = 0
    while index < len(fields):
        field = fields[index]
        index += 1
        if not field:
            continue
        additions_raw, deletions_raw, path = field.split("\t", maxsplit=2)
        additions = None if additions_raw == "-" else int(additions_raw)
        deletions = None if deletions_raw == "-" else int(deletions_raw)
        if not path:
            if index + 1 >= len(fields):
                raise ValueError("Truncated rename/copy row in NUL-delimited numstat")
            index += 1  # old path
            path = fields[index]
            index += 1
        stats[path] = (additions, deletions)
    return stats


def integration_evidence(pr: PullRequest) -> IntegrationEvidence:
    if not commit_exists(pr.merge_commit_sha):
        raise RuntimeError(
            f"Merge commit {pr.merge_commit_sha} for {pr.repository.slug}#{pr.number} "
            "is absent; fetch the complete main history"
        )
    if not is_ancestor(pr.merge_commit_sha):
        raise RuntimeError(
            f"Merge commit {pr.merge_commit_sha} for {pr.repository.slug}#{pr.number} "
            "is not reachable from main"
        )

    parents = git("show", "-s", "--format=%P", pr.merge_commit_sha).split()
    if not parents:
        raise RuntimeError(f"Merged PR commit {pr.merge_commit_sha} unexpectedly has no parent")
    first_parent = parents[0]
    status_rows = git(
        "diff",
        "--name-status",
        "--find-renames",
        "--find-copies",
        first_parent,
        pr.merge_commit_sha,
    ).splitlines()
    numstat_result = subprocess.run(
        [
            "git",
            "diff",
            "--numstat",
            "-z",
            "--find-renames",
            "--find-copies",
            first_parent,
            pr.merge_commit_sha,
        ],
        cwd=ROOT,
        check=True,
        stdout=subprocess.PIPE,
    )
    numstat = parse_numstat_z(numstat_result.stdout)

    files: list[ChangedFile] = []
    for row in status_rows:
        columns = row.split("\t")
        status = columns[0]
        if status.startswith(("R", "C")) and len(columns) == 3:
            path = f"{columns[1]} -> {columns[2]}"
            stat_path = columns[2]
        else:
            path = columns[-1]
            stat_path = columns[-1]
        additions, deletions = numstat.get(stat_path, (None, None))
        files.append(ChangedFile(status=status, path=path, additions=additions, deletions=deletions))

    if len(parents) > 1:
        commit_range = f"{first_parent}..{parents[1]}"
        commit_lines = git(
            "log", "--reverse", "--format=%H%x09%s", commit_range
        ).splitlines()
    else:
        commit_lines = [git("show", "-s", "--format=%H%x09%s", pr.merge_commit_sha)]

    commits = tuple(
        (sha, subject)
        for line in commit_lines
        if line
        for sha, subject in [line.split("\t", maxsplit=1)]
    )
    return IntegrationEvidence(
        merge_commit=pr.merge_commit_sha,
        first_parent=first_parent,
        parent_count=len(parents),
        commits=commits,
        files=tuple(files),
    )


def normalized_category(title: str) -> str:
    lowered = re.sub(r"^\[codex\]\s*", "", title.strip(), flags=re.IGNORECASE).lower()
    if re.search(r"\b(remove|drop|retire|deprecat)(?:e|es|ed|ing|ion)?\b", lowered):
        return "removal"
    if lowered.startswith((
        "feat",
        "add ",
        "implement ",
        "introduce ",
        "build ",
        "connect ",
        "wire ",
        "publish ",
        "ship ",
        "jetstream ",
        "fase ",
        "frontier model benchmark",
        "bundle multi-resolution",
        "rehydrationmode",
        "kernel memory service",
    )):
        return "feature"
    if lowered.startswith(("fix", "harden ", "preserve ", "wait ", "close ", "pass github_token", "phase 0:")):
        return "fix"
    if lowered.startswith(("refactor", "reclassify", "recalibrate", "extract ", "isolate ")):
        return "refactor"
    if lowered.startswith(("docs", "document ", "teach ", "define ", "freeze ", "align ")):
        return "documentation"
    if lowered.startswith(("test", "cover ", "validate ")):
        return "test"
    if lowered.startswith(("ci", "scope workflow", "modernize workflow")):
        return "ci"
    if lowered.startswith(("chore: v", "release v")):
        return "release"
    if "bump " in lowered or lowered.startswith(("fix(deps)", "chore(deps)")):
        return "dependency"
    if lowered.startswith(("perf", "raise rust coverage", "clean up", "chore:")):
        return "maintenance"
    return "other"


def explicit_removal_signals(pr: PullRequest, evidence: IntegrationEvidence) -> list[str]:
    signals: list[str] = []
    removal_pattern = re.compile(
        r"\b(remove|removed|removes|removing|drop|dropped|retire|retired|deprecat(?:e|ed|es|ion))\b",
        re.IGNORECASE,
    )
    if removal_pattern.search(pr.title):
        signals.append(pr.title)
    for _, subject in evidence.commits:
        if removal_pattern.search(subject) and subject not in signals:
            signals.append(subject)
    return signals


def impact_lines(pr: PullRequest, evidence: IntegrationEvidence) -> list[str]:
    category = normalized_category(pr.title)
    additions = {
        "feature": pr.title,
    }
    changes = {
        "fix": pr.title,
        "refactor": pr.title,
        "documentation": pr.title,
        "test": pr.title,
        "ci": pr.title,
        "release": pr.title,
        "dependency": pr.title,
        "maintenance": pr.title,
        "other": pr.title,
        "removal": pr.title,
    }
    removal_signals = explicit_removal_signals(pr, evidence)
    added = additions.get(category, "No new capability is explicitly claimed by the PR title.")
    changed = changes.get(category, "The PR is classified as a new capability; inspect the evidence below for supporting changes.")
    removed = (
        "; ".join(removal_signals)
        if removal_signals
        else "No capability removal is explicitly claimed by the title or integrated commit subjects."
    )
    return [
        f"- **Classification:** `{category}`",
        f"- **Capability added:** {added}",
        f"- **Behavior or maintenance changed:** {changed}",
        f"- **Capability removed:** {removed}",
    ]


def safe_source_body(pr: PullRequest) -> str:
    if not pr.body:
        return "_The pull request has no description._"
    body = pr.body.replace("\r\n", "\n").replace("\r", "\n").strip()

    def make_permanent(match: re.Match[str]) -> str:
        prefix, target, suffix = match.groups()
        lowered = target.lower()
        if lowered.startswith(("http://", "https://", "mailto:", "data:", "#")):
            return match.group(0)
        path, separator, fragment = target.partition("#")
        normalized = posixpath.normpath("/" + path.lstrip("./"))
        permanent = (
            f"https://github.com/{pr.repository.slug}/blob/{pr.merge_commit_sha}"
            f"{normalized}"
        )
        if separator:
            permanent += f"#{fragment}"
        return f"{prefix}{permanent}{suffix}"

    return re.sub(r"(!?\[[^\]]*\]\()([^)\s]+)(\))", make_permanent, body)


def format_count(value: int | None) -> str:
    return "binary" if value is None else str(value)


def render_pr_document(pr: PullRequest, evidence: IntegrationEvidence) -> str:
    areas = ", ".join(f"`{area}`" for area in evidence.top_level_areas) or "_none_"
    merge_method = "merge commit" if evidence.parent_count > 1 else "single-parent integration (squash or rebase)"
    commit_rows = "\n".join(
        f"- [`{sha[:12]}`](https://github.com/{pr.repository.slug}/commit/{sha}) {subject}"
        for sha, subject in evidence.commits
    ) or "- _No branch commits could be isolated._"
    file_rows = "\n".join(
        f"- `{item.status}` `{item.path}` (+{format_count(item.additions)} / -{format_count(item.deletions)})"
        for item in evidence.files
    ) or "- _No tree change._"
    impact = "\n".join(impact_lines(pr, evidence))

    return f"""# {pr.repository.slug} PR #{pr.number}: {pr.title}

## Integration record

- **Status:** merged
- **Pull request:** [#{pr.number}]({pr.url})
- **Author:** [`{pr.author}`](https://github.com/{pr.author})
- **Created:** `{pr.created_at}`
- **Merged:** `{pr.merged_at}`
- **Base:** `{pr.base_ref}` at `{pr.base_sha}`
- **Head:** `{pr.head_ref}` at `{pr.head_sha}`
- **Merge commit:** [`{pr.merge_commit_sha}`](https://github.com/{pr.repository.slug}/commit/{pr.merge_commit_sha})
- **Integration method:** {merge_method}
- **Audited tree range:** `{evidence.first_parent}..{pr.merge_commit_sha}`

## Normalized impact

{impact}

This classification is deliberately conservative. The title and integrated
commit subjects support the normalized claims; the complete PR description and
tree evidence below remain the authority for detailed behavior.

## Author's change description

{safe_source_body(pr)}

## Verified repository evidence

- **Integrated commits:** {len(evidence.commits)}
- **Changed files:** {len(evidence.files)}
- **Text additions/deletions:** +{evidence.additions} / -{evidence.deletions}
- **Binary files:** {evidence.binary_files}
- **Top-level areas:** {areas}

### Integrated commits

{commit_rows}

<details>
<summary>Changed paths ({len(evidence.files)})</summary>

{file_rows}

</details>

## Audit note

The repository diff above is calculated from the first parent of the recorded
merge commit to the merge commit itself. It therefore records what this PR
introduced into `main`, even when the branch was stale or GitHub used a squash
merge. Statements in the PR description are author claims; changed paths and
commits are repository evidence.
"""


def markdown_table_text(value: str) -> str:
    return value.replace("|", "\\|").replace("\n", " ")


def render_index(all_prs: Sequence[PullRequest], evidence_by_key: dict[tuple[str, int], IntegrationEvidence]) -> str:
    tip = git("rev-parse", "main")
    snapshot_through = max(pr.merged_at for pr in all_prs)
    lines = [
        "# KMP pull-request evolution record",
        "",
        "This catalog records every merged pull request in the complete KMP code",
        "lineage: the archived `underpass-ai/rehydration-kernel` repository followed",
        "by the current `underpass-ai/kmp` repository. Reused PR numbers are kept in",
        "separate directories and are never conflated.",
        "",
        "## Coverage",
        "",
        f"- Latest merged PR in snapshot: `{snapshot_through}`",
        f"- Audited `main` tip: `{tip}`",
        f"- Initial foundation documents: 1",
        f"- Merged pull requests documented: {len(all_prs)}",
    ]
    for repository in REPOSITORIES:
        count = sum(pr.repository == repository for pr in all_prs)
        lines.append(f"- {repository.label}: {count} merged PRs")
    lines.extend(
        [
            "",
            "GitHub issues and pull requests share a numeric sequence. Missing PR",
            "numbers are therefore not gaps in this catalog: no Markdown file is",
            "invented when no merged pull-request object exists.",
            "",
            "## Evidence policy",
            "",
            "1. GitHub is authoritative for PR identity, author, description, state,",
            "   timestamps, refs, and merge SHA.",
            "2. The local Git graph is authoritative for reachability, first-parent",
            "   integration ranges, commits, paths, and line statistics.",
            "3. Feature classification is conservative. Positive removal claims require",
            "   removal language in the PR title or integrated commit subjects.",
            "4. The full PR description is retained next to verified tree evidence so a",
            "   reviewer can distinguish author intent from repository fact.",
            "",
            "Start with [the initial foundation](./0000-initial-foundation.md), then",
            "follow the tables below in merge order.",
        ]
    )

    for repository in REPOSITORIES:
        repository_prs = [pr for pr in all_prs if pr.repository == repository]
        lines.extend(
            [
                "",
                f"## {repository.label}",
                "",
                "| Merged | PR | Classification | Files | Change |",
                "| --- | ---: | --- | ---: | --- |",
            ]
        )
        for pr in repository_prs:
            evidence = evidence_by_key[(repository.slug, pr.number)]
            relative = f"./{repository.directory}/pr-{pr.number:04d}.md"
            lines.append(
                f"| `{pr.merged_at[:10]}` | [#{pr.number}]({relative}) | "
                f"`{normalized_category(pr.title)}` | {len(evidence.files)} | "
                f"{markdown_table_text(pr.title)} |"
            )
    lines.append("")
    return "\n".join(lines)


def render_initial_foundation(first_pr: PullRequest) -> str:
    roots = git("rev-list", "--max-parents=0", "main").splitlines()
    if len(roots) != 1:
        raise RuntimeError(f"Expected one root commit, found {len(roots)}")
    root = roots[0]
    endpoint = first_pr.base_sha
    if not commit_exists(endpoint):
        raise RuntimeError(f"Initial-boundary commit {endpoint} is absent")
    commit_count = int(git("rev-list", "--count", f"{root}^..{endpoint}", check=False) or "0")
    if commit_count == 0:
        commit_count = int(git("rev-list", "--count", endpoint))
    file_count = len(git("ls-tree", "-r", "--name-only", endpoint).splitlines())
    shortstat = git("diff", "--shortstat", root, endpoint)
    root_date = git("show", "-s", "--format=%aI", root)
    endpoint_date = git("show", "-s", "--format=%aI", endpoint)

    return f"""# Initial foundation: first commit to the first merged PR boundary

## Boundary

- **Root commit:** [`{root}`](https://github.com/underpass-ai/rehydration-kernel/commit/{root})
- **Root authored:** `{root_date}`
- **Last pre-PR commit:** [`{endpoint}`](https://github.com/underpass-ai/rehydration-kernel/commit/{endpoint})
- **Boundary authored:** `{endpoint_date}`
- **Next integration:** [`underpass-ai/rehydration-kernel` PR #1]({first_pr.url})
- **Commits reachable at the boundary:** {commit_count}
- **Files at the boundary:** {file_count}
- **Root-to-boundary tree delta:** {shortstat or '_no net tree delta_'}

The first PR branched after the initial `main` foundation. This document stops
at its recorded base SHA, so PR #1's changes are not counted twice.

## What existed before PR #1

The pre-PR foundation was intentionally small. Its repository evidence shows:

- the initial product README and project identity;
- a Rust workspace scaffold with domain, application, port, adapter, transport,
  server, observability, and test-oriented crate boundaries;
- an initial split of protobuf contracts; and
- protobuf generation plus a CI quality gate.

The asynchronous command/projection contract was not yet integrated into
`main`; that change begins with the first PR dossier.

## Net capability additions

- A compilable Rust/Protobuf project skeleton for an API-first memory kernel.
- Hexagonal boundaries that subsequent PRs could fill without coupling the
  protocol to a persistence or transport implementation.
- A generated-contract and CI baseline capable of detecting schema or quality
  regressions before later integrations.

## Net capability removals

No capability removal is evidenced in this pre-PR interval. The commits build
the initial repository from an empty root rather than replacing a prior
in-repository implementation.

## Evidence commands

```bash
git log --reverse {endpoint}
git diff --stat {root} {endpoint}
git ls-tree -r --name-only {endpoint}
```

This initial document uses the Git graph rather than a PR description because
no pull request exists for the interval.
"""


def collect() -> tuple[list[PullRequest], dict[tuple[str, int], IntegrationEvidence]]:
    all_prs: list[PullRequest] = []
    evidence_by_key: dict[tuple[str, int], IntegrationEvidence] = {}
    for repository in REPOSITORIES:
        repository_prs = fetch_merged_pull_requests(repository)
        if not repository_prs:
            raise RuntimeError(f"No merged pull requests found for {repository.slug}")
        for pr in repository_prs:
            key = (repository.slug, pr.number)
            if key in evidence_by_key:
                raise RuntimeError(f"Duplicate pull request key: {key}")
            evidence_by_key[key] = integration_evidence(pr)
        all_prs.extend(repository_prs)
    return sorted(all_prs, key=lambda pr: (pr.merged_at, pr.repository.slug, pr.number)), evidence_by_key


def sync() -> None:
    all_prs, evidence_by_key = collect()
    HISTORY_ROOT.mkdir(parents=True, exist_ok=True)
    for repository in REPOSITORIES:
        (HISTORY_ROOT / repository.directory).mkdir(parents=True, exist_ok=True)

    first_pr = all_prs[0]
    INITIAL_DOC.write_text(render_initial_foundation(first_pr), encoding="utf-8")
    for pr in all_prs:
        evidence = evidence_by_key[(pr.repository.slug, pr.number)]
        pr.document_path.write_text(render_pr_document(pr, evidence), encoding="utf-8")
    INDEX_DOC.write_text(render_index(all_prs, evidence_by_key), encoding="utf-8")
    validate_expected_files(all_prs)
    print(f"Documented {len(all_prs)} merged pull requests plus the initial foundation.")


def validate_expected_files(all_prs: Sequence[PullRequest]) -> None:
    expected = {pr.document_path.resolve() for pr in all_prs}
    actual = {
        path.resolve()
        for repository in REPOSITORIES
        for path in (HISTORY_ROOT / repository.directory).glob("pr-*.md")
    }
    missing = sorted(expected - actual)
    unexpected = sorted(actual - expected)
    if missing or unexpected:
        details = []
        if missing:
            details.append("missing: " + ", ".join(str(path.relative_to(ROOT)) for path in missing))
        if unexpected:
            details.append("unexpected: " + ", ".join(str(path.relative_to(ROOT)) for path in unexpected))
        raise RuntimeError("PR history coverage mismatch (" + "; ".join(details) + ")")


def validate() -> None:
    all_prs, evidence_by_key = collect()
    validate_expected_files(all_prs)
    if not INITIAL_DOC.is_file() or not INDEX_DOC.is_file():
        raise RuntimeError("Initial foundation or history index is missing")

    failures: list[str] = []
    for pr in all_prs:
        document = pr.document_path.read_text(encoding="utf-8")
        evidence = evidence_by_key[(pr.repository.slug, pr.number)]
        required_fragments = (
            f"# {pr.repository.slug} PR #{pr.number}: {pr.title}",
            f"- **Status:** merged",
            pr.url,
            pr.merge_commit_sha,
            f"- **Changed files:** {len(evidence.files)}",
            "## Normalized impact",
            "## Author's change description",
            "## Verified repository evidence",
        )
        for fragment in required_fragments:
            if fragment not in document:
                failures.append(f"{pr.document_path.relative_to(ROOT)} lacks {fragment!r}")

    index = INDEX_DOC.read_text(encoding="utf-8")
    if f"Merged pull requests documented: {len(all_prs)}" not in index:
        failures.append("history index has a stale merged-PR count")
    if failures:
        raise RuntimeError("\n".join(failures))
    print(f"Validated complete coverage for {len(all_prs)} merged pull requests.")


def parse_args(argv: Sequence[str]) -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("command", choices=("sync", "validate"))
    return parser.parse_args(argv)


def main(argv: Sequence[str] | None = None) -> int:
    args = parse_args(argv or sys.argv[1:])
    try:
        if args.command == "sync":
            sync()
        else:
            validate()
    except (OSError, RuntimeError, ValueError) as error:
        print(f"pr-history: {error}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
