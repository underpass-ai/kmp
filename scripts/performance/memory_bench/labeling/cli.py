"""`memory_bench real`: the private B-real corpus, from the freeze to a run.

  B='python3 -m scripts.performance.memory_bench'
  F=~/Documents/ai/artifacts/kmp-bench-private/2026-09-26

  $B real freeze    --freeze $F --binary /path/to/kmp-mcp   adopt store/, export, pin the reference
  $B real import    --freeze $F                             real asks of the transcripts -> questions.jsonl
  $B real rules     --freeze $F                             write, hash and register labeling-rules.json
  $B real register  --freeze $F --file F --version V        freeze any other rules file by hash
  $B real pool      --freeze $F [--question ID]             build the blind pools
  $B real label     --freeze $F --labeler NAME --question ID    interactive labeling
  $B real search    --freeze $F --question ID WORDS...      free search in the question's about
  $B real status    --freeze $F                             counts only
  $B real validate  --freeze $F                             check every label file
  $B real agreement --freeze $F --labelers A,B            Cohen's κ per facet -> agreement.json
  $B real resolve   --freeze $F                             labels -> gold.jsonl (kmp.bench.question.v1)
  $B real run       --freeze $F --variant variants/X.toml  run gold.jsonl on the frozen store

Every file it writes lands inside the freeze directory (or the private cache
beside it) and never inside the repository. Output on the terminal of the
non-interactive commands is aggregates and ids only.
"""
import argparse
from collections import Counter
import itertools
import json
from pathlib import Path
import shutil
import sys

from ...token_harness.domain.errors import HarnessError
from ..corpora import real_freeze as RF
from ..corpora import real_private as RP
from ..corpora.store_snapshot import read_store
from ..domain import cachekey, jsonl
from ..domain.errors import BenchError
from . import agreement as AG
from . import labels as LB
from . import pool as PL
from .schema import SCHEMA_PATH, load_schema
from .session import LabelingSession, search

RULES_SCHEMA = 'kmp.bench.breal.rules.v1'
RULES_VERSION = 'breal-labeling.v1'
DEFAULT_SEED = 7
POOL_DEFAULTS = {'kernel_top': 30, 'random': 10, 'answer_policy': 'best_effort', 'max_calls': 8,
                 'budget': {'max_entries': 30, 'max_bytes': 100000, 'detail': 'balanced'},
                 'anchor_match': 'written occurrence (anchor_forms.Anchor.strict_pattern) in the entry texts'}
AGREEMENT_DEFAULTS = {'threshold': 0.6, 'min_questions': 30, 'unit': 'facet_entry'}
FROZEN_CODE = ('corpora/real_private.py', 'labeling/pool.py', 'labeling/labels.py', 'labeling/agreement.py',
               'labeling/gold_schema.json')
BENCH_DIR = Path(__file__).resolve().parents[1]


def _freeze(args):
    return RF.FreezeDir.at(args.freeze)


def _print(value):
    print(json.dumps(value, indent=2, sort_keys=True, ensure_ascii=False))


def _work(fd):
    return fd.private_root / 'cache' / 'work'


# --- freeze, import, rules ---------------------------------------------------------------------

def cmd_freeze(args):
    fd = _freeze(args)
    manifest = RF.freeze(fd, args.binary, _work(fd), args.source_note)
    _print({'freeze': str(fd.root), 'kernel_sha256': manifest['store']['kernel_sha256'],
            'content_digest': manifest['bundle']['content_digest'], 'integrity': manifest['store']['integrity_check'],
            'reference': manifest['reference']})
    return 0


def cmd_import(args):
    fd = _freeze(args)
    validation_from = None
    if args.validation_after:
        first = RF.first_registration(fd.private_root, rules_version=args.validation_after)
        if first is None:
            raise RF.FreezeInvalid(f'rules {args.validation_after} are not registered yet')
        validation_from = first['registered_at']
    claude, codex = RP.transcript_paths(args.home)
    fresh = RP.collect(list(RP.read_claude(claude)) + list(RP.read_codex(codex)), args.since, args.until,
                       validation_from)
    existing = RP.load_intake(fd.questions)
    merged = RP.merge(existing, fresh)
    fd.write_text(fd.questions, RP.dump_intake(merged))
    _print({'questions': len(merged), 'added': len(merged) - len(existing),
            'labelable': sum(1 for r in merged if r.status == 'labelable'),
            'by_split': dict(Counter(r.split for r in merged))})
    return 0


def rules_document(manifest, seed=DEFAULT_SEED):
    return {'schema': RULES_SCHEMA, 'rules_version': RULES_VERSION, 'seed': seed, 'pool': POOL_DEFAULTS,
            'agreement': AGREEMENT_DEFAULTS,
            'intake': {'anchor_rule': RP.ANCHOR_RULE, 'template_rule': RP.TEMPLATE_RULE,
                       'type_rule': LB.TYPE_RULE, 'default_answer_policy': LB.DEFAULT_POLICY},
            'reference': {'sha256': manifest['reference']['sha256'], 'version': manifest['reference']['version']},
            'store': {'kernel_sha256': manifest['store']['kernel_sha256'],
                      'content_digest': manifest['bundle']['content_digest']},
            'gold_schema_sha256': RF.sha256_file(SCHEMA_PATH),
            'code_sha256': {name: RF.sha256_file(BENCH_DIR / name) for name in FROZEN_CODE}}


def rules_bytes(document):
    return (json.dumps(document, indent=2, sort_keys=True, ensure_ascii=False) + '\n').encode('utf-8')


def cmd_rules(args):
    fd = _freeze(args)
    data = rules_bytes(rules_document(RF.verify(fd), args.seed))
    sha = cachekey.sha256_hex(data)
    if fd.rules.exists() and fd.rules.read_bytes() != data:
        raise RF.FreezeInvalid(f'{fd.rules} exists with other content: labeling rules are frozen once '
                               'registered; start a new dated freeze to change them')
    if not fd.rules.exists():
        fd.inside(fd.rules).write_bytes(data)
    entry = RF.first_registration(fd.private_root, rules_sha256=sha) or RF.register(fd.private_root, sha, RULES_VERSION)
    _print({'rules_sha256': sha, 'registered_at': entry['registered_at'], 'rules_version': RULES_VERSION})
    return 0


def cmd_register(args):
    fd = _freeze(args)
    sha = RF.sha256_file(args.file)
    entry = RF.first_registration(fd.private_root, args.version, sha) or RF.register(fd.private_root, sha, args.version)
    _print(entry)
    return 0


def load_rules(fd):
    """The frozen rules, refused unless registered and consistent with the freeze."""
    try:
        data = fd.rules.read_bytes()
    except OSError as failure:
        raise RF.FreezeInvalid('no labeling-rules.json: run `real rules` first') from failure
    sha = cachekey.sha256_hex(data)
    if RF.first_registration(fd.private_root, rules_sha256=sha) is None:
        raise RF.FreezeInvalid(f'labeling rules {sha[:12]} are not registered')
    rules = json.loads(data)
    manifest = RF.verify(fd)
    if rules['reference']['sha256'] != manifest['reference']['sha256'] \
            or rules['store']['kernel_sha256'] != manifest['store']['kernel_sha256']:
        raise RF.FreezeInvalid('labeling rules pin another reference binary or store')
    if rules['gold_schema_sha256'] != RF.sha256_file(SCHEMA_PATH):
        raise RF.FreezeInvalid('gold_schema.json changed since the rules were frozen')
    return rules, sha


# --- pools -------------------------------------------------------------------------------------

def _intakes(fd, wanted=None):
    records = [r for r in RP.load_intake(fd.questions) if r.status == 'labelable' and r.duplicate_of is None]
    if wanted:
        missing = set(wanted) - {r.id for r in records}
        if missing:
            raise RP.IntakeInvalid(f'not labelable or unknown: {sorted(missing)}')
        records = [r for r in records if r.id in set(wanted)]
    return records


class KernelCaller:
    """kmp_ask on the pinned reference binary over a disposable copy of the frozen store."""

    def __init__(self, fd, out_dir):
        from ..runtime.session import BenchProcess, ProcessConfig
        config = ProcessConfig(fd.reference, _work(fd), 'breal-pool', template=fd.data_dir,
                               probe_resources=False)
        self.process = BenchProcess(config, out_dir, 0, itertools.count(1))

    def __call__(self, tool, arguments):
        with self.process.journey(None) as tap:
            response = tap.call(tool, arguments)
        return (response.get('result') or {}).get('structuredContent')

    def close(self):
        self.process.close()


def cmd_pool(args):
    fd = _freeze(args)
    rules, sha = load_rules(fd)
    pool_rules = PL.PoolRules.from_rules(rules)
    snapshot = read_store(fd.data_dir)
    records = _intakes(fd, args.question)
    todo = [r for r in records if args.force or not fd.pool(r.id).exists()]
    out_dir = _work(fd) / f'pool-traces-{sha[:12]}'
    if out_dir.exists():
        shutil.rmtree(out_dir)
    out_dir.mkdir(parents=True)
    caller = KernelCaller(fd, out_dir) if todo else None
    counts = Counter()
    try:
        for record in todo:
            about = snapshot.about(record.about)
            known = {doc.ref for doc in about.entries}
            kernel = PL.kernel_top(caller, record.call, pool_rules, known)
            pool = PL.build_pool(record.id, about, record.anchors, kernel, pool_rules, sha)
            fd.write_json(fd.pool(record.id), pool.as_dict())
            fd.write_json(fd.provenance(record.id), pool.provenance())
            counts.update({key: value for key, value in pool.counts.items()})
            counts['pools'] += 1
    finally:
        if caller is not None:
            caller.close()
        shutil.rmtree(out_dir, ignore_errors=True)
    _print({'built': counts.pop('pools', 0), 'skipped_existing': len(records) - len(todo),
            'entries': dict(counts)})
    return 0


def _pool(fd, question_id):
    try:
        return PL.parse_pool(json.loads(fd.pool(question_id).read_text(encoding='utf-8')), question_id)
    except OSError as failure:
        raise PL.PoolFailed(f'no pool for {question_id}: run `real pool` first') from failure


# --- labeling ----------------------------------------------------------------------------------

def _intake(fd, question_id):
    records = _intakes(fd, [question_id])
    return records[0]


def cmd_label(args):
    fd = _freeze(args)
    rules, sha = load_rules(fd)
    intake = _intake(fd, args.question)
    pool = _pool(fd, intake.id)
    about = read_store(fd.data_dir).about(intake.about)
    known = {doc.ref for doc in about.entries}
    target = fd.label(args.labeler, intake.id)
    previous = LB.load_record(target) if target.exists() else None
    schema = load_schema()

    def save(gold, notes, searches):
        record = LB.label_record(intake.id, args.labeler, RF.now_utc(), pool.digest(), sha, gold, notes, searches)
        LB.check_label(record, intake, known, pool.digest(), sha, schema)
        return fd.write_json(target, record)

    session = LabelingSession(intake, pool, about, previous)
    session.show()
    lines = Path(args.script).read_text(encoding='utf-8').splitlines() if args.script else _prompt_lines()
    session.run(lines, save)
    return 0


def _prompt_lines():
    while True:
        try:
            yield input('label> ')
        except EOFError:
            return


def cmd_search(args):
    fd = _freeze(args)
    about_name = args.about or _intake(fd, args.question).about
    about = read_store(fd.data_dir).about(about_name)
    docs = {doc.ref: doc for doc in about.entries}
    for number, ref in enumerate(search(about.entries, ' '.join(args.words)), start=1):
        print(f'f{number}. {ref}\n     {" ".join(docs[ref].text.split())[:300]}')
    return 0


def _labels(fd, schema=None):
    """{question_id: {labeler: Label}} for every label file, validated."""
    rules, sha = load_rules(fd)
    snapshot = read_store(fd.data_dir)
    intakes = {r.id: r for r in _intakes(fd)}
    schema = schema or load_schema()
    out = {}
    for labeler in fd.labelers():
        for path in sorted((fd.root / 'labels' / labeler).glob('*.json')):
            question_id = path.stem
            if question_id not in intakes:
                raise LB.LabelInvalid(f'{labeler}/{path.name}: not a labelable question')
            intake, pool = intakes[question_id], _pool(fd, question_id)
            known = {doc.ref for doc in snapshot.about(intake.about).entries}
            label = LB.check_label(LB.load_record(path), intake, known, pool.digest(), sha, schema)
            if label.labeler != labeler:
                raise LB.LabelInvalid(f'{labeler}/{path.name} names labeler {label.labeler}')
            out.setdefault(question_id, {})[labeler] = label
    return out, intakes, sha


def cmd_validate(args):
    fd = _freeze(args)
    labels, intakes, _ = _labels(fd)
    per_labeler = Counter(labeler for by in labels.values() for labeler in by)
    _print({'labels': sum(per_labeler.values()), 'by_labeler': dict(per_labeler),
            'questions_labelable': len(intakes), 'questions_labeled': len(labels)})
    return 0


def cmd_status(args):
    fd = _freeze(args)
    records = RP.load_intake(fd.questions)
    pools = len(list((fd.root / 'pools').glob('*.json'))) if (fd.root / 'pools').is_dir() else 0
    labels = {labeler: len(list((fd.root / 'labels' / labeler).glob('*.json'))) for labeler in fd.labelers()}
    _print({'questions': len(records), 'labelable': sum(1 for r in records if r.status == 'labelable'
                                                         and r.duplicate_of is None),
            'pools': pools, 'labels': labels, 'rules': fd.rules.exists(), 'gold': fd.gold.exists()})
    return 0


def cmd_agreement(args):
    fd = _freeze(args)
    first, second = args.labelers.split(',')
    labels, _, sha = _labels(fd)
    rules = json.loads(fd.rules.read_bytes())
    paired = [(qid, by[first].gold, by[second].gold, _pool(fd, qid).refs)
              for qid, by in sorted(labels.items()) if first in by and second in by]
    report, disagreeing = AG.agreement_report(paired, (first, second), rules['agreement']['threshold'],
                                              rules['agreement']['min_questions'], sha)
    fd.write_json(fd.agreement, report)
    fd.write_json(fd.path('labels', 'disagreements.json'),
                  {'labelers': [first, second], 'question_ids': disagreeing})
    _print(report)
    return 0 if report['gate']['status'] == 'pass' else 1


def cmd_resolve(args):
    fd = _freeze(args)
    labels, intakes, sha = _labels(fd)
    questions, how = [], Counter()
    for qid, intake in intakes.items():
        by = labels.get(qid, {})
        adjudicated = by.pop(LB.ADJUDICATOR, None)
        resolution = LB.resolve_one(list(by.values()), adjudicated)
        how[resolution.how] += 1
        if resolution.gold is not None:
            questions.append(LB.to_question(intake, resolution.gold, sha, resolution.how))
    from ..domain.question import write_questions
    write_questions(fd.gold, questions, overwrite=True)
    _print({'resolved': len(questions), 'by_resolution': dict(how)})
    return 0


# --- the runner --------------------------------------------------------------------------------

def arm_for(fd, variant_path, repo_root=jsonl.REPO_ROOT):
    """One arm on the frozen store: the variant's binary, the freeze as store template."""
    from ..application.run_questions import Arm, StoreRef
    from ..domain.variant import load_variant
    variant = load_variant(variant_path, repo_root)
    if variant.binary.path is None:
        raise BenchError(f'{variant_path}: git_ref binaries are built by quick-a (BT12); give a path')
    binary = Path(variant.binary.path)
    binary = binary if binary.is_absolute() else Path(repo_root) / binary
    manifest = RF.verify(fd)
    source = cachekey.bundle_source(content_digest=manifest['bundle']['content_digest'],
                                    label=f'b-real {fd.root.name}')
    key = cachekey.store_key(source=source, n=manifest['bundle']['event_count'],
                             bundle_format=manifest['bundle']['bundle_format'],
                             reader_sha256=RF.sha256_file(binary), build='copy')
    store = StoreRef(key, manifest['bundle']['content_digest'], f'b-real {fd.root.name}', fd.data_dir)
    return Arm(variant, str(variant_path), binary.resolve(), store)


def cmd_run(args):
    from ..application.run_questions import RunPlan, run_arms
    from ..domain.question import load_questions
    from ..runtime.layout import private_layout
    fd = _freeze(args)
    questions = tuple(q for q in load_questions(fd.gold) if args.split in (None, 'all')
                      or f'split:{args.split}' in q.tags)
    arms = [arm_for(fd, path) for path in args.variant]
    plan = RunPlan(questions, 'real', samples=args.samples, repeats=args.repeats, max_bytes=args.max_bytes,
                   cpus=args.cpus)
    results = run_arms(arms, plan, private_layout(fd.private_root))
    _print([{'variant': r.name, 'run_id': r.run_id, 'cached': r.cached, 'questions': len(questions),
             'failures': len(r.manifest.get('failures') or [])} for r in results])
    return 0


# --- parser ------------------------------------------------------------------------------------

def add_commands(commands):
    def command(name, action, help_text):
        parser = commands.add_parser(name, help=help_text)
        parser.set_defaults(real_action=action)
        parser.add_argument('--freeze', type=Path, required=True, help='the dated private freeze directory')
        return parser
    freeze = command('freeze', cmd_freeze, 'adopt store/, export the bundle, pin the reference binary')
    freeze.add_argument('--binary', type=Path, required=True)
    freeze.add_argument('--source-note', help='where the store copy came from (free text)')
    imp = command('import', cmd_import, 'real asks of the local transcripts -> questions.jsonl')
    imp.add_argument('--since', type=_date, default=RP.DEFAULT_SINCE)
    imp.add_argument('--until', type=_date)
    imp.add_argument('--home', type=Path, help='read transcripts under this home (default: $HOME)')
    imp.add_argument('--validation-after', metavar='RULES_VERSION',
                     help='asks after the first registration of these rules go to the validation split')
    rules = command('rules', cmd_rules, 'write, hash and register labeling-rules.json')
    rules.add_argument('--seed', type=int, default=DEFAULT_SEED)
    register = command('register', cmd_register, 'register the hash of any rules file (e.g. the P4 gate)')
    register.add_argument('--file', type=Path, required=True)
    register.add_argument('--version', required=True)
    pool = command('pool', cmd_pool, 'build the blind pools')
    pool.add_argument('--question', action='append')
    pool.add_argument('--force', action='store_true', help='rebuild existing pools')
    label = command('label', cmd_label, 'interactive labeling of one question')
    label.add_argument('--labeler', required=True)
    label.add_argument('--question', required=True)
    label.add_argument('--script', type=Path, help='read commands from a file instead of the terminal')
    find = command('search', cmd_search, 'free text search in an about')
    target = find.add_mutually_exclusive_group(required=True)
    target.add_argument('--question')
    target.add_argument('--about')
    find.add_argument('words', nargs='+')
    command('status', cmd_status, 'counts only')
    command('validate', cmd_validate, 'check every label file')
    agree = command('agreement', cmd_agreement, "Cohen's kappa per facet -> agreement.json")
    agree.add_argument('--labelers', required=True, help='A,B')
    command('resolve', cmd_resolve, 'labels -> gold.jsonl')
    run = command('run', cmd_run, 'run gold.jsonl against one or two variants on the frozen store')
    run.add_argument('--variant', type=Path, action='append', required=True)
    run.add_argument('--split', choices=('development', 'validation', 'all'), default='all')
    run.add_argument('--samples', type=int, default=1)
    run.add_argument('--repeats', type=int, default=1)
    run.add_argument('--max-bytes', type=int)
    run.add_argument('--cpus', default=None, help='cpu list for latency (default: no pinning)')


def _date(text):
    import datetime
    return datetime.date.fromisoformat(text)


def build_parser():
    parser = argparse.ArgumentParser(prog='memory_bench real', description=__doc__,
                                     formatter_class=argparse.RawDescriptionHelpFormatter)
    add_commands(parser.add_subparsers(dest='real_command', required=True))
    return parser


def main(argv=None):
    args = build_parser().parse_args(argv)
    try:
        return args.real_action(args)
    except HarnessError as error:
        print(str(error), file=sys.stderr)
        return 2


if __name__ == '__main__':
    sys.exit(main())
