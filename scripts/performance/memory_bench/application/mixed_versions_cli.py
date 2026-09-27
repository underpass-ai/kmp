"""`mixed` verb (BT18): wire real binaries and a cached store into `mixed_versions`.

  python3 -m scripts.performance.memory_bench.application.mixed_versions_cli \
      --reference PATH --candidate target/release/kmp-mcp [--old PATH] [--scenario NAME ...]

The CLI owner can mount it with `add_arguments(subparser)`; the subparser's
`action` is then `run_command(args)`, which returns the exit code (0 when no
scenario failed, 1 otherwise). Everything is written under
`tmp/memory-bench/bt18/<run>/`: `report.json`, one trace and one stderr per
process. The store is a copy of a cached synth-v1 template (runtime/store_cache);
an old binary is copied into the run directory and run from there, never in place.
"""
import argparse
from datetime import datetime, timezone
from pathlib import Path
import shutil
import sys
import tempfile

from ...token_harness.native.capture import sha256_file
from ..domain.errors import BenchError
from ..runtime import jev_fixture, jev_stand_in, server_log, shared_store
from ..runtime.layout import public_layout
from ..runtime.store_cache import StoreCache
from . import mixed_versions as mv

DEFAULT_STORE_LABEL = 'synth-v1 seed 7 mono 1000 B1000'
RUN_DIR = 'bt18'
# The book scenario's store: Jev on (replayed, never the network) and ask re-ranking on,
# with the margin gate off (`margin_tenths: null`, DESIGN L4 4c). With the gate at its
# measured default, an ask whose first lexical candidate leads with `High` confidence sends
# no request, which on the synth store is every warm-up question: the warm-up judged
# nothing and the scenario never reached the book. The scenario tests the book, not the gate.
RERANK_WITHOUT_MARGIN_GATE = b'{"pool_size":40,"margin_tenths":null}'
JEV_STORE_FILES = {
    'typesafe.json': b'{"endpoint":"https://api.typesafe.ai/v1/systemone","model":"'
                     + jev_fixture.DEFAULT_MODEL.encode() + b'","timeout_ms":20000}',
    'rerank.json': RERANK_WITHOUT_MARGIN_GATE,
}
STAND_IN = 'stand-in.cassette.json'
LEXICAL_INDEX_ENV = 'KMP_LEXICAL_INDEX'


class MixedSetupError(BenchError):
    code = 'MIXED_SETUP_FAILED'


def binary_ref(role, path, probe=shared_store.declares, version=shared_store.binary_version):
    path = Path(path).resolve()
    if not path.is_file():
        raise MixedSetupError(f'{role} binary {path} not found')
    return mv.BinaryRef(role, path, sha256_file(path), version(path),
                        probe(path, mv.CAPABILITY_MARKERS))


def find_store(cache, label=DEFAULT_STORE_LABEL, key=None):
    """The cached template to seed stores from: by key, else the first entry with `label`."""
    if key:
        entry = cache.get(key)
        if entry is None:
            raise MixedSetupError(f'no cached store {key}')
        return entry
    for _, entry, _ in cache.entries():
        if entry is not None and entry.record.label == label:
            return entry
    raise MixedSetupError(f'no cached store labelled {label!r}: run `memory_bench build` first')


def store_factory(entry, work_dir, out_dir):
    counter = iter(range(1, 10_000))

    def new_store(label, seeded=True, jev=False, lexical=False):
        folder = out_dir / f'{next(counter):02d}-{label}'
        template = entry.template if seeded else None
        # The lexical sidecar is off by default; `lexical` opens it in shadow.
        lexical_env = {LEXICAL_INDEX_ENV: 'shadow'} if lexical else {}
        if not jev:
            return shared_store.SharedStore(work_dir, folder, label, template=template,
                                            env=lexical_env)
        # Jev answers from a stand-in cassette in replay mode, behind the book: it
        # starts empty and `prepare_jev` fills it from a warm-up process.
        scratch = Path(tempfile.mkdtemp(prefix='bt18-jev-', dir=work_dir))
        stand_in = scratch / STAND_IN
        jev_fixture.empty_cassette(stand_in, jev_fixture.DEFAULT_MODEL)
        env = {jev_fixture.CASSETTE_ENV: str(stand_in), jev_fixture.CASSETTE_MODE_ENV: 'replay',
               'RUST_LOG': server_log.BENCH_LOG_FILTER, **lexical_env}
        try:
            store = shared_store.SharedStore(work_dir, folder, label, template=template, env=env,
                                             store_files=JEV_STORE_FILES)
        except BaseException:
            shutil.rmtree(scratch, ignore_errors=True)
            raise
        store.owned = (scratch,)
        return store
    return new_store


def prepare_jev(store, binary, settings):
    """Fill the store's stand-in cassette with what `binary` asks for `settings.questions`.

    A warm-up process on a fork asks every question behind the still-empty
    cassette; each judgement misses and its telemetry names the request. The
    stand-in then answers those requests, so the scenario's processes judge
    offline and the book records what they judged.
    """
    stand_in = Path(store.env[jev_fixture.CASSETTE_ENV])
    warm = store.fork('book-warm-up')
    try:
        server = warm.open(binary, 'book-warm-up')
        for question in settings.questions:
            mv.ask(server, settings.about, question)
        events = server.close().log_events
    finally:
        warm.close()
    asked = jev_stand_in.misses(events)
    if not asked:
        raise MixedSetupError('the warm-up judged nothing: rerank.json was not applied')
    jev_stand_in.write_stand_in(stand_in, asked)


def add_arguments(parser):
    parser.add_argument('--reference', required=True, help='the v0.23.0 release binary')
    parser.add_argument('--candidate', default='target/release/kmp-mcp', help='vN (the worktree build)')
    parser.add_argument('--old', help='an older binary (read-only use; copied before running)')
    parser.add_argument('--store-key', help='cached store key (default: first 10^3 mono B1000)')
    parser.add_argument('--store-label', default=DEFAULT_STORE_LABEL)
    parser.add_argument('--scenario', action='append', choices=mv.SCENARIOS)
    parser.add_argument('--writes', type=int, default=mv.Settings.writes)
    parser.add_argument('--reads', type=int, default=mv.Settings.reads)
    parser.add_argument('--out', help='run directory (default tmp/memory-bench/bt18/<utc time>)')
    parser.set_defaults(action=run_command)
    return parser


def run_command(args):
    layout = public_layout()
    stamp = datetime.now(timezone.utc).strftime('%Y%m%dT%H%M%SZ')
    out = layout.require_inside(Path(args.out) if args.out else layout.root / RUN_DIR / stamp)
    out.mkdir(parents=True, exist_ok=False)
    old = None
    if args.old:
        copied = out / 'bin' / Path(args.old).name
        copied.parent.mkdir()
        shutil.copy2(args.old, copied)
        old = binary_ref('old', copied)
    candidate = binary_ref('candidate', args.candidate)
    env = mv.Environment(
        new_store=store_factory(find_store(StoreCache(layout), args.store_label, args.store_key),
                                layout.scratch(), out),
        reference=binary_ref('reference', args.reference), candidate=candidate, old=old,
        jev_available=mv.BOOK in candidate.capabilities, prepare_jev=prepare_jev)
    settings = mv.Settings(writes=args.writes, reads=args.reads)
    result = mv.run(env, settings, args.scenario,
                    progress=lambda r: print(mv.render_line(r), file=sys.stderr, flush=True))
    (out / 'report.json').write_text(mv.dumps(result), encoding='utf-8')
    print(out / 'report.json')
    return 1 if result['counts'][mv.FAIL] else 0


def main(argv=None):
    parser = add_arguments(argparse.ArgumentParser(prog='memory_bench mixed', description=__doc__,
                                                   formatter_class=argparse.RawDescriptionHelpFormatter))
    args = parser.parse_args(argv)
    try:
        return args.action(args)
    except BenchError as failure:
        print(f'memory_bench mixed: {failure}', file=sys.stderr)
        return 2


if __name__ == '__main__':
    sys.exit(main())
