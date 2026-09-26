"""The Rust scorecards of the judged corpora, run as the CI scripts run them and read from stdout.

`retrieval_kmp_scorecard`, `jev_kmp_scorecard` and `relate_kmp_scorecard`
(crates/kmp-testkit) are the reference the stdio ports in `judged_retrieval`
and `judged_jev` are held to, and the only source for what those ports leave
out (relate, the Jev write/summary/label columns). Their metric lines are read
with the expression `scripts/eval/jev-samples.sh` uses (BENCH_SPEC 4.4).

The binary is built with cargo (the build is the only step that needs the
operator's HOME, for the toolchain) and then run directly in an isolated
environment: HOME and TMPDIR point into a scratch directory, so the scorecard's
temporary stores (`std::env::temp_dir()`) never collide with another run and
nothing near a real KMP store is opened. `runner` is injectable for tests.
"""
from dataclasses import dataclass, field
import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import tempfile

from ..domain.errors import BenchError

METRIC_LINE = re.compile(r'\s+([a-z0-9_]+)\s+([0-9.]+)$')  # scripts/eval/jev-samples.sh:73
JUDGED = 'crates/kmp-testkit/judged'


class ScorecardFailed(BenchError):
    code = 'SCORECARD_FAILED'


@dataclass(frozen=True)
class ScorecardSpec:
    """One scorecard invocation: which bin, its positional arguments and its environment."""
    name: str
    binary: str
    arguments: tuple = ()
    env: dict = field(default_factory=dict)  # repo-relative paths are made absolute at run time
    path_env: tuple = ()  # env names whose value is a repo-relative path


def retrieval(rerank=None):
    env = {'KMP_LEXICAL_BRIDGE': f'{JUDGED}/lexical-bridge.kmpb'}
    paths = ('KMP_LEXICAL_BRIDGE',)
    if rerank is not None:
        env.update(RETRIEVAL_RERANK=rerank, KMP_TYPESAFE_CASSETTE=f'{JUDGED}/retrieval.jev.cassette.json',
                   KMP_TYPESAFE_CASSETTE_MODE='replay')
        paths += ('KMP_TYPESAFE_CASSETTE',)
    return ScorecardSpec('retrieval' if rerank is None else f'retrieval-{rerank}',
                         'retrieval_kmp_scorecard',
                         (f'{JUDGED}/retrieval_cases.json', 'docs/development/retrieval-baseline.tsv'),
                         env, paths)


def jev():
    return ScorecardSpec('jev', 'jev_kmp_scorecard',
                         (f'{JUDGED}/jev_cases.json', 'docs/development/jev-baseline.tsv'),
                         {'KMP_TYPESAFE_CASSETTE': f'{JUDGED}/jev.cassette.json',
                          'KMP_TYPESAFE_CASSETTE_MODE': 'replay',
                          'KMP_LEXICAL_BRIDGE': f'{JUDGED}/lexical-bridge.kmpb',
                          'JEV_REPORT_ONLY': '1'},
                         ('KMP_TYPESAFE_CASSETTE', 'KMP_LEXICAL_BRIDGE'))


def relate():
    return ScorecardSpec('relate', 'relate_kmp_scorecard',
                         (f'{JUDGED}/relate_cases.json', 'docs/development/relate-baseline.tsv'))


def parse_metrics(stdout):
    """`name value` lines, first occurrence of each name kept, in stdout order."""
    values = {}
    for line in stdout.splitlines():
        match = METRIC_LINE.match(line)
        if match and match.group(1) not in values:
            try:
                values[match.group(1)] = float(match.group(2))
            except ValueError:
                continue
    return values


@dataclass(frozen=True)
class ScorecardRun:
    spec: ScorecardSpec
    returncode: int
    metrics: dict
    stdout: str
    stderr: str


def build(binary, repo_root, runner=subprocess.run, release=False):
    """`cargo build --locked -p kmp-testkit --bin <binary>`; returns the executable path."""
    command = ['cargo', 'build', '--locked', '--quiet', '-p', 'kmp-testkit', '--bin', binary,
               '--message-format=json']
    if release:
        command.insert(2, '--release')
    done = runner(command, cwd=repo_root, capture_output=True, text=True, check=False)
    if done.returncode != 0:
        raise ScorecardFailed(f'cargo build {binary}: exit {done.returncode}: {done.stderr[-800:]}')
    for line in done.stdout.splitlines():
        try:
            message = json.loads(line)
        except ValueError:
            continue
        if message.get('reason') == 'compiler-artifact' and message.get('executable') and \
                message.get('target', {}).get('name') == binary:
            return Path(message['executable'])
    raise ScorecardFailed(f'cargo build {binary}: no executable reported')


def isolated_env(spec, repo_root, scratch, parent_env=None):
    parent_env = os.environ if parent_env is None else parent_env
    env = {key: parent_env[key] for key in ('PATH', 'LANG', 'LC_ALL') if key in parent_env}
    home, tmp = Path(scratch) / 'home', Path(scratch) / 'tmp'
    home.mkdir(parents=True, exist_ok=True)
    tmp.mkdir(parents=True, exist_ok=True)
    env.update(HOME=str(home), TMPDIR=str(tmp), XDG_DATA_HOME=str(home / '.local/share'),
               XDG_CONFIG_HOME=str(home / '.config'))
    for key, value in spec.env.items():
        env[key] = str(Path(repo_root) / value) if key in spec.path_env else value
    return env


def run(spec, repo_root, work_dir, runner=subprocess.run, release=False, executable=None):
    """Build (unless `executable` is given) and run one scorecard; metrics parsed from stdout.

    A nonzero exit is kept in `returncode` (a gated scorecard exits 1 when a floor
    breaks, after printing every metric); only a run that printed no metric fails.
    """
    repo_root = Path(repo_root).resolve()
    executable = executable or build(spec.binary, repo_root, runner, release)
    Path(work_dir).mkdir(parents=True, exist_ok=True)
    scratch = tempfile.mkdtemp(prefix=f'scorecard-{spec.name}-', dir=work_dir)
    arguments = [str(repo_root / argument) for argument in spec.arguments]
    try:
        done = runner([str(executable), *arguments], cwd=repo_root, capture_output=True, text=True,
                      check=False, env=isolated_env(spec, repo_root, scratch))
    finally:
        shutil.rmtree(scratch, ignore_errors=True)
    metrics = parse_metrics(done.stdout)
    if not metrics:
        raise ScorecardFailed(f'{spec.name}: exit {done.returncode}, no metric line; '
                              f'{done.stderr[-800:]}')
    return ScorecardRun(spec, done.returncode, metrics, done.stdout, done.stderr)
