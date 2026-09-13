#!/usr/bin/env python3
"""Compare complete read results before summarizing two baseline.py runs."""
import gzip
import json
from pathlib import Path
import sys


def main():
    before, after = (Path(value).resolve() for value in sys.argv[1:])
    left, right = [json.loads((root / "results.json").read_text()) for root in (before, after)]
    assert [r["shape"] for r in left] == [r["shape"] for r in right], "different shapes"
    for old, new in zip(left, right):
        shape = old["shape"]
        with gzip.open(before / shape / "read-results.json.gz", "rt") as source:
            expected = json.load(source)
        with gzip.open(after / shape / "read-results.json.gz", "rt") as source:
            actual = json.load(source)
        assert actual == expected, f"complete native response mismatch: {shape}"
        a = {(r["operation"], r.get("cache")): r for r in old["rows"]}
        b = {(r["operation"], r.get("cache")): r for r in new["rows"]}
        for key in a:
            if "p50_ms" in a[key] and key[1] == "warm":
                print(json.dumps({"shape": shape, "operation": key[0],
                    "before_p50_ms": a[key]["p50_ms"], "after_p50_ms": b[key]["p50_ms"],
                    "before_p95_ms": a[key]["p95_ms"], "after_p95_ms": b[key]["p95_ms"],
                    "complete_read_parity": True}))


if __name__ == "__main__":
    main()
