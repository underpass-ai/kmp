"""A stand-in for the Jev provider where the network is blocked (BT11 and BT18 on the real binary).

The binary talks to Jev only through `https://api.typesafe.ai` or a cassette
(`cassette_judgement.rs`), so an offline run that must *ask* something needs a
cassette that already answers it. This module builds one the way a provider
would answer, without knowing the binary's request bodies:

1. a warm-up process judges the same calls behind an empty cassette in replay
   mode; every judgement misses and its `kmp_judgement` telemetry line keeps
   the request key and how many questions it held;
2. `write_stand_in` answers each of those keys: per yes/no question `p<i>` a
   probability drawn from `seed` and the key, like a model whose samples
   differ (another seed, other answers).

Only the passage sites (`rerank`, `wake_focus`) are supported: their questions
are named `p0..p<n-1>` and are all yes/no, so a line says everything a body
would. Any other site is refused rather than guessed.
"""
import json
from pathlib import Path
import random

from ..domain.errors import BenchError
from .jev_fixture import CASSETTE_SCHEMA, DEFAULT_MODEL

PASSAGE_SITES = ('rerank', 'wake_focus')
TOKENS_PER_QUESTION = 40


class StandInError(BenchError):
    code = 'JEV_STAND_IN_REFUSED'


def misses(events):
    """(request_key, questions) of every judgement a warm-up process could not answer."""
    found = {}
    for event in events:
        fields = event.get('fields') or {}
        if fields.get('event') != 'kmp_judgement' or fields.get('source') != 'cassette_miss':
            continue
        if fields.get('site') not in PASSAGE_SITES:
            raise StandInError(f'site {fields.get("site")!r} is not a passage site; its questions '
                               'cannot be read from telemetry')
        key, questions = fields.get('request_key'), fields.get('questions')
        if not key or not isinstance(questions, int) or questions < 1:
            raise StandInError(f'judgement line without a request key or question count: {fields}')
        found[key] = questions
    return found


def answers_for(key, questions, seed):
    rng = random.Random(f'{seed}:{key}')
    return {f'p{n}': {'type': 'noul', 'yes': round(rng.random(), 2)} for n in range(questions)}


def write_stand_in(path, asked, seed=0, model=DEFAULT_MODEL):
    """A cassette answering every (request_key -> questions) in `asked`; returns its entry count."""
    entries = {key: {'model': model, 'answers': answers_for(key, questions, seed),
                     'input_tokens': TOKENS_PER_QUESTION * questions, 'requests': 1}
               for key, questions in sorted(asked.items())}
    Path(path).parent.mkdir(parents=True, exist_ok=True)
    Path(path).write_text(json.dumps({'schema': CASSETTE_SCHEMA, 'model': model,
                                      'entries': entries}, indent=1, sort_keys=True) + '\n')
    return len(entries)
