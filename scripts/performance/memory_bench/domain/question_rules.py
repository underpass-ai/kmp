"""Type rules of kmp.bench.question.v1: which gold each question type must carry.

The types are BENCH_SPEC section 5 plus `evidence_recall` (section 4.6: R@k
without a reader). A rule here is a structural promise a generator or a
labeler makes; scoring (BT10) relies on it instead of re-checking.
"""

TYPES = (
    'enumerative_anchored', 'singular_anchored',
    'anchor_neighbor_existing', 'anchor_absent', 'near_miss_attribute', 'negated_anchor',
    'singular_anchored_twin', 'cross_about_anchor', 'identifier_guard',
    'lookup_exact', 'rare_identifier', 'paraphrase_zero_overlap',
    'crosslang_es_en', 'crosslang_en_es', 'hard_distractor', 'multihop_why_k',
    'path_between', 'path_open', 'current_after_supersession',
    'as_of_historical', 'interval_scoped', 'hub_adjacent', 'multi_about',
    'wake_resume', 'repeat_and_determinism', 'evidence_recall',
)
# Hard negatives (section 4.2): the right answer is UNKNOWN, generated with a known cause.
NEGATIVE_TYPES = ('anchor_neighbor_existing', 'anchor_absent', 'near_miss_attribute',
                  'singular_anchored_twin', 'cross_about_anchor')
ANCHORED_TYPES = ('enumerative_anchored', 'singular_anchored', 'negated_anchor',
                  'rare_identifier', 'identifier_guard') + NEGATIVE_TYPES
PATH_TYPES = ('path_between', 'path_open')
TOOL_BY_TYPE = {'wake_resume': ('kmp_wake',), 'path_between': ('kmp_trace', 'kmp_curate'),
                'path_open': ('kmp_trace', 'kmp_curate'),
                'repeat_and_determinism': ('kmp_ask', 'kmp_wake', 'kmp_trace', 'kmp_curate')}
SHAPE_BY_TYPE = {'enumerative_anchored': 'enumerative', 'singular_anchored': 'singular',
                 'singular_anchored_twin': 'singular'}


def check_rules(question, fields):
    gold, kind = question.gold, question.type
    if kind not in TYPES:
        fields.fail('type', f'{kind!r} is not a question type')
    if question.tool not in TOOL_BY_TYPE.get(kind, ('kmp_ask',)):
        fields.fail('tool', f'{kind} is asked through {" or ".join(TOOL_BY_TYPE.get(kind, ("kmp_ask",)))}')
    if question.tool == 'kmp_ask':
        _check_ask(question, fields)
    if kind in ANCHORED_TYPES and not question.anchors:
        fields.fail('anchors', f'{kind} names the anchor(s) it asks about')
    if kind in NEGATIVE_TYPES:
        if gold.answerable != 'UNKNOWN' or gold.unknown_reason is None or not gold.absent_terms:
            fields.fail('gold', f'{kind} is UNKNOWN with an unknown_reason and absent_terms')
    if kind == 'negated_anchor' and (gold.answerable != 'KNOWN' or not gold.excluded_refs):
        fields.fail('gold', 'negated_anchor is KNOWN with the excluded refs it must not cite')
    if kind == 'multihop_why_k' and gold.chain is None:
        fields.fail('gold', 'multihop_why_k carries its chain')
    if kind in PATH_TYPES:
        _check_path(question, fields)
    if kind == 'current_after_supersession':
        if gold.current_ref is None or not gold.stale_refs or gold.current_ref not in gold.answer_refs():
            fields.fail('gold', 'current_after_supersession names current_ref (an answer) and stale_refs')
    if kind == 'as_of_historical' and question.as_of is None:
        fields.fail('as_of', 'as_of_historical stands at an instant')
    if kind == 'interval_scoped' and question.interval is None:
        fields.fail('interval', 'interval_scoped reads a span')
    if kind == 'wake_resume' and not gold.wake_required:
        fields.fail('gold', 'wake_resume lists the refs a resume must deliver')
    if gold.nearest_outside is not None and question.interval is None:
        fields.fail('gold', 'nearest_outside only exists for an interval question')


def _check_ask(question, fields):
    gold, kind = question.gold, question.type
    if gold.shape is None or not gold.facets:
        fields.fail('gold', 'a kmp_ask question has a shape and at least one facet')
    expected = SHAPE_BY_TYPE.get(kind)
    if expected is not None and gold.shape != expected:
        fields.fail('gold', f'{kind} is {expected}')
    if gold.answerable == 'KNOWN' and not gold.answer_refs():
        fields.fail('gold', 'a KNOWN question has at least one answering entry')


def _check_path(question, fields):
    path = question.gold.path
    if path is None:
        fields.fail('gold', f'{question.type} carries its path')
    if (question.type == 'path_between') != (path.end is not None):
        fields.fail('gold', 'path_between has a destination; path_open has none')
    arguments = question.arguments
    if arguments.get('from') not in (None, path.start):
        fields.fail('arguments', 'from differs from gold.path.from')
    if path.end is not None and arguments.get('to') not in (None, path.end):
        fields.fail('arguments', 'to differs from gold.path.to')
