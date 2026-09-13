"""Bounded non-personal graph shapes shared by native and browser controls."""
from temporal_body_selection import fixture


SHAPES = {
    "small": (8, 256, 1),
    "medium": (32, 1024, 2),
    "high-degree": (64, 1024, 8),
    "large-body": (8, 32768, 1),
}


def make_fixture(name):
    count, body_bytes, degree = SHAPES[name]
    packet, refs = fixture(count, body_bytes)
    for entry in packet["memory"]["entries"]:
        for coordinate in entry["coordinates"]:
            coordinate.pop("valid_until", None)
    # Acyclic ordered proof graph, with bounded degree and no duplicate edges.
    template = packet["memory"]["relations"][0]
    packet["memory"]["relations"] = [
        {**template, "from": refs[i], "to": refs[j]}
        for i in range(1, count) for j in range(max(0, i - degree), i)
    ]
    return packet, refs


def read_cases(about, refs):
    budget = {"max_bytes": 64000, "max_entries": 8, "detail": "full"}
    return [
        ("wake", "kmp_wake", {"about": about, "budget": budget}),
        ("ask", "kmp_ask", {"about": about, "question": "quantity 17 excludes B", "budget": budget}),
        ("goto", "kmp_goto", {"about": about, "axis": "observed",
            "at": {"ref": refs[3]}, "limit": {"entries": 4},
            "budget": {"max_bytes": 64000}}),
        ("forward", "kmp_forward", {"about": about, "axis": "observed",
            "from": {"ref": refs[1]}, "limit": {"entries": 4},
            "budget": {"max_bytes": 64000}}),
    ]
