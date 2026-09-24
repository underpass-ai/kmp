use crate::curate::application::jev_usage::JevUsage;
use crate::curate::application::prepared_relation::PreparedRelation;
use crate::curate::domain::apply_doubt::ApplyDoubt;
use crate::curate::domain::apply_rejection::ApplyRejection;

/// What an apply may write, what Jev asks the agent to look at again, and
/// what this call cannot write at all.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct PreparedApply {
    pub relations: Vec<PreparedRelation>,
    pub doubted: Vec<ApplyDoubt>,
    pub rejected: Vec<ApplyRejection>,
    pub jev: Option<JevUsage>,
    pub warnings: Vec<String>,
}
