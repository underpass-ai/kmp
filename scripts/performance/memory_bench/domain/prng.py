"""Counted SplitMix64: the bench's only source of randomness.

Every draw is a pure function of `(seed, index)`: value `i` is the SplitMix64
finalizer applied to `seed + (i + 1) * GAMMA` modulo 2**64. The generator only
counts how many values it has handed out, so a stream can be replayed from any
position (`SplitMix64(seed, counter=i)`) and forked into independent labelled
sub-streams without sharing state. Integer arithmetic only, so Python 3.11 and
3.12 (and any platform) draw the same values; `random.Random` is never used.

Reference vector: seed 0 draws 0xE220A8397B1DCDAF, 0x6E789E6AA1B965F4, ...
(Vigna's splitmix64.c).
"""
import hashlib

MASK = (1 << 64) - 1
GAMMA = 0x9E3779B97F4A7C15


def mix(z):
    """The SplitMix64 output finalizer (Stafford variant 13)."""
    z &= MASK
    z = ((z ^ (z >> 30)) * 0xBF58476D1CE4E5B9) & MASK
    z = ((z ^ (z >> 27)) * 0x94D049BB133111EB) & MASK
    return z ^ (z >> 31)


def draw(seed, index):
    """The `index`-th value (0-based) of the stream seeded with `seed`."""
    if index < 0:
        raise ValueError('index must be non-negative')
    return mix((seed + (index + 1) * GAMMA) & MASK)


def derive_seed(seed, label):
    """A sub-stream seed: stable across runs, independent of draw order."""
    material = f'{seed & MASK}:{label}'.encode('utf-8')
    return int.from_bytes(hashlib.sha256(material).digest()[:8], 'big')


class SplitMix64:
    """A counted stream. `counter` is the number of values already drawn."""

    __slots__ = ('seed', 'counter')

    def __init__(self, seed, counter=0):
        if not isinstance(seed, int) or isinstance(seed, bool):
            raise TypeError('seed must be an int')
        if counter < 0:
            raise ValueError('counter must be non-negative')
        self.seed = seed & MASK
        self.counter = counter

    def next_u64(self):
        value = draw(self.seed, self.counter)
        self.counter += 1
        return value

    def below(self, bound):
        """Uniform integer in [0, bound), unbiased (rejection of the short tail)."""
        if not isinstance(bound, int) or bound <= 0:
            raise ValueError('bound must be a positive int')
        if bound > MASK + 1:
            raise ValueError('bound exceeds 2**64')
        limit = (MASK + 1) - ((MASK + 1) % bound)
        while True:
            value = self.next_u64()
            if value < limit:
                return value % bound

    def random(self):
        """Uniform float in [0, 1) with 53 random bits."""
        return (self.next_u64() >> 11) * (1.0 / (1 << 53))

    def choice(self, items):
        if not items:
            raise ValueError('cannot choose from an empty sequence')
        return items[self.below(len(items))]

    def shuffled(self, items):
        """A Fisher-Yates shuffled copy (items are never mutated)."""
        out = list(items)
        for i in range(len(out) - 1, 0, -1):
            j = self.below(i + 1)
            out[i], out[j] = out[j], out[i]
        return out

    def sample(self, items, k):
        """k distinct items, in draw order (partial Fisher-Yates)."""
        if k < 0 or k > len(items):
            raise ValueError('sample size out of range')
        pool = list(items)
        for i in range(k):
            j = i + self.below(len(pool) - i)
            pool[i], pool[j] = pool[j], pool[i]
        return pool[:k]

    def fork(self, label):
        """An independent stream named by `label`; does not advance this one."""
        return SplitMix64(derive_seed(self.seed, label))

    def __repr__(self):
        return f'SplitMix64(seed={self.seed:#x}, counter={self.counter})'
