#!/usr/bin/env python3
"""Synchronize the marketplace overview into GitHub and crates.io READMEs."""

from __future__ import annotations

import argparse
import pathlib
import re


ROOT = pathlib.Path(__file__).resolve().parents[2]
DEFAULT_SOURCE = ROOT / "plugins/kmp/README.md"
DEFAULT_TARGETS = (ROOT / "README.md", ROOT / "crates/kmp-mcp/README.md")
BEGIN = "<!-- kmp:public-overview:begin -->"
END = "<!-- kmp:public-overview:end -->"
CONTENT_CONTRACT = (
    ("local-first operation", r"\blocal[- ]first\b"),
    ("decision memory", r"\bdecisions?\b"),
    ("evidence", r"\bevidence\b"),
    ("the transcript boundary", r"\bnot transcripts\b"),
    ("the embedded SQLite engine", r"\bsqlite\b"),
    ("Codex support", r"\bcodex\b"),
    ("Claude Code support", r"\bclaude code\b"),
    ("ChronoLoom", r"\bchronoloom\b"),
    ("the ten memory tools", r"\bten\s+memory\s+tools\b"),
    ("the three semantic view tools", r"\bthree\s+semantic\s+view\s+tools\b"),
)


def marked_block(text: str, path: pathlib.Path) -> tuple[int, int, str]:
    if text.count(BEGIN) != 1 or text.count(END) != 1:
        raise ValueError(f"{path}: expected exactly one public-overview marker pair")
    start = text.index(BEGIN)
    end = text.index(END, start) + len(END)
    return start, end, text[start:end]


def synchronized(
    source: pathlib.Path, target: pathlib.Path
) -> tuple[str, str]:
    source_text = source.read_text(encoding="utf-8")
    target_text = target.read_text(encoding="utf-8")
    _, _, source_block = marked_block(source_text, source)
    start, end, _ = marked_block(target_text, target)
    return target_text, target_text[:start] + source_block + target_text[end:]


def validate_content(text: str, path: pathlib.Path) -> None:
    missing = [
        fact
        for fact, pattern in CONTENT_CONTRACT
        if re.search(pattern, text, flags=re.IGNORECASE) is None
    ]
    if missing:
        raise ValueError(
            f"{path}: public product story is missing " + ", ".join(missing)
        )


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser()
    parser.add_argument("command", choices=("sync", "check"))
    parser.add_argument("--source", type=pathlib.Path, default=DEFAULT_SOURCE)
    parser.add_argument("--target", action="append", type=pathlib.Path)
    return parser.parse_args()


def main() -> None:
    args = parse_args()
    source = args.source.resolve()
    targets = tuple(path.resolve() for path in (args.target or DEFAULT_TARGETS))
    stale: list[tuple[pathlib.Path, str]] = []
    try:
        source_text = source.read_text(encoding="utf-8")
        validate_content(source_text, source)
        for target in targets:
            current, expected = synchronized(source, target)
            if current != expected:
                stale.append((target, expected))
    except (OSError, ValueError) as error:
        raise SystemExit(f"public README sync: {error}") from error

    if args.command == "check":
        if stale:
            names = ", ".join(str(path) for path, _ in stale)
            raise SystemExit(
                f"public README sync: stale generated overview in {names}; run "
                "'python3 scripts/release/sync-public-readme.py sync'"
            )
        try:
            for target in targets:
                validate_content(target.read_text(encoding="utf-8"), target)
        except (OSError, ValueError) as error:
            raise SystemExit(f"public README sync: {error}") from error
        print("public README sync: GitHub, marketplace and crates.io match")
        return

    for target, expected in stale:
        validate_content(expected, target)
        target.write_text(expected, encoding="utf-8")
        print(f"public README sync: updated {target}")
    if not stale:
        print("public README sync: already current")


if __name__ == "__main__":
    main()
