#!/usr/bin/env python3
"""Merge LCOV line records and enforce KMP's line-coverage threshold."""

from __future__ import annotations

import argparse
import pathlib
import sys
import tempfile
from collections import defaultdict


LineCounts = dict[str, dict[int, int]]


def read_fragment(path: pathlib.Path) -> LineCounts:
    records: LineCounts = defaultdict(dict)
    source: str | None = None

    for raw_line in path.read_text(encoding="utf-8").splitlines():
        if raw_line.startswith("SF:"):
            source = raw_line[3:]
        elif raw_line.startswith("DA:"):
            if source is None:
                raise ValueError(f"{path}: DA record before SF")
            fields = raw_line[3:].split(",")
            if len(fields) < 2:
                raise ValueError(f"{path}: malformed DA record: {raw_line}")
            line_number = int(fields[0])
            count = int(fields[1])
            records[source][line_number] = max(
                records[source].get(line_number, 0), count
            )
        elif raw_line == "end_of_record":
            source = None

    if not records:
        raise ValueError(f"{path}: no LCOV line records")
    return records


def merge_fragments(paths: list[pathlib.Path]) -> LineCounts:
    merged: LineCounts = defaultdict(dict)
    for path in paths:
        for source, lines in read_fragment(path).items():
            destination = merged[source]
            for line_number, count in lines.items():
                # A line is covered when any test job reached it. Max avoids
                # pretending that hit counts are comparable across runners.
                destination[line_number] = max(
                    destination.get(line_number, 0), count
                )
    return merged


def write_lcov(records: LineCounts, destination: pathlib.Path) -> None:
    destination.parent.mkdir(parents=True, exist_ok=True)
    with destination.open("w", encoding="utf-8") as handle:
        for source in sorted(records):
            lines = records[source]
            covered = sum(count > 0 for count in lines.values())
            print("TN:KMP merged coverage", file=handle)
            print(f"SF:{source}", file=handle)
            for line_number, count in sorted(lines.items()):
                print(f"DA:{line_number},{count}", file=handle)
            print(f"LF:{len(lines)}", file=handle)
            print(f"LH:{covered}", file=handle)
            print("end_of_record", file=handle)


def self_test() -> None:
    with tempfile.TemporaryDirectory(prefix="kmp-coverage-contract-") as directory:
        root = pathlib.Path(directory)
        first = root / "unit.info"
        second = root / "integration.info"
        output = root / "merged.info"
        first.write_text(
            "TN:unit\nSF:/repo/a.rs\nDA:1,4\nDA:2,0\nend_of_record\n"
            "SF:/repo/b.rs\nDA:5,0\nend_of_record\n",
            encoding="utf-8",
        )
        second.write_text(
            "TN:integration\nSF:/repo/a.rs\nDA:1,0\nDA:2,3\nDA:3,0\n"
            "end_of_record\nSF:/repo/b.rs\nDA:5,1\nend_of_record\n",
            encoding="utf-8",
        )

        merged = merge_fragments([first, second])
        expected = {
            "/repo/a.rs": {1: 4, 2: 3, 3: 0},
            "/repo/b.rs": {5: 1},
        }
        if dict(merged) != expected:
            raise SystemExit(f"coverage merge self-test mismatch: {merged!r}")
        write_lcov(merged, output)
        rendered = output.read_text(encoding="utf-8")
        for required in ("DA:2,3", "LF:3", "LH:2", "LF:1", "LH:1"):
            if required not in rendered:
                raise SystemExit(
                    f"coverage merge self-test missing {required!r}: {rendered}"
                )
    print("coverage merge self-test passed: union and max-hit semantics")


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("fragments", nargs="*", type=pathlib.Path)
    parser.add_argument("--output", type=pathlib.Path)
    parser.add_argument("--fail-under-lines", type=float, default=80.0)
    parser.add_argument("--self-test", action="store_true")
    args = parser.parse_args()

    if args.self_test:
        self_test()
        return 0
    if not args.fragments or args.output is None:
        parser.error("provide LCOV fragments and --output, or use --self-test")

    missing = [path for path in args.fragments if not path.is_file()]
    if missing:
        parser.error("missing coverage fragments: " + ", ".join(map(str, missing)))

    try:
        records = merge_fragments(args.fragments)
    except (OSError, UnicodeError, ValueError) as error:
        print(f"coverage merge failed: {error}", file=sys.stderr)
        return 2

    write_lcov(records, args.output)
    total = sum(len(lines) for lines in records.values())
    covered = sum(
        count > 0 for lines in records.values() for count in lines.values()
    )
    percentage = covered * 100.0 / total if total else 0.0
    print(
        f"line coverage: {percentage:.2f}% ({covered}/{total}) "
        f"from {len(args.fragments)} test artifacts"
    )
    if percentage + 1e-9 < args.fail_under_lines:
        print(
            f"line coverage is below {args.fail_under_lines:.2f}%",
            file=sys.stderr,
        )
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
