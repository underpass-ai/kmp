/// A current fact of the selection: its ref, the about that owns it, and its
/// stored text.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct CurateFact {
    pub reference: String,
    pub about: String,
    pub text: String,
}
