"""summary.md of a mode run, from its summary.json alone (deterministic, aggregates only)."""
from .markdown import freeze_line

HEADLINE = (('useful_rate', 'useful'), ('false_answer_rate', 'false answer'),
            ('false_unknown_rate', 'false UNKNOWN'), ('recall_at_5', 'R@5'), ('core_precision', 'core precision'))
JUDGED_ROWS = ('recall_at_1', 'mean_reciprocal_rank', 'ndcg_at_10', 'answer_core_precision',
               'false_unknown_rate', 'all_recall_at_1', 'guarded_false_answer_rate',
               'guarded_high_false_answer_rate', 'ask_mrr_plain', 'ask_mrr_rerank',
               'path_found_declared_only', 'path_found_with_jev', 'wake_all_pages_plain',
               'wake_all_pages_focused')


def short(value):
    return '—' if not value else str(value)[:12]


def number(value, digits=4):
    if value is None:
        return '—'
    if isinstance(value, float):
        return f'{value:.{digits}f}'
    return str(value)


def _arm(label, arm):
    if arm is None:
        return [f'| {label} | replica of the baseline (fresh processes, same binary and configuration) | | | |']
    provenance = arm.get('binary_provenance') or {}
    source = provenance.get('source')
    where = provenance.get('git_ref') or provenance.get('path') or 'outside the checkout'
    commit = short(provenance.get('git_commit'))
    dirty = ' (dirty tree)' if provenance.get('dirty') else ''
    return [f'| {label} | `{arm["variant"]}` ({arm["claim"]}, jev {arm["jev"]}) | `{short(arm["binary_sha256"])}` '
            f'{arm.get("binary_version") or ""} | {source} `{where}` @ `{commit}`{dirty} '
            f'| `{short(arm["config_digest"])}` |']


def _headline_cell(section, metric):
    rows = section.get('headline') or {}
    base = (rows.get('baseline') or {}).get(metric)
    cand = (rows.get('candidate') or {}).get(metric)
    if base is None and cand is None:
        return '—'
    return f'{number(base, 3)} → {number(cand, 3)}'


def _sections(summary):
    lines = ['| Section | Status | Verdict | Questions | ' + ' | '.join(label for _, label in HEADLINE)
             + ' | Parity | Seconds | Report | Freeze |',
             '|---|---|---|---|' + '---|' * len(HEADLINE) + '---|---|---|---|']
    for section in summary['sections']:
        questions = sum((section.get('questions') or {}).values()) or '—'
        parity = section.get('parity') or {}
        rate = parity.get('parity_rate')
        parity_cell = '—' if not parity else f'{number(rate, 4)} ({parity.get("different", 0)} differ)'
        report = f'{section["layout"]} `{short(section["report_key"])}`' if section.get('report_key') else '—'
        lines.append(f'| {section["name"]} | {section["status"]} | {section.get("verdict") or "—"} | {questions} | '
                     + ' | '.join(_headline_cell(section, metric) for metric, _ in HEADLINE)
                     + f' | {parity_cell} | {section["seconds"]:.1f} | {report} | {_freeze_cell(section)} |')
    notes = [f'- **{s["name"]}** {s["status"]}: {s["reason"]}' for s in summary['sections'] if s.get('reason')]
    for section in summary['sections']:
        aa = section.get('aa')
        if aa:
            moved = ', '.join(aa.get('moved') or ()) or 'none'
            notes.append(f'- **{section["name"]}** A/A: quality deltas 0: {_yes(aa.get("quality_delta_zero"))}; '
                         f'token deltas 0: {_yes(aa.get("token_delta_zero"))} raw, '
                         f'{_yes(aa.get("token_delta_zero_normalized"))} with handles normalized; '
                         f'moved: {moved}')
    return lines + ([''] + notes if notes else [])


def _freeze_cell(section):
    freeze = section.get('freeze')
    if not freeze:
        return '—'
    return (f'`{freeze["path"]}` store `{short(freeze["store_digest"].removeprefix("sha256:"))}` '
            f'questions `{short(freeze["questions_digest"])}`')


def _yes(value):
    return '—' if value is None else ('yes' if value else 'no')


def _judged(summary):
    lines = []
    for section in summary['sections']:
        for row in section.get('judged') or ():
            recorded = row['recorded']
            held = {arm: ('—' if check is None else ('holds' if check['holds'] else
                                                     'fails ' + ', '.join(check['failing'])))
                    for arm, check in recorded.items()}
            checked = next((c for c in recorded.values() if c), None)
            scope = '' if checked is None else (f' ({checked["rows"]} rows of `{checked["file"]}`'
                                                + (f', {checked["not_measured"]} not ported' if checked['not_measured']
                                                   else '') + ')')
            lines += [f'### {row["arm"]}', '',
                      f'Recorded baseline{scope}: baseline {held["baseline"]}; candidate {held["candidate"]}. '
                      f'Cached: baseline {row["cached"]["baseline"]}, candidate {row["cached"]["candidate"]}. '
                      f'Stores whose files could not be acknowledged: baseline '
                      f'{row["unverified_stores"]["baseline"]}, candidate {row["unverified_stores"]["candidate"]}.', '',
                      '| Row | Baseline | Candidate | Δ |', '|---|---|---|---|']
            names = [n for n in JUDGED_ROWS if n in row['deltas']] or sorted(row['deltas'])
            moved = sorted(n for n, d in row['deltas'].items() if d and n not in names)
            for name in names + moved:
                lines.append(f'| {name} | {number(row["baseline"][name])} | {number(row["candidate"][name])} '
                             f'| {number(row["deltas"][name])} |')
            lines.append('')
    return lines or ['No judged corpus in this mode.']


PUBLIC_ROWS = ('recall_at_1', 'recall_at_5', 'mrr', 'full_chain_at_5', 'session_recall_at_5',
               'stale_served', 'successor_rescued', 'abstained', 'unknown')


def _public(summary):
    """One table per public corpus the `public` section ran; a skipped corpus says why."""
    lines = []
    for section in summary['sections']:
        for row in section.get('public') or ():
            if row['status'] != 'ran':
                lines += [f'- **{row["corpus"]}** {row["status"]}: {row["reason"]}']
                continue
            base, cand = row['arms']['baseline']['headline'], row['arms']['candidate']['headline']
            for name in sorted(base):
                lines += ['', f'### {name}', '', '| Figure | Baseline | Candidate | Δ |', '|---|---|---|---|']
                shift = row['delta'].get(name) or {}
                for figure in ('questions',) + PUBLIC_ROWS:
                    a, b = base[name].get(figure), (cand.get(name) or {}).get(figure)
                    if a is None and b is None:
                        continue
                    lines.append(f'| {figure} | {number(a)} | {number(b)} | {number(shift.get(figure))} |')
    return lines


def _limitations(summary):
    notes = sorted({note for s in summary['sections'] for note in s.get('limitations') or ()})
    return [f'- {note}' for note in notes] or ['None recorded.']


def render(summary):
    timings = summary['timings']
    budget = timings['budget_s']
    within = timings['within_budget']
    budget_text = 'no budget' if budget is None else (f'budget {budget:.0f} s, '
                                                      f'{"within" if within else "OVER"} budget')
    verdict = summary['verdict']
    lines = [f'# KMP memory bench: {summary["mode"]}', '',
             f'Summary `{short(summary["summary_key"])}` · bench `{summary["bench_version"]}` · modes '
             f'`{short(summary["modes_sha256"])}` · code `{short(summary["generated_by"]["code_sha256"])}` · '
             f'started {timings["started_at"]}', '',
             f'**Verdict: `{verdict["value"]}`** in {timings["total_s"]:.1f} s ({budget_text}).', '']
    lines += [f'- {reason}' for reason in verdict['reasons']] + ['']
    lines += [f'B-real freeze: {freeze_line(summary.get("freeze"), "none: no section of this mode read one")}.',
              '']
    lines += ['## Arms', '', '| Arm | Variant | Binary | Provenance | Config |', '|---|---|---|---|---|']
    lines += _arm('baseline', summary['arms']['baseline']) + _arm('candidate', summary['arms']['candidate'])
    parameters = summary['parameters']
    lines += ['', 'Parameters (modes.toml): ' + ', '.join(f'{k} {parameters[k]}' for k in sorted(parameters)), '']
    lines += ['## Sections', '', 'Headline cells read baseline → candidate. Each run section has its own '
              '`report.json` and `report.md` under `reports/<key>/` of the named cache.', '']
    lines += _sections(summary)
    lines += ['', '## Judged corpora', ''] + _judged(summary)
    public = _public(summary)
    if public:
        lines += ['', '## Public benchmarks (evidence recall)', ''] + public
    lines += ['', '## Limitations', ''] + _limitations(summary)
    return '\n'.join(lines).rstrip('\n') + '\n'
