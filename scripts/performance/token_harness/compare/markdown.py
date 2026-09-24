"""Markdown generated from report.json alone; no figure is computed anywhere else."""
PRIMARY_REPRESENTATION = 'json_compact_lexical_v1'
ORACLE_COLUMNS = ('unresolved_evidence_reference_count', 'evidence_body_disguised_as_ref_count',
                  'support_bookkeeping_in_state_count', 'support_displacement_count',
                  'historical_relation_as_action_count', 'identical_body_merged_citation_count',
                  'contract_violation_count')
SHORT = {'unresolved_evidence_reference_count': 'unresolved refs',
         'evidence_body_disguised_as_ref_count': 'body as ref',
         'support_bookkeeping_in_state_count': 'supports lines in state',
         'support_displacement_count': 'displaced memories',
         'historical_relation_as_action_count': 'relation as action',
         'identical_body_merged_citation_count': 'equal bodies merged',
         'contract_violation_count': 'contract violations'}


def _cell(value):
    if value is None:
        return '—'
    if isinstance(value, bool):
        return 'pass' if value else 'FAIL'
    if isinstance(value, float):
        return f'{value:.4f}'
    return str(value).replace('|', '\\|')


def _table(headers, rows):
    lines = ['| ' + ' | '.join(headers) + ' |', '|' + '---|' * len(headers)]
    lines += ['| ' + ' | '.join(_cell(v) for v in row) + ' |' for row in rows]
    return '\n'.join(lines)


def _header(report, source):
    base, cand = report['baseline'], report['candidate']
    tokenizer = ', '.join(f'{t["library"]} {t["library_version"]} {t["encoding"]}'
                          for t in report['tokenizers'])
    rows = [(name, side['run'], (side.get('build_provenance') or {}).get('commit'),
             side['binary_sha256'], side['wake_contract'], side['journeys'],
             len(side['capture_failures']), len(side['incomplete_journeys']))
            for name, side in (('baseline', base), ('candidate', cand))]
    summary = report['summary']
    return '\n'.join([
        f'Generated from `{source}` by `python -m scripts.performance.token_harness render`; '
        'edit the harness or rerun it, not this file.', '',
        f'- Evidence scope: `{report["evidence_scope"]}`; model calls: {report["model_calls"]}.',
        f'- Tokenizer: {tokenizer}; representations: '
        + ', '.join(f'`{r}`' for r in report['representations']) + '.',
        f'- Tasks: {summary["tasks"]}; journeys: {summary["journeys"]}; attempts per journey: '
        f'{summary["attempts_per_journey"]}; paired rows: {summary["rows"]}.',
        f'- Configuration mismatch: {report["configuration_mismatch"] or "none"}.',
        f'- Harness code SHA-256: `{report["harness_code_sha256"]}`.', '',
        _table(('variant', 'run', 'commit', 'binary SHA-256', 'wake contract', 'journeys',
                'capture failures', 'incomplete'), rows), '',
        'Limitations:', ''] + [f'- {item}' for item in report['limitations']])


def _oracle(report):
    rows = []
    for name in ('baseline', 'candidate'):
        for journey in report['oracle'][name]:
            metrics = journey['metrics']
            rows.append((journey['journey'], name, journey['quality_pass'],
                         *(metrics.get(column) for column in ORACLE_COLUMNS),
                         (metrics.get('first_supported_unit') or {}).get('after_rpc'),
                         (metrics.get('task_ready') or {}).get('after_rpc'), metrics.get('rpc_calls'),
                         ', '.join(journey['quality_failures']) or '—'))
    rows.sort(key=lambda row: (row[0], row[1]))
    return _table(('journey', 'variant', 'oracle', *(SHORT[c] for c in ORACLE_COLUMNS),
                   'first unit after rpc', 'task ready after rpc', 'rpc', 'failed checks'), rows)


def _unmet(report):
    lines = []
    for name in ('baseline', 'candidate'):
        for journey in report['oracle'][name]:
            for item in journey['unmet_obligations']:
                target = item.get('text') or item.get('local_id')
                lines.append(f'- {name} `{journey["journey"]}`: `{item["kind"]}` '
                             f'"{target}" → {_cell(item["satisfied"])}')
    return '\n'.join(lines) or '- none'


def _paired(report, representation):
    rows = [(r['case_id'], r['journey'], r['goal_id'], r['encoding'], r['baseline_tokens'],
             r['candidate_tokens'], r['delta_tokens'], r['reduction_fraction'], r['baseline_rpc'],
             r['candidate_rpc'], r['first_supported_unit']['baseline'],
             r['first_supported_unit']['candidate'], r['quality_baseline'], r['quality_candidate'],
             r['comparable'], r['conclusion'])
            for r in report['rows'] if r['representation_id'] == representation]
    return _table(('case_id', 'journey', 'goal_id', 'encoding', 'baseline_tokens', 'candidate_tokens',
                   'delta_tokens', 'reduction_fraction', 'baseline_rpc', 'candidate_rpc',
                   'first unit (base)', 'first unit (cand)', 'quality_baseline', 'quality_candidate',
                   'comparable', 'conclusion'), rows)


def _stages(report, representation):
    rows = []
    for r in report['rows']:
        if r['representation_id'] != representation:
            continue
        base, cand = r['baseline'], r['candidate']
        memory = (None if None in (base['memory_tokens'], cand['memory_tokens'])
                  else cand['memory_tokens'] - base['memory_tokens'])
        rows.append((r['journey'], base['startup_tokens'], cand['startup_tokens'], base['memory_tokens'],
                     cand['memory_tokens'], memory, base['tokens_until_first_supported_unit'],
                     cand['tokens_until_first_supported_unit'], base['tokens_until_task_ready'],
                     cand['tokens_until_task_ready']))
    return _table(('journey', 'startup (base)', 'startup (cand)', 'memory (base)', 'memory (cand)',
                   'memory delta', 'until first unit (base)', 'until first unit (cand)',
                   'until task ready (base)', 'until task ready (cand)'), rows)


def _cohorts(report):
    rows = [(c['encoding'], c['representation_id'], c['budget_group'], c['n'],
             c['excluded_without_both_counts'], c['baseline_tokens'], c['candidate_tokens'],
             c['weighted_reduction'], c['median_delta_tokens'],
             (c['worst_regression'] or {}).get('journey'), (c['worst_regression'] or {}).get('delta_tokens'),
             ', '.join(f'{k}: {v}' for k, v in c['conclusions'].items()))
            for c in report['cohorts']]
    return _table(('encoding', 'representation', 'group', 'n', 'excluded', 'baseline sum',
                   'candidate sum', 'weighted_reduction', 'median delta', 'worst journey',
                   'worst delta', 'conclusions'), rows)


def _controls(controls):
    rows = []
    for source, control in controls:
        deltas = [r['delta_tokens'] for r in control['rows']]
        rows.append((source, control['label'], control['baseline']['run'], control['candidate']['run'],
                     len(deltas), sum(1 for d in deltas if d not in (0, None)),
                     sum(1 for d in deltas if d is None)))
    return _table(('control', 'label', 'baseline run', 'candidate run', 'rows', 'non-zero deltas',
                   'rows without counts'), rows)


def render(report, source, controls=(), title='Agent token optimization — paired Wake report (#544 I3)'):
    representation = PRIMARY_REPRESENTATION
    sections = ['## A/A controls', 'The same verified artifacts compared with themselves must give '
                'zero deltas.', _controls(controls)] if controls else []
    return '\n\n'.join([
        f'# {title}', _header(report, source),
        '## Conclusions', ', '.join(f'`{k}`: {v}' for k, v in report['summary']['conclusions'].items())
        + ' (rows over every representation).',
        '## Oracle per journey', _oracle(report),
        '### Unmet obligations', _unmet(report),
        f'## Paired table (`{representation}`)',
        'Whole-journey reference tokens: startup (initialize, initialized, tools/list) plus every '
        'memory call the server directed.', _paired(report, representation),
        f'### Stage split and time to first unit (`{representation}`)', _stages(report, representation),
        '## Cohorts', 'weighted_reduction = 1 - sum(candidate) / sum(baseline) over rows with both '
        'counts; failing rows stay in the cohort.', _cohorts(report), *sections]) + '\n'
