"""synth-v1 topologies: which abouts a world has and how episodes fall into them.

`mono` puts everything in one about. `multi` has `MULTI_ABOUTS` abouts (the real store has
10 abouts with entries and 12 anchors) and assigns each episode to one about with a Zipf
law of exponent s = 1.1. The weights are integers (round(10^6 / i^1.1), computed once and
written down) so no float ever decides where an entry lands. The about list is fixed per
topology and never depends on the level, so the ladder stays nested.
"""
from dataclasses import dataclass

TOPOLOGIES = ('mono', 'multi')
ZIPF_S = 1.1
MULTI_ABOUTS = 12
# round(1_000_000 / i ** 1.1) for i = 1..12.
ZIPF_WEIGHTS = (1000000, 466516, 298653, 217638, 170268, 139326, 117596, 101532, 89194,
                79433, 71527, 64998)
DIMENSION = {'id': 'work:main', 'kind': 'work'}


def about_name(topology, index):
    return f'synth:{topology}-a{index:03d}'


@dataclass(frozen=True)
class Topology:
    name: str

    def __post_init__(self):
        if self.name not in TOPOLOGIES:
            raise ValueError(f'topology {self.name!r} is not one of {", ".join(TOPOLOGIES)}')

    @property
    def size(self):
        return 1 if self.name == 'mono' else MULTI_ABOUTS

    @property
    def zipf_s(self):
        return None if self.name == 'mono' else ZIPF_S

    def abouts(self):
        return tuple(about_name(self.name, index) for index in range(self.size))

    def about_records(self):
        return tuple({'about': about, 'title': f'Synthetic journal {about.rsplit("-", 1)[1]}',
                      'dimensions': [dict(DIMENSION)]} for about in self.abouts())

    def draw_about(self, rng):
        """One about for an episode: Zipf over the about list (index 0 the most popular)."""
        if self.size == 1:
            return about_name(self.name, 0)
        total = sum(ZIPF_WEIGHTS)
        point = rng.below(total)
        for index, weight in enumerate(ZIPF_WEIGHTS):
            if point < weight:
                return about_name(self.name, index)
            point -= weight
        raise AssertionError('unreachable: the point falls inside the total weight')

    def hub_about(self, hub_index):
        """Hubs live in the most popular abouts: a000, a001, a002, a003 in multi."""
        return about_name(self.name, 0 if self.size == 1 else hub_index % self.size)

    def other_abouts(self, about):
        return tuple(name for name in self.abouts() if name != about)
