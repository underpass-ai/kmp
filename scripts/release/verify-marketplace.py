#!/usr/bin/env python3
"""Refuse a release whose resolved marketplace artifacts or public copy drift."""

from __future__ import annotations

import argparse
import hashlib
import io
import json
import pathlib
import re
import subprocess
import sys
import tarfile
import urllib.error
import urllib.parse
import urllib.request


DEFAULT_ROOT = "https://raw.githubusercontent.com/underpass-ai/plugins/main/plugins/kmp"
DEFAULT_LISTING = "https://raw.githubusercontent.com/underpass-ai/plugins/main/.claude-plugin/marketplace.json"
DEFAULT_CODEX_LISTING = "https://raw.githubusercontent.com/underpass-ai/plugins/main/.agents/plugins/marketplace.json"
DEFAULT_README = "https://raw.githubusercontent.com/underpass-ai/plugins/main/README.md"
DEFAULT_CLAUDE_REPOSITORY = "https://github.com/underpass-ai/kmp.git"
CLAUDE_SOURCE = {
    "source": "git-subdir",
    "url": "https://github.com/underpass-ai/kmp.git",
    "path": "plugins/kmp",
}
CODEX_SOURCE = {"source": "local", "path": "./plugins/kmp"}
COMMIT_SHA = re.compile(r"[0-9a-f]{40}")


def github_commit_sha(repository: str, ref: str = "main") -> str:
    remote = f"https://github.com/{repository}.git"
    result = subprocess.run(
        ["git", "ls-remote", remote, f"refs/heads/{ref}"],
        check=False,
        capture_output=True,
        text=True,
    )
    fields = result.stdout.split()
    sha = fields[0] if result.returncode == 0 and fields else None
    if sha is None or not re.fullmatch(r"[0-9a-f]{40}", sha):
        detail = result.stderr.strip() or "ref was not found"
        raise SystemExit(f"could not resolve {repository}@{ref}: {detail}")
    return sha


def local_release_commit(version: str) -> str:
    repository = pathlib.Path(__file__).resolve().parents[2]
    tag = f"v{version}"
    result = subprocess.run(
        ["git", "rev-parse", "--verify", f"refs/tags/{tag}^{{commit}}"],
        cwd=repository,
        check=False,
        capture_output=True,
        text=True,
    )
    commit = result.stdout.strip()
    if result.returncode != 0 or not COMMIT_SHA.fullmatch(commit):
        detail = result.stderr.strip() or "tag was not found"
        raise SystemExit(
            f"could not resolve local release tag {tag}: {detail}; "
            "pre-tag verification must pass --expected-commit and --allow-unpublished-tag"
        )
    return commit


def remote_release_commit(
    repository: str,
    ref: str,
    expected_commit: str,
    allow_unpublished_tag: bool,
) -> str:
    result = subprocess.run(
        [
            "git",
            "ls-remote",
            "--tags",
            repository,
            f"refs/tags/{ref}",
            f"refs/tags/{ref}^{{}}",
        ],
        check=False,
        capture_output=True,
        text=True,
    )
    if result.returncode != 0:
        detail = result.stderr.strip() or "tag lookup failed"
        raise SystemExit(f"could not resolve Claude source tag {ref}: {detail}")

    refs = {}
    for line in result.stdout.splitlines():
        fields = line.split()
        if len(fields) == 2 and COMMIT_SHA.fullmatch(fields[0]):
            refs[fields[1]] = fields[0]

    tag_ref = f"refs/tags/{ref}"
    peeled_ref = f"{tag_ref}^{{}}"
    tag_object = refs.get(tag_ref)
    peeled_commit = refs.get(peeled_ref)
    if tag_object is None and peeled_commit is None and allow_unpublished_tag:
        if repository == DEFAULT_CLAUDE_REPOSITORY:
            main_commit = github_commit_sha("underpass-ai/kmp")
        else:
            head = subprocess.run(
                ["git", "ls-remote", repository, "refs/heads/main"],
                check=False,
                capture_output=True,
                text=True,
            )
            fields = head.stdout.split()
            main_commit = fields[0] if head.returncode == 0 and fields else ""
        if main_commit != expected_commit:
            raise SystemExit(
                f"unpublished Claude source tag {ref} would not name remote main: "
                f"expected {expected_commit}, found {main_commit or 'no main ref'}"
            )
        return expected_commit
    if tag_object is None:
        raise SystemExit(f"Claude source tag {ref} does not exist in {repository}")
    if peeled_commit is None:
        raise SystemExit(
            f"Claude source tag {ref} must be annotated so its peeled commit is auditable"
        )
    if peeled_commit != expected_commit:
        raise SystemExit(
            f"Claude source tag {ref} peels to {peeled_commit}, not expected commit {expected_commit}"
        )
    return peeled_commit


def pin_public_marketplace(args: argparse.Namespace) -> str | None:
    defaults = {
        "root": DEFAULT_ROOT,
        "listing": DEFAULT_LISTING,
        "codex_listing": DEFAULT_CODEX_LISTING,
        "readme": DEFAULT_README,
    }
    if not any(getattr(args, name) == value for name, value in defaults.items()):
        return None
    sha = args.marketplace_commit or github_commit_sha("underpass-ai/plugins")
    raw = f"https://raw.githubusercontent.com/underpass-ai/plugins/{sha}"
    replacements = {
        "root": f"{raw}/plugins/kmp",
        "listing": f"{raw}/.claude-plugin/marketplace.json",
        "codex_listing": f"{raw}/.agents/plugins/marketplace.json",
        "readme": f"{raw}/README.md",
    }
    for name, default in defaults.items():
        if getattr(args, name) == default:
            setattr(args, name, replacements[name])
    return sha


def digest_entries(entries: list[tuple[str, bool, bytes]]) -> str:
    digest = hashlib.sha256()
    for relative, executable, content in sorted(entries):
        if relative in (".claude-plugin/plugin.json", ".codex-plugin/plugin.json"):
            content = re.sub(
                rb'("version"\s*:\s*"[^"+]+)\+[^\"]+(\")',
                rb"\1\2",
                content,
                count=1,
            )
        path = relative.encode("utf-8")
        digest.update(len(path).to_bytes(8, "big"))
        digest.update(path)
        digest.update(b"x" if executable else b"-")
        digest.update(len(content).to_bytes(8, "big"))
        digest.update(content)
    return digest.hexdigest()


def directory_digest(root: pathlib.Path) -> str:
    entries = []
    for path in root.rglob("*"):
        if path.is_file():
            entries.append(
                (
                    path.relative_to(root).as_posix(),
                    bool(path.stat().st_mode & 0o111),
                    path.read_bytes(),
                )
            )
    return digest_entries(entries)


def local_plugin_digest() -> str:
    repository = pathlib.Path(__file__).resolve().parents[2]
    tracked = subprocess.run(
        ["git", "ls-files", "-z", "--", "plugins/kmp"],
        cwd=repository,
        check=True,
        stdout=subprocess.PIPE,
    ).stdout.split(b"\0")
    plugin = repository / "plugins/kmp"
    entries = []
    for raw in tracked:
        if not raw:
            continue
        path = repository / raw.decode("utf-8")
        entries.append(
            (
                path.relative_to(plugin).as_posix(),
                bool(path.stat().st_mode & 0o111),
                path.read_bytes(),
            )
        )
    return digest_entries(entries)


def archive_plugin_digest(repository: str, sha: str) -> str:
    url = f"https://github.com/{repository}/archive/{sha}.tar.gz"
    request = urllib.request.Request(
        url,
        headers={"User-Agent": "kmp-marketplace-release-gate/1"},
    )
    try:
        with urllib.request.urlopen(request, timeout=30) as response:
            payload = response.read()
    except (OSError, urllib.error.URLError) as error:
        raise SystemExit(f"could not read immutable plugin archive {url}: {error}")
    entries = []
    marker = "/plugins/kmp/"
    try:
        with tarfile.open(fileobj=io.BytesIO(payload), mode="r:gz") as archive:
            for member in archive.getmembers():
                if not member.isfile() or marker not in member.name:
                    continue
                handle = archive.extractfile(member)
                if handle is None:
                    raise SystemExit(f"could not read {member.name} from {url}")
                entries.append(
                    (
                        member.name.split(marker, 1)[1],
                        bool(member.mode & 0o111),
                        handle.read(),
                    )
                )
    except tarfile.TarError as error:
        raise SystemExit(f"immutable plugin archive {url} is invalid: {error}")
    if not entries:
        raise SystemExit(f"immutable plugin archive {url} has no plugins/kmp tree")
    return digest_entries(entries)


def read_manifest(root: str, relative: str) -> dict[str, object]:
    if root.startswith("https://"):
        url = urllib.parse.urljoin(f"{root.rstrip('/')}/", relative)
        request = urllib.request.Request(
            url,
            headers={"User-Agent": "kmp-marketplace-release-gate/1"},
        )
        try:
            with urllib.request.urlopen(request, timeout=20) as response:
                payload = response.read()
        except (OSError, urllib.error.URLError) as error:
            raise SystemExit(f"could not read public marketplace manifest {url}: {error}")
        source = url
    else:
        path = pathlib.Path(root) / relative
        try:
            payload = path.read_bytes()
        except OSError as error:
            raise SystemExit(f"could not read marketplace manifest {path}: {error}")
        source = str(path)

    try:
        value = json.loads(payload)
    except (UnicodeDecodeError, json.JSONDecodeError) as error:
        raise SystemExit(f"marketplace manifest {source} is not valid JSON: {error}")
    if not isinstance(value, dict):
        raise SystemExit(f"marketplace manifest {source} must contain a JSON object")
    return value


def verify_manifest(expected: str, host: str, root: str, relative: str) -> str:
    manifest = read_manifest(root, relative)
    version = manifest.get("version")
    if not isinstance(version, str) or not version:
        raise SystemExit(f"{host} marketplace manifest has no string version")
    if version.split("+", 1)[0] != expected:
        remediation = (
            "merge the KMP release candidate into underpass-ai/kmp main"
            if host == "Claude"
            else "merge the underpass-ai/plugins mirror PR"
        )
        raise SystemExit(
            f"kmp@underpass for {host} is {version!r}, not {expected!r}; "
            f"{remediation} before promoting the release"
        )
    verify_description(f"{host} marketplace manifest", manifest.get("description"))
    return version


def read_listing(source: str) -> dict[str, object]:
    if source.startswith("https://"):
        request = urllib.request.Request(
            source,
            headers={"User-Agent": "kmp-marketplace-release-gate/1"},
        )
        try:
            with urllib.request.urlopen(request, timeout=20) as response:
                payload = response.read()
        except (OSError, urllib.error.URLError) as error:
            raise SystemExit(f"could not read public marketplace listing {source}: {error}")
    else:
        try:
            payload = pathlib.Path(source).read_bytes()
        except OSError as error:
            raise SystemExit(f"could not read marketplace listing {source}: {error}")
    try:
        value = json.loads(payload)
    except (UnicodeDecodeError, json.JSONDecodeError) as error:
        raise SystemExit(f"marketplace listing {source} is not valid JSON: {error}")
    if not isinstance(value, dict):
        raise SystemExit(f"marketplace listing {source} must contain a JSON object")
    return value


def verify_description(source: str, description: object) -> None:
    if not isinstance(description, str) or "ChronoLoom" not in description:
        raise SystemExit(f"{source} must describe the ChronoLoom view")
    if re.search(r"\b(?:ten|10)(?:\s+kmp)?\s+(?:mcp\s+)?(?:moves|tools)\b", description, re.I):
        raise SystemExit(f"{source} still advertises the retired ten-move whole-surface count")


def kmp_entry(source: str) -> dict[str, object]:
    listing = read_listing(source)
    plugins = listing.get("plugins")
    if not isinstance(plugins, list):
        raise SystemExit("marketplace listing has no plugins array")
    kmp = next(
        (plugin for plugin in plugins if isinstance(plugin, dict) and plugin.get("name") == "kmp"),
        None,
    )
    if kmp is None:
        raise SystemExit("marketplace listing has no kmp entry")
    return kmp


def verify_claude_listing(
    expected: str,
    source: str,
    repository: str,
    expected_commit: str,
    allow_unpublished_tag: bool,
) -> tuple[str, str]:
    kmp = kmp_entry(source)
    verify_description("public marketplace kmp entry", kmp.get("description"))
    plugin_source = kmp.get("source")
    if not isinstance(plugin_source, dict):
        raise SystemExit("Claude marketplace kmp entry has no git-subdir source")
    stable = {key: plugin_source.get(key) for key in CLAUDE_SOURCE}
    if stable != CLAUDE_SOURCE:
        raise SystemExit("Claude marketplace kmp entry no longer resolves underpass-ai/kmp/plugins/kmp")
    ref = plugin_source.get("ref")
    release_ref = f"v{expected}"
    if ref != release_ref:
        raise SystemExit(
            f"Claude marketplace kmp entry must pin clonable immutable release tag {release_ref}"
        )
    commit = remote_release_commit(
        repository,
        release_ref,
        expected_commit,
        allow_unpublished_tag,
    )
    return f"https://raw.githubusercontent.com/underpass-ai/kmp/{commit}/plugins/kmp", commit


def verify_codex_listing(source: str) -> None:
    kmp = kmp_entry(source)
    if kmp.get("source") != CODEX_SOURCE:
        raise SystemExit("Codex marketplace kmp entry no longer resolves the reviewed plugins/kmp snapshot")


def read_text(source: str) -> str:
    if source.startswith("https://"):
        request = urllib.request.Request(
            source,
            headers={"User-Agent": "kmp-marketplace-release-gate/1"},
        )
        try:
            with urllib.request.urlopen(request, timeout=20) as response:
                payload = response.read()
        except (OSError, urllib.error.URLError) as error:
            raise SystemExit(f"could not read public marketplace README {source}: {error}")
    else:
        try:
            payload = pathlib.Path(source).read_bytes()
        except OSError as error:
            raise SystemExit(f"could not read marketplace README {source}: {error}")
    try:
        return payload.decode("utf-8")
    except UnicodeDecodeError as error:
        raise SystemExit(f"marketplace README {source} is not UTF-8: {error}")


def verify_readme(source: str) -> None:
    text = read_text(source)
    rows = [line for line in text.splitlines() if line.startswith("| `kmp` |")]
    if len(rows) != 1:
        raise SystemExit(f"public marketplace README must have one kmp product row, found {len(rows)}")
    row = rows[0]
    verify_description("public marketplace README kmp row", row)
    if re.search(r"\b(?:ten|10)(?:\s+kmp)?\s+(?:mcp\s+)?(?:moves|tools)\b", text, re.I):
        raise SystemExit("public marketplace README still contains retired whole-surface copy")
    for claim in ("thirteen MCP tools", "ten memory moves", "three shared ChronoLoom view tools"):
        if claim not in row:
            raise SystemExit(f"public marketplace README kmp row must say {claim!r}")


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("version", help="release SemVer without the v prefix")
    parser.add_argument(
        "--root",
        default=DEFAULT_ROOT,
        help="public raw plugin root, or a local fixture root for contract tests",
    )
    parser.add_argument(
        "--listing",
        default=DEFAULT_LISTING,
        help="public Claude marketplace listing URL, or a local fixture for contract tests",
    )
    parser.add_argument(
        "--codex-listing",
        default=DEFAULT_CODEX_LISTING,
        help="public Codex marketplace listing URL, or a local fixture for contract tests",
    )
    parser.add_argument(
        "--readme",
        default=DEFAULT_README,
        help="public marketplace README URL, or a local fixture for contract tests",
    )
    parser.add_argument(
        "--claude-root",
        help="override the Claude source root derived from the public listing (contract tests only)",
    )
    parser.add_argument(
        "--claude-repository",
        default=DEFAULT_CLAUDE_REPOSITORY,
        help="override the Git remote used to audit the Claude release tag (contract tests only)",
    )
    parser.add_argument(
        "--expected-commit",
        help="expected peeled release commit; defaults to the local annotated version tag",
    )
    parser.add_argument(
        "--allow-unpublished-tag",
        action="store_true",
        help="pre-tag release mode: require the expected commit at remote main when the tag is not published yet",
    )
    parser.add_argument(
        "--marketplace-commit",
        help=(
            "immutable underpass-ai/plugins commit to verify instead of public main; "
            "used to review the catalog before it is made public"
        ),
    )
    parser.add_argument(
        "--source-root",
        type=pathlib.Path,
        help="override the local KMP plugin source used for byte-parity tests",
    )
    args = parser.parse_args()

    if args.expected_commit is not None and not COMMIT_SHA.fullmatch(args.expected_commit):
        parser.error("--expected-commit must be a lowercase 40-character commit SHA")
    if args.marketplace_commit is not None and not COMMIT_SHA.fullmatch(
        args.marketplace_commit
    ):
        parser.error("--marketplace-commit must be a lowercase 40-character commit SHA")
    if args.allow_unpublished_tag and args.expected_commit is None:
        parser.error("--allow-unpublished-tag requires --expected-commit")

    marketplace_sha = pin_public_marketplace(args)
    expected_commit = args.expected_commit or local_release_commit(args.version)
    claude_root, claude_sha = verify_claude_listing(
        args.version,
        args.listing,
        args.claude_repository,
        expected_commit,
        args.allow_unpublished_tag,
    )
    verify_codex_listing(args.codex_listing)
    verify_readme(args.readme)
    claude = verify_manifest(
        args.version,
        "Claude",
        args.claude_root or claude_root,
        ".claude-plugin/plugin.json",
    )
    codex = verify_manifest(
        args.version,
        "Codex",
        args.root,
        ".codex-plugin/plugin.json",
    )
    expected_tree = (
        directory_digest(args.source_root) if args.source_root else local_plugin_digest()
    )
    claude_tree = (
        directory_digest(pathlib.Path(args.claude_root))
        if args.claude_root
        else archive_plugin_digest("underpass-ai/kmp", claude_sha)
    )
    codex_tree = (
        archive_plugin_digest("underpass-ai/plugins", marketplace_sha)
        if marketplace_sha
        else directory_digest(pathlib.Path(args.root))
    )
    if claude_tree != expected_tree:
        raise SystemExit(
            "Claude marketplace source has the expected version but not the release plugin tree"
        )
    if codex_tree != expected_tree:
        raise SystemExit(
            "Codex marketplace snapshot has the expected version but not the release plugin tree"
        )
    print(f"marketplace parity verified: kmp@underpass Claude={claude}, Codex={codex}")


if __name__ == "__main__":
    try:
        main()
    except BrokenPipeError:
        sys.exit(1)
