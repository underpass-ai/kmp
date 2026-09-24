"""Versioned adapters for the wake claim contract; chosen by declaration, never by sniffing.

The capture declares which contract its binary serves. The adapter then
validates every claim strictly: a v1 capture carrying `evidence_refs`, or a v2
capture carrying `evidence_ref`, is a contract violation, not a variant to
guess around.
"""
from dataclasses import dataclass
from typing import Protocol


class WakeClaimContract(Protocol):
    contract_id: str

    def violations(self, claim: dict) -> list: ...

    def refs(self, claim: dict) -> list: ...


@dataclass(frozen=True)
class EvidenceRefV1:
    """main a22b6402: `evidence_ref` is one string meant as a proof.evidence id."""
    contract_id: str = 'kmp.wake_claim.v1'

    def violations(self, claim):
        problems = []
        if 'evidence_refs' in claim or 'evidence' in claim:
            problems.append('v1 claim carries a v2 field')
        if 'evidence_ref' in claim and not isinstance(claim['evidence_ref'], str):
            problems.append('v1 evidence_ref is not a string')
        return problems

    def refs(self, claim):
        value = claim.get('evidence_ref')
        return [value] if isinstance(value, str) and value else []


@dataclass(frozen=True)
class EvidenceRefsV2:
    """#544 I1: `evidence_refs` ids resolved from the graph, inline `evidence` otherwise.

    A claim with neither is legal: structural links carry no proof.
    """
    contract_id: str = 'kmp.wake_claim.v2'

    def violations(self, claim):
        problems = []
        if 'evidence_ref' in claim:
            problems.append('v2 claim carries the retired evidence_ref')
        refs = claim.get('evidence_refs', [])
        if not isinstance(refs, list) or not all(isinstance(r, str) for r in refs):
            problems.append('v2 evidence_refs is not a list of strings')
        if 'evidence' in claim and not isinstance(claim['evidence'], str):
            problems.append('v2 inline evidence is not a string')
        return problems

    def refs(self, claim):
        refs = claim.get('evidence_refs', [])
        return [r for r in refs if isinstance(r, str)] if isinstance(refs, list) else []


CONTRACTS = {c.contract_id: c for c in (EvidenceRefV1(), EvidenceRefsV2())}


def contract(contract_id):
    if contract_id not in CONTRACTS:
        raise KeyError(f'unknown wake contract {contract_id!r}; declare one of {sorted(CONTRACTS)}')
    return CONTRACTS[contract_id]
