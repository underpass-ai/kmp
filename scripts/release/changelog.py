#!/usr/bin/env python3
"""Prepare and verify one Keep a Changelog release section."""

from __future__ import annotations

import argparse
import datetime as dt
import pathlib
import re


ROOT = pathlib.Path(__file__).resolve().parents[2]
DEFAULT_CHANGELOG = ROOT / "CHANGELOG.md"
SEMVER = re.compile(r"^[0-9]+\.[0-9]+\.[0-9]+(?:-[A-Za-z0-9.-]+)?$")
SECTION = re.compile(r"(?m)^## \[([^]]+)](?: - ([^\n]+))?\n")
ENTRY = re.compile(r"(?m)^- \S")


class ChangelogError(ValueError):
    """A release changelog does not satisfy the publication contract."""


def sections(text: str) -> list[tuple[str, int, int, str]]:
    matches = list(SECTION.finditer(text))
    return [
        (
            match.group(1),
            match.start(),
            matches[index + 1].start() if index + 1 < len(matches) else len(text),
            text[match.end() : matches[index + 1].start() if index + 1 < len(matches) else len(text)],
        )
        for index, match in enumerate(matches)
    ]


def section_for(text: str, name: str) -> tuple[str, int, int, str] | None:
    return next((section for section in sections(text) if section[0] == name), None)


def require_entries(section: tuple[str, int, int, str], path: pathlib.Path) -> None:
    if not ENTRY.search(section[3]):
        raise ChangelogError(f"{path}: [{section[0]}] has no changelog entries")


def require_release(text: str, version: str, path: pathlib.Path) -> None:
    target = section_for(text, version)
    if target is None:
        raise ChangelogError(f"{path}: missing release section ## [{version}]")
    require_entries(target, path)
    if not re.search(rf"(?m)^\[{re.escape(version)}]:\s+\S+", text):
        raise ChangelogError(f"{path}: missing [{version}] comparison link")


def prepare(path: pathlib.Path, version: str, release_date: str) -> bool:
    text = path.read_text(encoding="utf-8")
    existing = section_for(text, version)
    if existing is not None:
        require_release(text, version, path)
        return False

    unreleased = section_for(text, "Unreleased")
    if unreleased is None:
        raise ChangelogError(f"{path}: missing ## [Unreleased]")
    if not ENTRY.search(unreleased[3]):
        raise ChangelogError(
            f"{path}: [Unreleased] is empty and no [{version}] section exists"
        )

    following_versions = [
        name
        for name, start, _, _ in sections(text)
        if start > unreleased[1] and name != "Unreleased"
    ]
    if not following_versions:
        raise ChangelogError(f"{path}: cannot determine the previous release")
    previous = following_versions[0]
    notes = unreleased[3].strip()
    replacement = (
        "## [Unreleased]\n\n"
        f"## [{version}] - {release_date}\n\n"
        f"{notes}\n\n"
    )
    text = text[: unreleased[1]] + replacement + text[unreleased[2] :]

    unreleased_link = re.compile(r"(?m)^\[Unreleased]:\s+\S+$")
    match = unreleased_link.search(text)
    if match is None:
        raise ChangelogError(f"{path}: missing [Unreleased] comparison link")
    links = (
        f"[Unreleased]: https://github.com/underpass-ai/kmp/compare/v{version}...HEAD\n"
        f"[{version}]: https://github.com/underpass-ai/kmp/compare/"
        f"v{previous}...v{version}"
    )
    text = text[: match.start()] + links + text[match.end() :]
    require_release(text, version, path)
    path.write_text(text, encoding="utf-8")
    return True


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser()
    parser.add_argument("command", choices=("prepare", "check"))
    parser.add_argument("version")
    parser.add_argument("--path", type=pathlib.Path, default=DEFAULT_CHANGELOG)
    parser.add_argument("--date", default=dt.date.today().isoformat())
    return parser.parse_args()


def main() -> None:
    args = parse_args()
    if not SEMVER.fullmatch(args.version):
        raise SystemExit(f"changelog: invalid release version {args.version!r}")
    path = args.path.resolve()
    try:
        if args.command == "prepare":
            changed = prepare(path, args.version, args.date)
            action = "prepared" if changed else "already prepared"
        else:
            require_release(path.read_text(encoding="utf-8"), args.version, path)
            action = "verified"
    except (ChangelogError, OSError) as error:
        raise SystemExit(f"changelog: {error}") from error
    print(f"changelog: {action} [{args.version}] in {path}")


if __name__ == "__main__":
    main()
