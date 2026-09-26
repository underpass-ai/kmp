"""What one `kmp_ingest` batch costs, as a function of its size and of what the about holds.

A build records, per batch, the entries the about held before the call (H), the
entries sent (B) and the wall ms. The cost is fitted by least squares, without an
intercept, on four candidate terms:

    wall_ms ≈ c_B·B + c_H·H + c_BH·B·H + c_BB·B²

- `B`: work per entry sent (the part a larger batch cannot save);
- `H`: work per batch over the whole about already stored (re-reading or re-projecting
  it), which makes a build O(N²/B);
- `B·H`: work per entry sent over the stored about (O(B·N) per batch, O(N²) per build
  whatever the batch size);
- `B²`: work quadratic within one batch.

B=1000 and B=5000 at 10^4 (BENCH_SPEC 4.5) give batches whose (B, H) pairs separate the
terms; `components_ms` says how many ms each term contributes to the largest batch
seen, so the report names the term that dominates instead of a coefficient.
`estimate_ms` sums the fitted cost over the batches of a bigger build, which is how a
10^4 measurement predicts 10^5 before 10^5 is attempted.
"""
from dataclasses import dataclass

COLUMNS = ('about', 'held_before', 'entries', 'relations', 'wall_ms', 'server_ms')
TERMS = ('B', 'H', 'B*H', 'B^2')


def _features(entries, held):
    return (float(entries), float(held), float(entries) * held, float(entries) ** 2)


def _row(row):
    values = row if isinstance(row, dict) else dict(zip(COLUMNS, row))
    return values['entries'], values['held_before'], values['wall_ms']


def _solve(matrix, vector):
    """Gauss-Jordan with partial pivoting; None when the system is singular."""
    size = len(vector)
    rows = [list(matrix[i]) + [vector[i]] for i in range(size)]
    for column in range(size):
        pivot = max(range(column, size), key=lambda r: abs(rows[r][column]))
        if abs(rows[pivot][column]) < 1e-12:
            return None
        rows[column], rows[pivot] = rows[pivot], rows[column]
        for other in range(size):
            if other != column:
                factor = rows[other][column] / rows[column][column]
                rows[other] = [a - factor * b for a, b in zip(rows[other], rows[column])]
    return [rows[i][size] / rows[i][i] for i in range(size)]


@dataclass(frozen=True)
class GrowthFit:
    terms: tuple  # names from TERMS, in coefficient order
    coefficients: tuple  # ms per unit of each term
    batches: int
    residual_ms: float  # root mean square error of the fit over the batches
    largest: tuple  # (B, H) of the batch with the largest B*H seen

    def predict_ms(self, entries, held):
        features = dict(zip(TERMS, _features(entries, held)))
        return sum(c * features[t] for t, c in zip(self.terms, self.coefficients))

    def components_ms(self):
        entries, held = self.largest
        features = dict(zip(TERMS, _features(entries, held)))
        return {t: c * features[t] for t, c in zip(self.terms, self.coefficients)}

    def estimate_ms(self, about_sizes, batch_size):
        """Predicted ingest wall ms for abouts of these sizes, loaded from empty in batches."""
        total = 0.0
        for size in about_sizes:
            for held in range(0, size, batch_size):
                total += self.predict_ms(min(batch_size, size - held), held)
        return total

    def as_dict(self):
        return {'model': 'wall_ms = ' + ' + '.join(f'c[{t}]*{t}' for t in self.terms),
                'coefficients': dict(zip(self.terms, self.coefficients)),
                'batches': self.batches, 'residual_ms': self.residual_ms,
                'largest_batch': {'B': self.largest[0], 'H': self.largest[1]},
                'components_ms_at_largest': self.components_ms()}


def fit(rows, terms=TERMS):
    """GrowthFit of batch rows (COLUMNS order, dicts or sequences) over `terms`.

    Terms the rows cannot separate (a singular system, e.g. one batch size and B²) are
    dropped from the end until the system solves; None with fewer batches than terms.
    """
    points = [_row(row) for row in rows]
    points = [(b, h, ms) for b, h, ms in points if b > 0]
    terms = tuple(terms)
    while terms and len(points) >= len(terms):
        index = [TERMS.index(t) for t in terms]
        features = [[_features(b, h)[i] for i in index] for b, h, _ in points]
        scale = [max(abs(f[j]) for f in features) or 1.0 for j in range(len(terms))]
        scaled = [[f[j] / scale[j] for j in range(len(terms))] for f in features]
        normal = [[sum(r[i] * r[j] for r in scaled) for j in range(len(terms))] for i in range(len(terms))]
        right = [sum(r[i] * ms for r, (_, _, ms) in zip(scaled, points)) for i in range(len(terms))]
        solved = _solve(normal, right)
        if solved is not None:
            coefficients = tuple(s / scale[j] for j, s in enumerate(solved))
            residuals = [ms - sum(c * f for c, f in zip(coefficients, feature))
                         for feature, (_, _, ms) in zip(features, points)]
            rms = (sum(r * r for r in residuals) / len(points)) ** 0.5
            largest = max(((b, h) for b, h, _ in points), key=lambda pair: (pair[0] * pair[1], pair))
            return GrowthFit(terms, coefficients, len(points), rms, largest)
        terms = terms[:-1]
    return None
