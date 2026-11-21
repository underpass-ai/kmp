/// A current fact of the selection: its ref, the about that owns it, its
/// entry kind, its stored text, and the date it occurred when its coordinates say.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct CurateFact {
    pub reference: String,
    pub about: String,
    /// Its entry kind (`decision`, `observation`, ...), empty when unknown.
    pub kind: String,
    pub text: String,
    pub occurred: Option<String>,
    /// When it occurred, in Unix seconds: what tells which of two facts was
    /// recorded first, finer than the date the judge reads in front of it.
    pub occurred_at: Option<i64>,
    /// Its label memberships, `(key, value)`: the dimensions its coordinates
    /// stand in.
    pub labels: Vec<(String, String)>,
}
