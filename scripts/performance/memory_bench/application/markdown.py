"""report.md from report.json alone (BENCH_SPEC section 12).

`render(report)` reads nothing but the report dict: no run directory, no clock, no
environment. Numbers are formatted with fixed precision and every table walks its
keys in sorted order (never in the dict's own order, which a reader may not keep), so
the same report.json always renders to the same bytes.
The eleven sections follow the report's own order.
"""

DASH = '—'
TITLES = ('Provenance', 'Headlines', 'Results by type', 'Scale', 'Latency and resources', 'Tokens',
          'Jev by site', 'Controls', 'Power and MDE', 'Limitations', 'Verdict')


def num(value, digits=3):
    if value is None:
        return DASH
    if isinstance(value, bool):
        return 'yes' if value else 'no'
    if isinstance(value, int):
        return str(value)
    if isinstance(value, float):
        return f'{value:.{digits}f}'
    return str(value)


def ci(interval, digits=3):
    return DASH if not interval else f'[{num(interval[0], digits)}, {num(interval[1], digits)}]'


def short(value, width=12):
    return DASH if not value else str(value)[:width]


def metric_cells(metric):
    if not metric:
        return [DASH, DASH, DASH]
    value = num(metric.get('value')) if metric.get('value') is not None else f'{DASH} ({metric.get("absent_reason")})'
    return [value, ci(metric.get('ci95')), num(metric.get('n'))]


def table(header, rows):
    lines = ['| ' + ' | '.join(header) + ' |', '|' + '|'.join('---' for _ in header) + '|']
    for row in rows:
        lines.append('| ' + ' | '.join(str(cell).replace('|', '\\|') for cell in row) + ' |')
    return lines


def _provenance(section):
    lines = []
    rows = []
    for arm in ('baseline', 'candidate'):
        data = section.get(arm)
        if data:
            rows.append([arm, data['variant'], num(data.get('binary_version')), short(data['binary_sha256']),
                         short(data['config_digest']), short(data['preregistration_digest']),
                         ', '.join(short(r) for r in data['run_ids']),
                         short((data.get('store') or {}).get('content_digest'), 19),
                         ', '.join(num(m) for m in data.get('max_bytes') or [])])
    lines += table(['arm', 'variant', 'version', 'binary', 'config', 'prereg', 'runs', 'store', 'max_bytes'],
                   rows)
    questions = section['questions']
    corpora = ', '.join(f'{name} {count}' for name, count in sorted(questions['by_corpus'].items()))
    rules = section['rules']
    lines += ['', f'- Questions: `{short(questions["digest"])}` ({corpora})',
              f'- Driver: `{section["driver_version"]}`; encoders: {", ".join(section["encoders"])}',
              f'- Rules: scoring `{short(rules.get("scoring_rules_sha256"))}`, negatives '
              f'`{short(rules.get("negatives_sha256"))}`, modes `{short(rules.get("modes_sha256"))}`',
              f'- Comparable: {num(section["comparable"])}'
              + (f' (drift: {", ".join(section["drift"])})' if section['drift'] else '')]
    return lines


def _headlines(rows):
    lines = []
    for row in rows:
        lines += ['', f'### {row["arm"]} · level {num(row["level"])} · topology {num(row["topology"])} '
                      f'· max_bytes {num(row["max_bytes"])}', '']
        body = [[name] + metric_cells(metric) for name, metric in sorted(row['metrics'].items())
                if metric.get('n')]
        lines += table(['metric', 'value', '95% CI', 'n'], body)
        unknown = row['false_unknown']
        per_useful = row['tokens_per_useful']
        lines += ['', f'- false UNKNOWN: scorecard {num(unknown["scorecard"]["value"])}, '
                      f'P(UNKNOWN | KNOWN) {num(unknown["given_known"]["value"])}',
                  f'- high precision: {num(row["high_precision"]["value"])} '
                  f'(n {num(row["high_precision"]["n"])}, certified {num(row["high_certified"])})',
                  f'- tokens per useful ({per_useful["encoding"]}): {num(per_useful["value"], 1)}'
                  + (f' ({per_useful["absent_reason"]})' if per_useful['absent_reason'] else ''),
                  f'- journeys not scored: {row["errors"]}']
    return lines


def _by_type(section):
    lines = []
    for kind in sorted(section):
        entry = section[kind]
        arms = sorted(entry['metrics'])
        lines += ['', f'### {kind} (n = {entry["n"]})', '']
        names = sorted(set().union(*(entry['metrics'][arm] for arm in arms)))
        rows = []
        for name in names:
            cells = [name] + [num((entry['metrics'][arm].get(name) or {}).get('value')) for arm in arms]
            d = entry['deltas'].get(name)
            if d is not None:
                cells += [num(d.get('delta')), num(d.get('p_value'))]
            rows.append(cells)
        header = ['metric'] + arms + (['Δ', 'p (McNemar)'] if entry['deltas'] else [])
        lines += table(header, rows) + ['']
        for arm in arms:
            outcomes = entry.get('outcomes', {}).get(arm) or {}
            reasons = (entry.get('reasons', {}).get(arm) or {}).get('bench') or {}
            if outcomes:
                lines.append(f'- {arm} outcomes: ' + ', '.join(f'{k} {v}' for k, v in sorted(outcomes.items())))
            if reasons:
                lines.append(f'- {arm} UNKNOWN reasons: ' + ', '.join(f'{k} {v}' for k, v in sorted(reasons.items())))
    return lines


def _scale(section):
    rows = [[e['arm'], num(e['topology']), num(e['metric']), num(e['b']), ci(e['ci95']), num(e['points']),
             ', '.join(str(level) for level in e['levels'])] for e in section['exponents']]
    lines = table(['arm', 'topology', 'metric', 'b', '95% CI', 'points', 'levels'], rows) if rows else \
        ['No run spans two ladder levels: no exponent.']
    lines += ['', f'- Calibration against the real store: {"present" if section["calibration"] else "not given"}']
    return lines


def _latency(section):
    rows = []
    for row in section['by_tool_phase']:
        rows.append([row['arm'], row['tool'], row['phase'], num(row['calls']), num(row['censored']),
                     num((row['wall_ms_p50'] or {}).get('value'), 2), ci((row['wall_ms_p50'] or {}).get('ci95'), 2),
                     num((row['wall_ms_p95'] or {}).get('value'), 2),
                     num((row['wall_ms_p99'] or {}).get('value'), 2),
                     num((row['server_ms'] or {}).get('p50'), 1), num((row['cpu_ms'] or {}).get('p50'), 2),
                     num((row['rss_peak_kb'] or {}).get('max'))])
    lines = table(['arm', 'tool', 'phase', 'calls', 'censored', 'wall p50 ms', 'CI', 'p95', 'p99',
                   'server p50 ms', 'cpu p50 ms', 'RSS max kB'], rows)
    for arm, startup in sorted(section['startup_ms'].items()):
        lines.append(f'- {arm} startup p50: {num((startup or {}).get("p50"), 2)} ms')
    return lines


def _load_line(arm, tool, load):
    if not load:
        return None
    pages, sizes, tokens = load['pages'] or {}, load['response_bytes'] or {}, load['tokens'] or {}
    return [arm, tool, num(load['journeys']), num(pages.get('mean'), 2), num(pages.get('max')),
            num(sizes.get('mean'), 0), num(sizes.get('max')), num(tokens.get('mean'), 0), num(tokens.get('max')),
            num(load['completed_rate']['value'])]


def _tokens(section):
    if not section['encoders']:
        return [f'Tokens not counted: {section["absent_reason"]}']
    lines = [f'Representation `{section["representation"]}`; encoders {", ".join(section["encoders"])}.', '']
    rows = []
    for kind in sorted(section['by_type']):
        for arm, encodings in sorted(section['by_type'][kind].items()):
            for encoding, fields in sorted(encodings.items()):
                rows.append([kind, arm, encoding] + [num(fields[f]['value'], 1) for f in
                                                     ('journey', 'first_page', 'to_first_evidence', 'to_task_ready')])
    lines += table(['type', 'arm', 'encoding', 'journey', 'first page', 'to 1st evidence', 'to task ready'], rows)
    lines += ['', 'Agent load until the journey completes:', '']
    load_rows = []
    for arm, data in sorted(section['arms'].items()):
        for tool, load in sorted(data['agent_load'].items()):
            line = _load_line(arm, tool, load)
            if line:
                load_rows.append(line)
    lines += table(['arm', 'tool', 'journeys', 'pages mean', 'pages max', 'bytes mean', 'bytes max',
                    'tokens mean', 'tokens max', 'completed'], load_rows)
    lines += ['']
    for arm, data in sorted(section['arms'].items()):
        for encoding, value in sorted(data['tokens_per_useful'].items()):
            lines.append(f'- {arm} tokens per useful ({encoding}): {num(value["value"], 1)}'
                         + (f' ({value["absent_reason"]})' if value['absent_reason'] else ''))
        for encoding, value in sorted(data['startup_amortized'].items()):
            lines.append(f'- {arm} startup amortized per journey ({encoding}): {num(value, 1)}')
    lines += ['', 'Quality-tokens curve:', '']
    curve_rows = [[arm, num(p['level']), num(p['max_bytes']), num(p['useful_rate']['value']),
                   num(p['tokens_journey']['value'], 1), num(p['tokens_per_useful'], 1)]
                  for arm, points in sorted(section['curve'].items()) for p in points]
    lines += table(['arm', 'level', 'max_bytes', 'useful', 'tokens mean', 'tokens/useful'], curve_rows)
    return lines


def _jev(section):
    lines = [f'Price: {section["price_usd_per_mtok"]} $/M input tokens.', '']
    for arm, data in sorted(section['arms'].items()):
        if data['sites'] is None:
            lines.append(f'- {arm}: {DASH} ({data["absent_reason"]})')
            continue
        if not data['sites']:
            lines.append(f'- {arm}: telemetry present, no Jev evaluation')
            continue
        rows = [[site, num(row['letter']), num(row['evaluations']), num(row['requests']),
                 num(row['input_tokens']), num(row['usd'], 6), num(row['book_hits']),
                 num(row['sources'].get('cassette_hit')), num(row['sources'].get('cassette_miss'))]
                for site, row in sorted(data['sites'].items())]
        lines += ['', f'{arm}:', ''] + table(['site', 'letter', 'evaluations', 'requests', 'input tokens', '$',
                                          'book hits', 'cassette hits', 'cassette misses'], rows) + ['']
    lines.append('')
    lines.append(f'- Δ $: {num(section["delta_usd"], 6)}; marginal $ per useful: '
                 f'{num(section["marginal_cost_per_useful"], 6)}'
                 + (f' ({section["marginal_absent_reason"]})' if section['marginal_absent_reason'] else ''))
    return lines


def _controls(section):
    lines = []
    aa = section.get('aa')
    lines.append(f'- A/A: {DASH}' if aa is None else
                 f'- A/A: holds {num(aa["holds"])}; quality Δ = 0 {num(aa["quality_delta_zero"])}; '
                 f'tokens Δ = 0 {num(aa["token_delta_zero"])} (raw), '
                 f'{num(aa.get("token_delta_zero_normalized"))} (handles normalized)'
                 + (f'; moved: {", ".join(aa["moved"])}' if aa['moved'] else '')
                 + (f'; raw moved: {", ".join(aa["raw_moved"])}' if aa.get('raw_moved') else ''))
    det = section.get('determinism')
    lines.append(f'- Determinism: {DASH} (no determinism report given)' if det is None else
                 f'- Determinism: rate {num(det["determinism_rate"])} over {num(det["calls"])} calls, '
                 f'{num(det["runs"])} runs, arch {num(det["arch"])}, binary of an arm {num(det["binary_matches_arm"])}')
    parity = section.get('parity')
    lines.append(f'- Parity: {DASH}' if parity is None else
                 f'- Parity: rate {num(parity["parity_rate"])} (identical {parity["identical"]}, normalized '
                 f'{parity["normalized"]}, different {parity["different"]} of {parity["calls"]} calls)')
    if parity and parity.get('path_counts'):
        lines += ['', 'Differing paths:', ''] + table(
            ['path', 'calls'], [[path, count] for path, count in sorted(parity['path_counts'].items())]) + ['']
    for arm, repeat in sorted((section.get('repeat') or {}).items()):
        lines.append(f'- {arm} repeat consistency: {DASH} (one repeat)' if repeat is None else
                     f'- {arm} repeat consistency: rate {num(repeat["parity_rate"])} over {repeat["calls"]} calls')
    if section.get('unpaired_questions'):
        lines.append(f'- Unpaired (question, sample): {section["unpaired_questions"]}')
    return lines


def _power(rows):
    body = []
    for row in rows:
        reference = row.get('mde_at_reference_d') or {}
        body.append([row['metric'], num(row['n']), num(row['discordant_rate']), num(row['mde']),
                     num(row['effect']), num(row['decidable']),
                     ', '.join(f'd={d}: {num(v)}' for d, v in sorted(reference.items()))])
    return table(['metric', 'n', 'discordant', 'MDE', 'effect', 'decidable', 'MDE at reference d'], body)


def _verdict(section):
    lines = [f'**{section["value"]}** (claim {num(section["claim"])})', '']
    lines += [f'- {reason}' for reason in section['reasons']]
    if section['targets']:
        lines += ['', 'Targets:', ''] + table(
            ['metric', 'Δ', '95% CI', 'p', 'MDE', 'effect', 'decidable', 'status'],
            [[t['metric'], num(t.get('delta')), ci(t.get('ci95')), num(t.get('p_value')), num(t.get('mde')),
              num(t.get('effect')), num(t.get('decidable')), num(t.get('status'))] for t in section['targets']])
    if section['guards']:
        lines += ['', 'Guards:', ''] + table(
            ['metric', 'margin', 'Δ', 'worsening', 'broken'],
            [[g['metric'], num(g['margin']), num(g.get('delta')), num(g.get('worsening')), num(g['broken'])]
             for g in section['guards']])
    cost = section.get('cost')
    if cost:
        tokens = cost.get('tokens') or {}
        usd = cost.get('jev_usd') or {}
        lines += ['', f'- Cost: tokens Δ {num(tokens.get("delta"), 1)} {ci(tokens.get("ci95"), 1)}; '
                      f'Jev $ Δ per question {num(usd.get("delta"), 6)}; worse {num(cost["worse"])}, '
                      f'better {num(cost["better"])}']
    return lines


def render(report):
    """Markdown for one report.json dict; deterministic."""
    lines = ['# KMP memory bench report', '',
             f'Mode `{report["mode"]}` · report `{short(report["report_key"])}` · bench `{report["bench_version"]}` '
             f'· code `{short(report["generated_by"]["code_sha256"])}`']
    renderers = (lambda: _provenance(report['provenance']), lambda: _headlines(report['headlines']),
                 lambda: _by_type(report['by_type']), lambda: _scale(report['scale']),
                 lambda: _latency(report['latency_resources']), lambda: _tokens(report['tokens']),
                 lambda: _jev(report['jev_by_site']), lambda: _controls(report['controls']),
                 lambda: _power(report['power']),
                 lambda: [f'- {note}' for note in report['limitations']] or ['None recorded.'],
                 lambda: _verdict(report['verdict']))
    for number, (title, renderer) in enumerate(zip(TITLES, renderers), start=1):
        lines += ['', f'## {number}. {title}', ''] + renderer()
    return '\n'.join(lines) + '\n'
