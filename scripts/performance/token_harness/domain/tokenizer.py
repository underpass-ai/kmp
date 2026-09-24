"""Identity of a reference tokenizer; two counts are comparable only if equal."""
from dataclasses import dataclass


@dataclass(frozen=True)
class TokenizerIdentity:
    library: str
    library_version: str
    encoding: str
    policy: str
    asset_sha256: str
    pattern_sha256: str

    def as_dict(self):
        return {'library': self.library, 'library_version': self.library_version,
                'encoding': self.encoding, 'policy': self.policy,
                'asset_sha256': self.asset_sha256, 'pattern_sha256': self.pattern_sha256}
