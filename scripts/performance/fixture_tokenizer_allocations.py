"""Run the #766 allocation control against the MCP fixture backend."""
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / "scripts" / "performance"))
import lexical_index_allocations as runner  # noqa: E402
import temporal_body_selection as transport  # noqa: E402
from temporal_body_selection import fixture as base_fixture  # noqa: E402


def fixture(entries, body):
    seed, refs = base_fixture(entries, body)
    for entry in seed["memory"]["entries"]:
        for coordinate in entry["coordinates"]:
            coordinate.pop("valid_until", None)
    return seed, refs


def fixture_client(binary, store, trace):
    original_popen = transport.subprocess.Popen

    def spawn(args, **kwargs):
        env = dict(kwargs["env"])
        env["KMP_MCP_BACKEND"] = "fixture"
        kwargs["env"] = env
        return original_popen(args, **kwargs)

    transport.subprocess.Popen = spawn
    try:
        return transport.Client(binary, store, trace)
    finally:
        transport.subprocess.Popen = original_popen


runner.fixture = fixture
runner.Client = fixture_client
runner.main()
