#!/usr/bin/env python3
"""Merge LCOV line records and enforce KMP's line-coverage floors.

The bar is per crate, not per run. A single aggregate percentage is only
meaningful over a fixed denominator, and this repository deliberately narrows
the test plan to the crates a change can reach — so the aggregate silently
became "whatever the router selected", and a plan narrowed to one crate held
that crate to a bar calibrated on the whole workspace.

Every crate is now held to the repository bar on its own, which is strictly
stronger: an under-tested crate can no longer hide behind a well-tested one.
The crates already below it carry a recorded floor that may rise freely and can
only be lowered by a reviewed change that says why."""

from __future__ import annotations

import argparse
import math
import pathlib
import sys
import tempfile
from collections import defaultdict


LineCounts = dict[str, dict[int, int]]
CrateCoverage = dict[str, tuple[int, int]]

FLOOR_PREAMBLE = """\
# Line-coverage floors for crates below the repository bar.
# A crate with no entry here must reach the bar on its own.
# A floor may be raised freely. Lowering one is a reviewed change that says why.
crate\tfloor
"""


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
        floors_path = root / "coverage-floors.tsv"
        # Two crates, one comfortably above the bar and one far below it.
        crate_records: LineCounts = {
            "/repo/crates/kmp-domain/src/a.rs": {1: 1, 2: 1, 3: 1, 4: 1, 5: 0},
            "/repo/crates/kmp-release/src/b.rs": {1: 1, 2: 0, 3: 0, 4: 0},
            "/repo/e2e/harness.rs": {1: 0},
        }
        measured = crate_coverage(crate_records)
        if measured != {"kmp-domain": (4, 5), "kmp-release": (1, 4)}:
            raise SystemExit(f"crate grouping self-test mismatch: {measured!r}")

        floors = write_floors(floors_path, {}, measured, 80.0)
        if floors != {"kmp-release": 25.0}:
            raise SystemExit(f"floor baseline self-test mismatch: {floors!r}")
        if read_floors(floors_path) != {"kmp-release": 25.0}:
            raise SystemExit("recorded floors must round-trip")
        # A floor is a whole percent: 3 of 4 lines records 75, not 75.0000001.
        if write_floors(floors_path, {}, {"kmp-x": (3, 4)}, 80.0)["kmp-x"] != 75.0:
            raise SystemExit("floors are whole percents")

        # The ratchet only turns one way: a later run that measures less than
        # the record must not be able to write the bar down.
        weaker = {"kmp-release": (0, 4)}
        if write_floors(floors_path, floors, weaker, 80.0).get("kmp-release") != 25.0:
            raise SystemExit("a weaker run must not lower a recorded floor")

        # A crate with no entry is held to the bar, and one with an entry is
        # held to its floor — both on their own numbers, not on the aggregate.
        # Enforcement follows the plan: a crate covered only incidentally
        # by another crate's tests is measured but never judged, and a blank
        # plan judges everything.
        incidental: LineCounts = {
            "/repo/crates/kmp-domain/src/a.rs": {1: 1, 2: 1, 3: 1, 4: 1},
            "/repo/crates/kmp-config/src/lib.rs": {1: 1, 2: 0, 3: 0, 4: 0},
        }
        sideways = crate_coverage(incidental)
        if enforce_floors(sideways, {}, 80.0, enforce={"kmp-domain"}):
            raise SystemExit("a crate outside the plan must not be judged")
        if not enforce_floors(sideways, {}, 80.0, enforce=None):
            raise SystemExit("a blank plan judges every measured crate")
        if not enforce_floors(sideways, {}, 80.0, enforce={"kmp-config"}):
            raise SystemExit("a crate inside the plan is judged on its own numbers")
        if enforced_crates("-p kmp-a -p kmp-b") != {"kmp-a", "kmp-b"}:
            raise SystemExit("plan parsing must read `-p name` pairs")
        if enforced_crates("") is not None:
            raise SystemExit("a blank plan spec must judge everything")
        if enforce_floors(measured, {"kmp-release": 25.0}, 80.0):
            raise SystemExit("a crate at its recorded floor must pass")
        if not enforce_floors(measured, {"kmp-release": 50.0}, 80.0):
            raise SystemExit("a crate below its recorded floor must fail")
        if not enforce_floors(measured, {}, 80.0):
            raise SystemExit("an unrecorded crate below the bar must fail")
        if enforce_floors({"kmp-domain": (4, 5)}, {}, 80.0):
            raise SystemExit("a crate at the bar must pass with no entry")
    print(
        "coverage merge self-test passed: union and max-hit semantics, "
        "per-crate floors, one-way ratchet"
    )


def crate_of(source: str) -> str | None:
    """The workspace crate a source path belongs to, or None for anything
    outside `crates/` — a build script, a generated file, a vendored path."""
    parts = pathlib.PurePosixPath(source.replace("\\", "/")).parts
    if "crates" not in parts:
        return None
    index = len(parts) - 1 - parts[::-1].index("crates")
    return parts[index + 1] if index + 1 < len(parts) else None


def crate_coverage(records: LineCounts) -> CrateCoverage:
    totals: CrateCoverage = {}
    for source, lines in records.items():
        crate = crate_of(source)
        if crate is None:
            continue
        covered, total = totals.get(crate, (0, 0))
        totals[crate] = (
            covered + sum(count > 0 for count in lines.values()),
            total + len(lines),
        )
    return totals


def enforced_crates(spec: str) -> set[str] | None:
    """The crates the plan selected, parsed from its `-p name` list.

    `None` means judge every measured crate: an absent or blank spec is a
    full run or an older caller, and silence must never weaken the gate.
    """
    tokens = spec.split()
    names = {name for flag, name in zip(tokens, tokens[1:]) if flag == "-p"}
    return names or None


def read_floors(path: pathlib.Path) -> dict[str, float]:
    floors: dict[str, float] = {}
    if not path.is_file():
        return floors
    for line in path.read_text(encoding="utf-8").splitlines():
        if not line or line.startswith("#") or line.startswith("crate\t"):
            continue
        name, _, floor = line.partition("\t")
        floors[name.strip()] = float(floor)
    return floors


def write_floors(
    path: pathlib.Path,
    floors: dict[str, float],
    measured: CrateCoverage,
    bar: float,
) -> dict[str, float]:
    """Ratchets the recorded floors up to what this run measured.

    Floors are whole percents, rounded down. Two honest runs of the same tree
    disagree in the last decimal — a different LLVM version counts a few lines
    differently — and a ratchet that trips on that noise teaches people to
    refresh it reflexively, which is the one thing it must not become.

    It never writes a floor down. A run that measured less than the record — a
    narrower plan, a skipped container job — must not be able to relax the bar
    by being run with the baseline flag set.
    """
    updated = dict(floors)
    for crate, (covered, total) in sorted(measured.items()):
        if not total:
            continue
        percentage = covered * 100.0 / total
        if percentage + 1e-9 >= bar:
            continue
        updated[crate] = max(updated.get(crate, 0.0), float(math.floor(percentage)))
    path.parent.mkdir(parents=True, exist_ok=True)
    with path.open("w", encoding="utf-8") as handle:
        handle.write(FLOOR_PREAMBLE)
        for crate, floor in sorted(updated.items()):
            handle.write(f"{crate}\t{floor:g}\n")
    return updated


def enforce_floors(
    measured: CrateCoverage,
    floors: dict[str, float],
    bar: float,
    enforce: set[str] | None = None,
) -> list[str]:
    """Judges each crate the plan selected against its floor.

    A crate outside the plan is measured but not judged: its lines were
    covered only incidentally, by tests that never claimed to prove it, and
    holding it to a floor calibrated on the full run makes every narrowed
    plan red for reasons the change cannot reach.
    """
    failures = []
    recorded = 0
    judged = 0
    for crate, (covered, total) in sorted(measured.items()):
        if not total:
            continue
        if enforce is not None and crate not in enforce:
            continue
        judged += 1
        percentage = covered * 100.0 / total
        floor = floors.get(crate, bar)
        if crate in floors:
            recorded += 1
        if percentage + 1e-9 < floor:
            failures.append(
                f"{crate}: {percentage:.2f}% is below its {floor:.2f}% floor"
            )
        elif crate in floors and percentage > floor + 1.0:
            print(
                f"  paid down: {crate} reached {percentage:.2f}%, "
                f"above its {floor:.2f}% floor"
            )
    print(
        f"per-crate floors: {judged} judged of {len(measured)} measured · "
        f"{recorded} with a recorded floor · "
        f"{judged - recorded} held to {bar:.2f}%"
    )
    return failures


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("fragments", nargs="*", type=pathlib.Path)
    parser.add_argument("--output", type=pathlib.Path)
    parser.add_argument("--fail-under-lines", type=float, default=80.0)
    parser.add_argument("--floors", type=pathlib.Path)
    parser.add_argument("--write-floors", action="store_true")
    parser.add_argument(
        "--enforce-only",
        default="",
        help="the plan's `-p name` list; blank judges every measured crate",
    )
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

    measured = crate_coverage(records)
    if args.floors is None:
        if percentage + 1e-9 < args.fail_under_lines:
            print(
                f"line coverage is below {args.fail_under_lines:.2f}%",
                file=sys.stderr,
            )
            return 1
        return 0

    enforce = enforced_crates(args.enforce_only)
    floors = read_floors(args.floors)
    if args.write_floors:
        # The ratchet follows the plan too: a narrowed run must not record
        # an incidental crate's low number as if it were that crate's truth.
        ratchet = {
            crate: coverage
            for crate, coverage in measured.items()
            if enforce is None or crate in enforce
        }
        floors = write_floors(args.floors, floors, ratchet, args.fail_under_lines)
        print(f"wrote {args.floors} with {len(floors)} recorded floors")
        return 0

    failures = enforce_floors(measured, floors, args.fail_under_lines, enforce)
    if failures:
        print(file=sys.stderr)
        for failure in failures:
            print(f"  {failure}", file=sys.stderr)
        print(
            "\nraise the crate's coverage, or record a reviewed floor in "
            f"{args.floors}",
            file=sys.stderr,
        )
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
