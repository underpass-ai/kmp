/// A label membership the judge would give a fact, chosen from the values
/// its about already uses under that key. Never written by itself.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct ProposedLabel {
    pub reference: String,
    pub key: String,
    pub value: String,
    pub confidence: f64,
}
