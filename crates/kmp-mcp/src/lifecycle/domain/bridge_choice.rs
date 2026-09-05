use std::path::PathBuf;

/// What a lifecycle run was asked to do about the lexical-bridge table.
///
/// Installing is the default because the alternative is the silence this
/// choice exists to end: without a table, `ask` matches within one language
/// and says so nowhere except `doctor`.
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
