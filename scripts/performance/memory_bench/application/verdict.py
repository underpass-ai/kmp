"""The verdict of a comparison, by the pre-registered rules of BENCH_SPEC section 12.

Inputs are the variant's pre-registration (claim, targets with effect, guards with
margin; `Variant.preregistration_digest` freezes them before the first run) and
the Deltas of `compare.py`. The rules, first match wins:

1. `no_comparable`: a single arm, anti-drift fields that differ, or a Jev replay
   that missed a recorded verdict (a guess is never scored).
2. `captura_fallida`: a journey that did not complete (tool, RPC or transport
   error), a process that could not start, or a variant whose store files the
   binary did not acknowledge.
3. `regresion`: a guard worse than its margin (margin 0 by default: false
   answers, stale serves, future leaks, core precision...), or a target whose CI95
   lies wholly on the worse side.
4. `claim = parity`: parity holds (every call identical after the documented
   volatile fields, and Δ = 0 on the targets) -> `neutral` ("same behaviour");
   otherwise `regresion`; not measured -> `indecidible`.
5. `indecidible`: a target whose MDE exceeds its pre-registered effect (principle
   3: without power there is no verdict, not even a favourable one).
6. every target's CI95 wholly on the better side: `mejora` for a quality claim
   whose cost does not worsen, `mejora_con_coste` when it does (a trade-off table
   for Tirso), `solo_coste` for a cost claim.
7. no target improved but the cost did: `solo_coste`.
8. otherwise `neutral` (the spec recommends removing the variant).

Cost worsens when the journey-token Δ has its CI95 wholly above 0 or the Jev
dollars grow; it improves in the mirror case.
"""
from . import compare

VERDICTS = ('mejora', 'mejora_con_coste', 'solo_coste', 'neutral', 'regresion', 'indecidible',
            'no_comparable', 'captura_fallida')
TOLERANCE = 1e-12


def guard_row(row, margin):
    worse = compare.worsening(row)
    broken = worse is not None and worse > margin + TOLERANCE
    return {'metric': row['metric'], 'margin': margin, 'baseline': row['baseline'],
            'candidate': row['candidate'], 'delta': row['delta'], 'worsening': worse,
            'broken': broken,
            'absent_reason': None if worse is not None else 'not measured on both arms'}


def target_row(row):
    return {**row, 'status': compare.improvement(row)}


def cost_summary(tokens_row, jev_row):
    tokens = None if tokens_row is None else compare.improvement(tokens_row)
    jev = None if jev_row is None or jev_row['delta'] is None else jev_row['delta']
    worse = tokens == -1 or (jev is not None and jev > TOLERANCE)
    better = not worse and (tokens == 1 or (jev is not None and jev < -TOLERANCE))
    return {'tokens': tokens_row, 'jev_usd': jev_row, 'worse': worse, 'better': better}


def _verdict(value, claim, targets, guards, cost, reasons):
    if value not in VERDICTS:
        raise ValueError(f'unknown verdict {value!r}')
    return {'value': value, 'claim': claim, 'targets': targets, 'guards': guards, 'cost': cost,
            'reasons': list(reasons)}


def decide(claim, targets=(), guards=(), cost=None, *, single_arm=False, drift=(),
           capture_failures=(), replay_misses=0, parity=None):
    """`targets`: target_row dicts; `guards`: guard_row dicts; `cost`: cost_summary.

    `parity`: the parity control ({parity_rate, ...}) for a parity claim."""
    targets, guards = list(targets), list(guards)

    def verdict(value, *reasons):
        return _verdict(value, claim, targets, guards, cost, reasons)

    if single_arm:
        return verdict('no_comparable', 'single arm: nothing to compare')
    if drift:
        return verdict('no_comparable', 'runs differ in ' + ', '.join(drift))
    if replay_misses:
        return verdict('no_comparable', f'{replay_misses} Jev evaluation(s) missed the replay cassette/book')
    if capture_failures:
        return verdict('captura_fallida', *capture_failures)
    broken = [g['metric'] for g in guards if g['broken']]
    if broken:
        return verdict('regresion', 'guard worse than its margin: ' + ', '.join(broken))
    worse = [t['metric'] for t in targets if t['status'] == -1]
    if worse:
        return verdict('regresion', 'target CI95 on the worse side: ' + ', '.join(worse))
    if claim == 'parity':
        if parity is None or parity.get('parity_rate') is None:
            return verdict('indecidible', 'parity claim without a parity measurement')
        moved = [t['metric'] for t in targets if t['delta'] not in (None, 0, 0.0)]
        if parity['parity_rate'] == 1.0 and not moved:
            return verdict('neutral', 'declared parity holds: every call equal, target deltas 0')
        reasons = [f'parity_rate {parity["parity_rate"]}'] + [f'{m} moved' for m in moved]
        return verdict('regresion', 'declared parity broken: ' + '; '.join(reasons))
    underpowered = [t['metric'] for t in targets if not t['decidable']]
    if underpowered:
        details = ', '.join(f'{t["metric"]} (MDE {t["mde"]}, effect {t["effect"]})'
                            for t in targets if not t['decidable'])
        return verdict('indecidible', 'MDE above the pre-registered effect: ' + details)
    improved = bool(targets) and all(t['status'] == 1 for t in targets)
    cost_worse = bool(cost and cost['worse'])
    cost_better = bool(cost and cost['better'])
    if improved and claim == 'cost':
        return verdict('solo_coste', 'cost target improved; no guard worse')
    if improved and cost_worse:
        return verdict('mejora_con_coste', 'targets improved at a higher cost: trade-off for review')
    if improved:
        return verdict('mejora', 'every target CI95 above 0; no guard or cost worse')
    if cost_better:
        return verdict('solo_coste', 'targets unchanged; cost lower')
    return verdict('neutral', 'no target moved beyond its CI95')
