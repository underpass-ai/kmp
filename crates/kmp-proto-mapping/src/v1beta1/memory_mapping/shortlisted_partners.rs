/// The facts of an about worth asking a judge about one of them (DESIGN L4
/// 4e): those that share a hard anchor with it, then its best BM25 matches.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ShortlistedPartners {
    /// Refs that name one of the fact's hard anchors, in the about's order.
    pub anchored: Vec<String>,
    /// Refs among the best BM25 matches of the fact's text that no anchor
    /// already brought, best first.
    pub lexical: Vec<String>,
    /// The fact's principal anchor as it wrote it: the hard anchor the
    /// fewest other facts name, the first named on a tie.
    pub principal_anchor: Option<String>,
    /// Refs that name the principal anchor, in the about's order.
    pub sharing_principal: Vec<String>,
}

impl ShortlistedPartners {
    /// Every shortlisted ref once: anchored first, then lexical.
    pub fn refs(&self) -> impl Iterator<Item = &String> {
        self.anchored.iter().chain(&self.lexical)
    }
}
