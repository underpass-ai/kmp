"""From a variant TOML to the arms a section runs: binary, store, Jev fixture.

`ArmSpec` is a loaded variant with its binary resolved (a path as it is, a
`git_ref` built in a worktree with provenance, `runtime.git_build`). A section
turns a spec into a `run_questions.Arm` on its own store; a variant with
`jev = replay | record` also gets the Jev fixture of that arm on that store and
question set (`runtime.jev_fixture`), so its samples replay or record their own
cassette (or book) instead of whatever the variant names.
"""
from dataclasses import dataclass, field
import os
from pathlib import Path
import sys

from ..domain.errors import BenchError
from ..domain.jsonl import REPO_ROOT
from ..domain.question import questions_digest
from ..domain.variant import load_variant
from ..runtime import git_build, jev_fixture
from .run_questions import Arm

JEV_BACKEND = jev_fixture.CASSETTE  # every binary up to P5 (no verdict book yet)


class ArmRefused(BenchError):
    code = 'ARM_REFUSED'


@dataclass(frozen=True)
class ArmSpec:
    variant: object  # domain.variant.Variant
    path: str  # the TOML, relative to the repository root when inside it
    binary: git_build.ResolvedBinary
    secrets: dict = field(default_factory=dict, repr=False)

    @property
    def name(self):
        return self.variant.name

    def identity(self):
        """What makes two arms the same arm: binary and behaviour-changing configuration."""
        return (self.binary.sha256, self.variant.config_digest())

    def provenance(self):
        return {'variant': self.variant.name, 'path': self.path, 'claim': self.variant.claim,
                'jev': self.variant.jev, 'store': self.variant.store,
                'binary_sha256': self.binary.sha256, 'binary_version': self.binary.version,
                'binary_provenance': self.binary.provenance,
                'config_digest': self.variant.config_digest(),
                'preregistration_digest': self.variant.preregistration_digest()}


def _relative(path, repo_root):
    path = Path(path).resolve()
    try:
        return path.relative_to(Path(repo_root).resolve()).as_posix()
    except ValueError:
        return str(path)


def load_spec(path, layout, repo_root=REPO_ROOT, record_secrets=False, log=sys.stderr):
    """A variant with its binary resolved; a Jev record hands the provider key over as a secret."""
    variant = load_variant(path, repo_root)
    binary = git_build.resolve_variant_binary(variant, layout, repo_root, log=log)
    secrets = {}
    if record_secrets and variant.jev == 'record' and os.environ.get(jev_fixture.API_KEY_ENV):
        secrets[jev_fixture.API_KEY_ENV] = os.environ[jev_fixture.API_KEY_ENV]
    return ArmSpec(variant, _relative(path, repo_root), binary, secrets)


def fixture_for(spec, layout, store_key, questions):
    """The Jev fixture of this arm on this store and question set, or None for `jev = off`."""
    variant = spec.variant
    if variant.jev == 'off':
        return None
    key = jev_fixture.fixture_key(binary_sha256=spec.binary.sha256,
                                  judged=jev_fixture.judged_config(variant), store_key=store_key,
                                  questions_digest=questions_digest(questions), backend=JEV_BACKEND)
    return jev_fixture.open_fixture(layout, key, variant.jev, JEV_BACKEND, variant_env=variant.env_map())


def arm_on(spec, store, layout, questions):
    """The run_questions arm of `spec` on `store` (a run_questions.StoreRef)."""
    if spec.variant.jev == 'record' and jev_fixture.API_KEY_ENV not in spec.secrets:
        raise ArmRefused(f'{spec.name}: jev = "record" asks the real provider; export '
                         f'{jev_fixture.API_KEY_ENV} and run the `jev` command with --record')
    return Arm(spec.variant, spec.path, spec.binary.path, store, spec.binary.version,
               spec.binary.provenance, dict(spec.secrets), fixture_for(spec, layout, store.key, questions))
