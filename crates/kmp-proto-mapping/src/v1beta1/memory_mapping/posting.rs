/// One candidate in a term's posting list: its ordinal inside the about and
/// how often the term occurs in its content and direct fields.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Posting {
    pub ordinal: u64,
    pub content: u64,
    pub direct: u64,
}
