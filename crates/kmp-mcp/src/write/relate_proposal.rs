/// A proposal `kmp_relate` returned, as the writer hands it back: the two
/// facts and the signals that proposed them. It is the proof a writer
/// carries when it declares the one relation that may cross an about.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct RelateProposal {
    pub(crate) from: String,
    pub(crate) to: String,
    pub(crate) proposed_by: Vec<String>,
}
