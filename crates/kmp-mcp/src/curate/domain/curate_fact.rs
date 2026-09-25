/// A current fact of the selection: its ref, the about that owns it, its
/// stored text, and the date it occurred when its coordinates say.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct CurateFact {
    pub reference: String,
    pub about: String,
    pub text: String,
    pub occurred: Option<String>,
}
