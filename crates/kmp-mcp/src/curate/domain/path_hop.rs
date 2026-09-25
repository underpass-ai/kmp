/// One step of a chain between two facts: declared by a writer, or proposed
/// by the judge as the next step and not yet declared.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct PathHop {
    pub from: String,
    pub to: String,
    /// The declared relation, or the type Jev would choose for a proposed hop.
    pub rel: Option<String>,
    pub declared: bool,
    /// Jev's confidence that `to` follows `from`; 1 for a declared hop.
    pub confidence: f64,
    /// True when the walk crosses this hop against its stored direction.
    pub reversed: bool,
}
