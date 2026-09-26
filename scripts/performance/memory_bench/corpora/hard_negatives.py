"""Hard negatives from a copy of the real store (BENCH_SPEC 4.2): the application.

  python3 -m scripts.performance.memory_bench.corpora.hard_negatives register --private-root P
  python3 -m scripts.performance.memory_bench.corpora.hard_negatives generate \
      --store <copy> --probe target/release/kmp_search_probe --out <private dir>
  python3 -m scripts.performance.memory_bench.corpora.hard_negatives audit-sample --out <private dir>
  python3 -m scripts.performance.memory_bench.corpora.hard_negatives audit-score --out <private dir>

`generate` reads the copy (never the live store), indexes every about through
the kernel's tokenizer, builds candidates per type and form with the
pre-registered rules, renders them, verifies each rendered question with the
probe again and writes `hard-negatives.jsonl`, `identifier-guards.jsonl`,
`verification.jsonl` and `summary.json` under --out, which must lie outside
the repository. Same store, rules, seed and probe binary give the same bytes.
"""
import argparse
import json
from pathlib import Path
import sys

from ...token_harness.domain.errors import HarnessError
from ..domain import jsonl
from ..domain.cachekey import canonical_json
from ..domain.prng import SplitMix64
from ..domain.question import questions_digest, write_questions
from . import identifier_guards, negative_audit
from .errors import NegativesShortfall
from .negative_builders import BUILDERS, FORMS, TYPES, Context
from .negative_render import build_question, render_text, verification
from .negative_rules import load_rules, register, require_registered
from .search_probe import BinaryProbe, ProbeScope
from .store_snapshot import read_store
from .term_index import build_index

PREFIX = {'anchor_neighbor_existing': 'neighbor', 'anchor_absent': 'absent',
          'near_miss_attribute': 'nearmiss', 'negated_anchor': 'negated',
          'singular_anchored_twin': 'twin', 'cross_about_anchor': 'cross'}


def candidate_word(rules):
    shortest = rules['attributes']['min_length']
    return lambda word: len(word) >= shortest


def build_context(snapshot, probe, rules):
    """Indexes of every about with enough entries, plus the probed twin lexicon."""
    indexes, texts = {}, {}
    for about in snapshot.abouts:
        if len(about.entries) >= rules['store']['min_entries'] or about.about in rules['store']['abouts']:
            indexes[about.about] = build_index(about, probe, candidate_word(rules))
            texts[about.about] = ProbeScope.of(about)
    queried = tuple(a for a in rules['store']['abouts'] if a in indexes)
    twin_words = {}
    for about in queried:
        language = 'es' if indexes[about].language == 'spanish' else 'en'
        words = sorted({w for group in rules['singular_anchored_twin'][language] for w in group})
        for word, terms in zip(words, probe.probe(texts[about], words)):
            twin_words[(about, word)] = terms.search_keys
    return Context(rules, indexes, queried, twin_words), texts


def _ordered(context, kind, form, rng):
    """Candidates of every queried about, shuffled per about and interleaved round robin."""
    lanes = []
    for about in context.queried:
        plans = BUILDERS[kind](context, about, form, rng.fork(f'build/{kind}/{form}/{about}'))
        plans = sorted(plans, key=lambda p: (p.anchor, p.attrs))
        lanes.append(rng.fork(f'order/{kind}/{form}/{about}').shuffled(plans))
    ordered = []
    for position in range(max((len(lane) for lane in lanes), default=0)):
        ordered.extend(lane[position] for lane in lanes if position < len(lane))
    return ordered


def _capped(context, plans, limit):
    anchors, attributes, chosen = {}, {}, []
    rules = context.rules
    for plan in plans:
        anchor_key = (plan.about, plan.anchor)
        if anchors.get(anchor_key, 0) >= rules['anchors']['max_per_anchor']:
            continue
        if any(attributes.get((plan.about, w), 0) >= rules['attributes']['max_per_attribute'] for w in plan.attrs):
            continue
        anchors[anchor_key] = anchors.get(anchor_key, 0) + 1
        for word in plan.attrs:
            attributes[(plan.about, word)] = attributes.get((plan.about, word), 0) + 1
        chosen.append(plan)
        if len(chosen) >= limit:
            break
    return chosen


def generate_negatives(context, texts, probe, provenance):
    """(questions, verification rows, counts) for every type and form."""
    rules, rng = context.rules, SplitMix64(context.rules['seed'])
    questions, rows, counts = [], [], {}
    for kind in TYPES:
        for form in FORMS:
            plans = _capped(context, _ordered(context, kind, form, rng), 3 * rules['per_form'])
            text_rng = rng.fork(f'text/{kind}/{form}')
            rendered = [(plan, render_text(plan, rules.templates(context.language(plan.about)), text_rng))
                        for plan in plans]
            readings = {}
            for about in context.queried:
                mine = [text for plan, text in rendered if plan.about == about]
                readings.update(zip(((about, t) for t in mine), probe.probe(texts[about], mine)))
            kept = 0
            for plan, text in rendered:
                ok, checks = verification(plan, text, readings[(plan.about, text)], context)
                keep = ok and kept < rules['per_form']
                identifier = f'hn-{PREFIX[kind]}-{form}-{kept + 1:03d}' if keep else None
                rows.append({'id': identifier, 'type': kind, 'form': form, 'about': plan.about,
                             'question': text, 'verified': ok, 'kept': keep, 'checks': checks})
                if keep:
                    language = context.language(plan.about)
                    questions.append(build_question(plan, identifier, text, language,
                                                    rules['answer_policy'], provenance))
                    kept += 1
            counts[f'{kind}/{form}'] = {'candidates': len(plans), 'kept': kept}
    return questions, rows, counts


def shortfalls(counts, minimum):
    return {name: value['kept'] for name, value in counts.items() if value['kept'] < minimum}


def run_generate(args):
    rules = load_rules()
    out = jsonl.guard_private_path(args.out)
    require_registered(rules.sha256, args.private_root or out.parent)
    probe = BinaryProbe(args.probe)
    snapshot = read_store(args.store)
    context, texts = build_context(snapshot, probe, rules)
    provenance = {'rules_version': rules['rules_version'], 'rules_sha256': rules.sha256,
                  'seed': rules['seed'], 'store_digest': snapshot.digest(), 'probe_sha256': probe.sha256,
                  'verified': True}
    negatives, rows, counts = generate_negatives(context, texts, probe, provenance)
    guards, guard_rows = identifier_guards.generate_guards(context, texts, probe, provenance)
    out.mkdir(parents=True, exist_ok=True)
    write_questions(out / 'hard-negatives.jsonl', negatives, overwrite=True)
    write_questions(out / 'identifier-guards.jsonl', guards, overwrite=True)
    jsonl.write_text(out / 'verification.jsonl', jsonl.dump_lines(rows + guard_rows), overwrite=True)
    missing = shortfalls(counts, rules['min_per_form'])
    summary = {'rules_sha256': rules.sha256, 'store_digest': snapshot.digest(), 'probe_sha256': probe.sha256,
               'seed': rules['seed'], 'negatives': counts, 'negatives_digest': questions_digest(negatives),
               'guards': identifier_guards.summarize(guards), 'guards_digest': questions_digest(guards),
               'languages': {a: context.indexes[a].language for a in context.queried},
               'shortfalls': missing}
    jsonl.write_text(out / 'summary.json', canonical_json(summary) + '\n', overwrite=True)
    print(json.dumps(summary, indent=2, sort_keys=True, ensure_ascii=False))
    if missing:
        raise NegativesShortfall(f'below {rules["min_per_form"]} verified questions: {missing}')
    return 0


def run_register(args):
    root = jsonl.guard_private_path(args.private_root)
    print(register(root))
    return 0


def build_parser():
    parser = argparse.ArgumentParser(prog='hard_negatives', description=__doc__,
                                     formatter_class=argparse.RawDescriptionHelpFormatter)
    commands = parser.add_subparsers(dest='command', required=True)
    reg = commands.add_parser('register', help='hash negatives.toml and record it (repo and private registry)')
    reg.add_argument('--private-root', type=Path, required=True)
    reg.set_defaults(action=run_register)
    gen = commands.add_parser('generate', help='generate and verify hard negatives and identifier guards')
    gen.add_argument('--store', type=Path, required=True, help='a COPY of the store directory')
    gen.add_argument('--probe', type=Path, required=True, help='kmp_search_probe binary')
    gen.add_argument('--out', type=Path, required=True, help='private output directory')
    gen.add_argument('--private-root', type=Path, help='where the rules registry lives (default: parent of --out)')
    gen.set_defaults(action=run_generate)
    negative_audit.add_commands(commands)
    return parser


def main(argv=None):
    args = build_parser().parse_args(argv)
    try:
        return args.action(args)
    except HarnessError as error:
        print(str(error), file=sys.stderr)
        return 2


if __name__ == '__main__':
    sys.exit(main())
