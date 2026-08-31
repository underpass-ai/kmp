//! Attribution on the wire.

use serde::{Deserialize, Serialize};

/// Who moved the view and why, as the wire spells it — shown to the human,
/// so an agent can never rearrange what they are looking at anonymously.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProvenanceDto {
    /// Who moved it.
    pub actor: String,
    /// Why, when the mover said.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub explanation: Option<String>,
    /// The intent's key, when the move came from one.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub idempotency_key: Option<String>,
    /// When the move landed.
    pub at: String,
}
