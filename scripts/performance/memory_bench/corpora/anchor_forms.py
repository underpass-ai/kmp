"""Anchor identifiers of the real store: `#188`, `C6.4`, `v0.18.6`.

Two readings of "entry E is about anchor X", on purpose of different width:

- `mentions(X)` is lexical and wide: E holds every search key of one of X's key
  sets (`#188` -> {188} or {pr188} or {issue188}; `C6.4` -> {c6, 4}). A negative
  requires *no* mention, so erring wide only drops candidates, never adds a
  false UNKNOWN.
- `strict(X)` is a written occurrence (`#188`, `issue 188`, `PR188`, `C6.4`,
  `c6-4`, a compound `C6.1-C6.4` endpoint, `v0.18.6` or `0.18.6`). A negative
  that needs the anchor to exist, and gold that lists an anchor's entries, use
  it.
"""
from dataclasses import dataclass
import re

HASH = re.compile(r'#(\d{2,6})')
DOTTED = re.compile(r'c(\d{1,2})((?:\.\d{1,3})+)')
VERSION = re.compile(r'v(\d{1,3})\.(\d{1,3})\.(\d{1,3})')
FAMILIES = ('hash', 'dotted', 'version')


@dataclass(frozen=True, order=True)
class Anchor:
    family: str
    parts: tuple  # integers: (188,), (6, 4), (0, 18, 6)

    @classmethod
    def parse(cls, identifier):
        """An anchor from a folded kernel identifier, or None."""
        for family, pattern in (('hash', HASH), ('dotted', DOTTED), ('version', VERSION)):
            match = pattern.fullmatch(identifier)
            if match:
                numbers = [group.split('.') for group in match.groups()]
                return cls(family, tuple(int(n) for group in numbers for n in group if n))
        return None

    @property
    def display(self):
        if self.family == 'hash':
            return f'#{self.parts[0]}'
        if self.family == 'dotted':
            return 'C' + '.'.join(str(p) for p in self.parts)
        return 'v' + '.'.join(str(p) for p in self.parts)

    @property
    def folded(self):
        return self.display.lower()

    def keysets(self):
        """The search-key sets any of which counts as a mention."""
        if self.family == 'hash':
            n = self.parts[0]
            return (frozenset({str(n)}), frozenset({f'pr{n}'}), frozenset({f'issue{n}'}))
        head = ('c' if self.family == 'dotted' else 'v') + str(self.parts[0])
        return (frozenset({head, *(str(p) for p in self.parts[1:])}),)

    def strict_pattern(self):
        if self.family == 'hash':
            n = self.parts[0]
            return re.compile(rf'(?i)(?<![\w.])(?:#|issues?\s*#?|pr\s*#?){n}(?!\d)')
        if self.family == 'dotted':
            head, rest = self.parts[0], [str(p) for p in self.parts[1:]]
            return re.compile(rf'(?i)(?<![\w.])c{head}[.-]{"[.-]".join(rest)}(?![\d]|\.\d)')
        # `0.18.6` without its `v` is the same release (stores write both).
        return re.compile(r'(?i)(?<![\w.])v?' + r'\.'.join(str(p) for p in self.parts) + r'(?!\d|\.\d)')

    def family_prefix(self):
        """Siblings share it: `C6.` for C6.4, `v0.18.` for v0.18.6, and the hash family for #188."""
        return (self.family,) + self.parts[:-1]

    def perturbations(self):
        """Nearby identifiers that look plausible (#188 -> #288, C6.4 -> C6.24), in a fixed order."""
        *head, last = self.parts
        head = tuple(head)
        if self.family == 'hash':
            candidates = [last + 100, int(f'{last}0') if last < 1000 else last + 1000,
                          int(str(last)[:1] + '0' + str(last)[1:]), last + 1000]
        else:
            candidates = [int(f'2{last}'), int(f'{last}0'), last + 20, last + 11]
        seen, out = set(), []
        for value in candidates:
            anchor = Anchor(self.family, head + (value,))
            if value != last and anchor not in seen:
                seen.add(anchor)
                out.append(anchor)
        return tuple(out)


def anchors_in(identifiers):
    """Every anchor a set of folded identifiers carries, compound members included."""
    found = set()
    for identifier in identifiers:
        for piece in re.split(r'[+/,]|(?<=\d)-(?=[cv#])', identifier):
            anchor = Anchor.parse(piece)
            if anchor is not None:
                found.add(anchor)
    return found
