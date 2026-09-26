"""`memory_bench world`: generate synth-v1 worlds, check them, calibrate them, cache them.

  python3 -m scripts.performance.memory_bench world --seed 7 --levels 1e3,1e4,1e5 \\
      --topology mono,multi [--probe PATH] [--no-probe] [--no-nested-check] [--out DIR]

One world per topology, generated up to the highest level, written to
`tmp/memory-bench/worlds/<world_key>/` (SCHEMAS.md section 1). The manifest carries the
invariant report and the calibration against the real-store reference. A world whose
invariants fail is still written (so it can be inspected) but the command exits 1.
"""
import json
import sys
import time

from ..generator import calibration, invariants, world as synth
from ..generator.probe import ProbeUnavailable, SearchProbe
from ..runtime.layout import public_layout


def build(params, probe=None, nested_check=True):
    """A generated world with its invariants and calibration filled in."""
    generated = synth.generate(params)
    started = time.perf_counter()
    questions = synth.load_questions(generated)

    def regenerate(level):
        alone = synth.WorldParams(params.seed, params.topology, (level,), params.block_size)
        return synth.generate(alone).level_digests[str(level)]

    generated.invariants = invariants.check_world(generated, questions, probe,
                                                  regenerate if nested_check else None)
    generated.timings_ms['invariants'] = round((time.perf_counter() - started) * 1000)
    started = time.perf_counter()
    if probe is None:
        generated.calibration = {'skipped': 'kmp_search_probe not available'}
    else:
        generated.calibration = calibration.world_calibration(generated, probe)
    generated.timings_ms['calibration'] = round((time.perf_counter() - started) * 1000)
    return generated


def run_world(args):
    probe = None
    if not args.no_probe:
        try:
            probe = SearchProbe.locate(args.probe)
        except ProbeUnavailable as error:
            print(f'memory_bench world: {error}', file=sys.stderr)
            return 2
    layout = public_layout(args.out)
    status = 0
    summary = {}
    for topology in args.topology:
        params = synth.WorldParams(args.seed, topology, tuple(args.levels))
        world = build(params, probe, nested_check=not args.no_nested_check)
        directory = layout.require_inside(layout.world(world.world_key))
        record = synth.write_world(world, directory)
        if not world.invariants['ok']:
            status = 1
        summary[topology] = {'directory': str(directory), 'world_digest': record['world_digest'],
                             'level_digests': record['level_digests'], 'counts': record['counts'],
                             'invariants_ok': world.invariants['ok'],
                             'timings_ms': record['timings_ms']}
    print(json.dumps(summary, indent=1, sort_keys=True))
    return status
