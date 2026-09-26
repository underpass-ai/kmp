use sha2::{Digest, Sha256};

/// The prompt template a verdict answers: the call site's stable word and a
/// version bumped whenever what the site asks changes meaning without its
/// text changing. Every verdict key starts with the template's prefix, so a
/// template (all versions) or one version can be dropped from the book by
/// key range.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct VerdictTemplate {
    pub id: &'static str,
    pub version: u16,
}

impl VerdictTemplate {
    /// Bytes of the id's digest that open a key; the version follows.
    pub(crate) const ID_PREFIX_BYTES: usize = 8;
    /// Bytes of the whole template prefix: id digest, then version.
    pub(crate) const PREFIX_BYTES: usize = Self::ID_PREFIX_BYTES + 2;

    /// The prefix every key of every version of this template starts with.
    pub(crate) fn id_prefix(self) -> [u8; Self::ID_PREFIX_BYTES] {
        let digest = Sha256::digest(self.id.as_bytes());
        let mut prefix = [0u8; Self::ID_PREFIX_BYTES];
        prefix.copy_from_slice(&digest[..Self::ID_PREFIX_BYTES]);
        prefix
    }

    /// The prefix every key of exactly this version starts with.
    pub(crate) fn prefix(self) -> [u8; Self::PREFIX_BYTES] {
        let mut prefix = [0u8; Self::PREFIX_BYTES];
        prefix[..Self::ID_PREFIX_BYTES].copy_from_slice(&self.id_prefix());
        prefix[Self::ID_PREFIX_BYTES..].copy_from_slice(&self.version.to_be_bytes());
        prefix
    }
}

#[cfg(test)]
mod tests {
    use super::VerdictTemplate;

    #[test]
    fn versions_share_the_id_prefix_and_differ_after_it() {
        let one = VerdictTemplate {
            id: "rerank",
            version: 1,
        };
        let two = VerdictTemplate { version: 2, ..one };
        assert_eq!(one.prefix()[..8], two.prefix()[..8]);
        assert_ne!(one.prefix(), two.prefix());
        assert_eq!(one.prefix()[8..], [0, 1]);
        let other = VerdictTemplate {
            id: "paths",
            version: 1,
        };
        assert_ne!(one.id_prefix(), other.id_prefix());
    }
}
