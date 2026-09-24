"""Synthetic, public-safe scenarios (annex A.12 subset: W01-W04, A01, E01)."""
import hashlib

from . import a01, e01, w01, w02, w03, w04

SCENARIOS = tuple(module.SCENARIO for module in (w01, w02, w03, w04, a01, e01))
BY_CASE = {scenario.case_id: scenario for scenario in SCENARIOS}


def scenarios_digest(scenarios=SCENARIOS):
    """One digest over every scenario definition, in order; pairs must share it."""
    digest = hashlib.sha256()
    for scenario in scenarios:
        digest.update(scenario.case_id.encode() + b'\0' + scenario.digest().encode() + b'\0')
    return digest.hexdigest()
