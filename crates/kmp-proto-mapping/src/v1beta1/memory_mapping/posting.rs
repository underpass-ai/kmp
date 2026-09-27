/// One candidate in a term's posting list: its ordinal inside the about and
/// how often the term occurs in its content and direct fields, plainly and
/// with the alias terms the anchored gate reads.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Posting {
    pub ordinal: u64,
    pub content: u64,
    pub direct: u64,
    pub aliased_content: u64,
    pub aliased_direct: u64,
}

impl Posting {
    /// The counts under a reading: content and direct.
    pub fn counts(&self, aliased: bool) -> (u64, u64) {
        if aliased {
            (self.aliased_content, self.aliased_direct)
        } else {
            (self.content, self.direct)
        }
    }
}
