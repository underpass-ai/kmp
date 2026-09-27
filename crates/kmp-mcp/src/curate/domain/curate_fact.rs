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
    /// Its label memberships, `(key, value)`: the dimensions its coordinates
    /// stand in.
    pub labels: Vec<(String, String)>,
}
