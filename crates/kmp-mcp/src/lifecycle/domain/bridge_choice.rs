use std::path::PathBuf;

/// What a lifecycle run was asked to do about the lexical-bridge table.
///
/// Installing adds word pairs for retrieval across languages. Stored text
/// and valid writer-supplied English summaries are searchable without it;
/// lifecycle receipts and `doctor` report the table's separate availability.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub enum BridgeChoice {
    /// Install the table this release publishes.
    #[default]
    FromRelease,
    /// Install a table the operator built or already holds.
    FromFile(PathBuf),
    /// Leave the machine's table exactly as it is.
    Declined,
}
