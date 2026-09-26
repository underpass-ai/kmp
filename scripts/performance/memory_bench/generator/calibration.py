"""How far a synth-v1 world sits from the real store (BENCH_SPEC section 4.5).

The reference profile below is aggregate-only: percentiles, an ECDF over document
frequencies, fractions and counts, measured once on a frozen copy of the live store
(918 entries, 2026-09-25). No text, term, ref or about name of the real store is kept,
here or anywhere in the repository. `measure_store` re-measures it from a store copy that
lives outside the repository and prints the same aggregates, so the profile can be
refreshed by pasting its output.

Every text statistic goes through `kmp_search_probe` (the kernel's own tokenizer, no
stemming: `about_texts = []`) on both sides, so the two profiles are measured with the
same instrument:

- **length**: characters of an entry's text; KS against the reference percentiles.
- **df**: for each informative term, the number of entries holding it (whole corpus);
  KS against the reference ECDF. Measured on the first 1000 entries of the world so the
  corpus size matches the store's.
- **identifier density**: distinct informative terms with a digit, per entry.
- **anchor-chain fraction**: entries sharing a digit-bearing kernel identifier with
  another entry of the same about.
- **isolated fraction**: entries with no entry-entry relation.
- **relations per entry**: entry-entry relations over entries.

The reference must be measured with the probe of the tree under test: a tokenizer change
moves both sides. P3 (compound identifiers as whole terms) took the store's identifier
density from 3.719 to 5.2 and seed 7's (multi, first 1000 entries) from 3.399 to 4.939;
the relative gap stayed below 0.2 on both probes (0.086 before, 0.050 after), so the
generator did not move, the instrument did. Re-measure with `measure_store` whenever
`search_tokens` changes.

The brief's figures were measured with other counts (3.6 identifier tokens per entry from
3 313 whitespace tokens with a digit; 52 % in anchor chains); the reference below keeps the
probe-measured values and says so.
"""
import argparse
import bisect
import collections
import json
from pathlib import Path
import re
import sqlite3
import sys

REFERENCE = {
    'source': 'live store copy frozen 2026-09-25 (918 entries, 10 abouts with entries), text statistics '
              're-measured on the 2026-09-26 freeze (same 918 entries) with the P3 probe; '
              'aggregates only, measured with kmp_search_probe, about_texts=[]',
    'entries': 918,
    'relations_entry_entry': 308,
    'length_percentiles': (
        38, 51, 55, 63, 67, 69, 73, 90, 101, 103, 105, 110, 115, 118, 122, 125, 131, 134, 137,
        140, 143, 147, 149, 151, 153, 156, 158, 161, 162, 165, 169, 172, 175, 178, 180, 183,
        187, 192, 196, 202, 207, 210, 215, 220, 224, 227, 231, 236, 238, 242, 246, 251, 254,
        259, 260, 266, 268, 272, 273, 277, 280, 285, 290, 295, 300, 306, 310, 316, 327, 332,
        341, 346, 357, 366, 374, 379, 390, 406, 421, 427, 440, 453, 461, 482, 493, 507, 532,
        541, 569, 613, 672, 762, 841, 957, 1049, 2028, 3563, 6017, 10975, 21558, 39823),
    # (df, share of the vocabulary with document frequency <= df); vocabulary 9 118 terms.
    # Re-measured on 2026-09-26 with the P3 probe, which adds each compound identifier
    # (`c6.24`, `0.7.0`) as a whole term beside its parts: the same frozen store read
    # 7 792 terms and 3.719 identifier terms per entry with the probe before P3.
    'df_ecdf': ((1, 0.4923), (2, 0.6393), (3, 0.7173), (4, 0.764), (5, 0.798),
                (6, 0.825), (8, 0.8625), (10, 0.8889), (12, 0.9052), (16, 0.9287),
                (20, 0.9471), (25, 0.96), (32, 0.9729), (40, 0.9801), (50, 0.9871),
                (64, 0.9933), (80, 0.9965), (100, 0.9978), (128, 0.9995), (160, 0.9998),
                (200, 1.0)),
    'vocabulary': 9118,
    'identifier_density': 5.2,
    'anchor_chain_fraction': 0.568,
    'isolated_fraction': 0.600,
    'relations_per_entry': 0.3355,
    'relation_mix': {'uses_background': 132, 'follows': 68, 'verified_by': 26, 'answers': 23,
                     'chosen_because': 19, 'qualifies_as': 13, 'confirms_selection': 7,
                     'corrects': 5, 'supersedes': 4, 'component_of': 3, 'derived_from': 2,
                     'updates_state': 2, 'other': 4},
    'max_degree': 15,
}
DIGIT = re.compile(r'\d')


def ks_against_percentiles(values, percentiles):
    """max |F_synth(x_q) - q/100| over the reference percentile points (a KS lower bound)."""
    values = sorted(values)
    if not values:
        return None
    worst = 0.0
    for q, x in enumerate(percentiles):
        below = bisect.bisect_right(values, x) / len(values)
        worst = max(worst, abs(below - q / 100))
    return round(worst, 4)


def ks_against_ecdf(values, points):
    """max |F_synth(d) - F_ref(d)| over the reference ECDF points."""
    values = sorted(values)
    if not values:
        return None
    worst = 0.0
    for point, share in points:
        worst = max(worst, abs(bisect.bisect_right(values, point) / len(values) - share))
    return round(worst, 4)


def text_profile(rows):
    """rows: (about, text, probe line) per entry -> length, df and identifier aggregates."""
    lengths = [len(text) for _, text, _ in rows]
    df = collections.Counter()
    shared = collections.Counter()
    digits = 0
    for about, _, probe in rows:
        terms = set(probe['informative_terms'])
        df.update(terms)
        digits += sum(1 for term in terms if DIGIT.search(term))
        shared.update((about, ident) for ident in set(probe['identifiers']) if DIGIT.search(ident))
    in_chain = sum(1 for about, _, probe in rows
                   if any(shared[(about, ident)] >= 2 for ident in set(probe['identifiers'])
                          if DIGIT.search(ident)))
    return {'lengths': lengths, 'df': list(df.values()),
            'identifier_density': digits / len(rows) if rows else None,
            'anchor_chain_fraction': in_chain / len(rows) if rows else None}


def graph_profile(entry_refs, edges):
    """edges: (from_ref, to_ref, rel) between entries."""
    entry_refs = set(entry_refs)
    degree = collections.Counter()
    mix = collections.Counter()
    count = 0
    for source, target, rel in edges:
        if source in entry_refs and target in entry_refs:
            degree[source] += 1
            degree[target] += 1
            mix[rel] += 1
            count += 1
    isolated = sum(1 for ref in entry_refs if not degree[ref])
    return {'relations': count, 'relation_mix': dict(sorted(mix.items())),
            'isolated_fraction': isolated / len(entry_refs) if entry_refs else None,
            'relations_per_entry': count / len(entry_refs) if entry_refs else None,
            'max_degree': max(degree.values()) if degree else 0}


def compare(text, graph, reference=REFERENCE):
    """The calibration block of a world manifest: each statistic beside the reference."""
    def pair(name, value):
        return {'synth': None if value is None else round(value, 4), 'real': reference[name]}
    return {
        'reference': reference['source'],
        'entries_measured': len(text['lengths']),
        'ks_length': ks_against_percentiles(text['lengths'], reference['length_percentiles']),
        'ks_df': ks_against_ecdf(text['df'], reference['df_ecdf']),
        'vocabulary': {'synth': len(text['df']), 'real': reference['vocabulary']},
        'identifier_density': pair('identifier_density', text['identifier_density']),
        'anchor_chain_fraction': pair('anchor_chain_fraction', text['anchor_chain_fraction']),
        'isolated_fraction': pair('isolated_fraction', graph['isolated_fraction']),
        'relations_per_entry': pair('relations_per_entry', graph['relations_per_entry']),
        'max_degree': {'synth': graph['max_degree'], 'real': reference['max_degree']},
        'relation_mix': {'synth': graph['relation_mix'], 'real': reference['relation_mix']},
    }


def world_calibration(world, probe, limit=1000):
    """Calibration of the first `limit` entries of a world (the store's own size)."""
    entries = [json.loads(line) for line in world.lines['entries'][:limit]]
    lines = probe.run([{'id': index, 'about_texts': [], 'text': entry['text']}
                       for index, entry in enumerate(entries)])
    rows = [(entry['about'], entry['text'], line) for entry, line in zip(entries, lines)]
    refs = [entry['ref'] for entry in entries]
    edges = [(r['from'], r['to'], r['rel']) for r in map(json.loads, world.lines['relations'])]
    return compare(text_profile(rows), graph_profile(refs, edges))


# --- Re-measuring the reference from a store copy (outside the repository) ------------------

def read_store(store_dir):
    """(ref, about, text) of every memory entry and (from, to, rel) of every relation.

    Opens a copy of kernel.sqlite3 read-only; the caller keeps the copy outside the repo.
    """
    path = Path(store_dir) / 'store' / 'kernel.sqlite3'
    connection = sqlite3.connect(f'file:{path}?mode=ro&immutable=1', uri=True)
    try:
        entries = []
        for key, value in connection.execute('select k, v from nodes'):
            node = json.loads(value)
            properties = node.get('properties', {})
            if properties.get('memory_entity_kind') == 'memory_entry':
                entries.append((key, properties.get('memory_about'), properties.get('payload_text') or ''))
        edges = [(source, target, rel) for source, target, rel in
                 connection.execute('select k1, k2, k3 from relations_by_source')]
    finally:
        connection.close()
    return entries, edges


def measure_store(store_dir, probe):
    entries, edges = read_store(store_dir)
    lines = probe.run([{'id': index, 'about_texts': [], 'text': text}
                       for index, (_, _, text) in enumerate(entries)])
    text = text_profile([(about, body, line) for (_, about, body), line in zip(entries, lines)])
    graph = graph_profile([ref for ref, _, _ in entries], edges)
    lengths = sorted(text['lengths'])
    n = len(lengths)
    df = sorted(text['df'])
    points = (1, 2, 3, 4, 5, 6, 8, 10, 12, 16, 20, 25, 32, 40, 50, 64, 80, 100, 128, 160, 200)
    return {'entries': n, 'relations_entry_entry': graph['relations'],
            'length_percentiles': [lengths[min(n - 1, n * q // 100)] for q in range(101)],
            'df_ecdf': [[p, round(bisect.bisect_right(df, p) / len(df), 4)] for p in points],
            'vocabulary': len(df), 'identifier_density': round(text['identifier_density'], 3),
            'anchor_chain_fraction': round(text['anchor_chain_fraction'], 3),
            'isolated_fraction': round(graph['isolated_fraction'], 3),
            'relations_per_entry': round(graph['relations_per_entry'], 4),
            'max_degree': graph['max_degree']}


def main(argv=None):
    from .probe import SearchProbe
    parser = argparse.ArgumentParser(description='Re-measure the real-store reference profile '
                                                 '(aggregates only) from a store copy.')
    parser.add_argument('--store', type=Path, required=True,
                        help='a copy of a KMP data directory, outside the repository')
    parser.add_argument('--probe', type=Path, default=None)
    args = parser.parse_args(argv)
    from ..domain.jsonl import guard_private_path
    guard_private_path(args.store)
    print(json.dumps(measure_store(args.store, SearchProbe.locate(args.probe)), indent=1))
    return 0


if __name__ == '__main__':
    sys.exit(main())
