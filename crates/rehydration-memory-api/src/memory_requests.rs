use serde::{Deserialize, Serialize};

/// How much of the memory a recall may resolve.
///
/// Mirrors the kernel's resolution ladder without importing it: a summary, the
/// causal spine, or the full evidence pack. The contract names the rungs;
/// what each costs is the implementation's business.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MemoryTier {
    Summary,
    CausalSpine,
    EvidencePack,
}

/// How `ask` may answer when the evidence is thin.
///
/// The default is the strict one. A memory that answers beyond its evidence is
/// worse than one that says it does not know, and a consumer that wants the
/// looser policies must ask for them by name.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum MemoryAnswerPolicy {
    #[default]
    EvidenceOrUnknown,
    ShowConflicts,
    BestEffort,
}

/// Recall the bounded context of one about.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MemoryWakeRequest {
    /// What the memory is about. The kernel's addressing, opaque to it: a
    /// consumer puts its own reference here and gets it back unchanged.
    pub about: String,
    /// Who is asking, for the kernel's own accounting.
    pub role: String,
    /// Why, in a sentence. Carried into the kernel's telemetry so a recall can
    /// be explained later.
    pub intent: String,
    /// Restrict recall to these dimension kinds. Empty means all of them.
    pub dimension_kinds: Vec<String>,
    /// Restrict recall to the current about's own scopes.
    pub scoped_to_about: bool,
    pub token_budget: u32,
    pub depth: u32,
    pub max_tier: Option<MemoryTier>,
    /// Cap on surfaced evidence entries. `None` is unbounded; when set and the
    /// about holds more, the kernel returns the first `max_entries` and says
    /// how much it withheld.
    pub max_entries: Option<u32>,
}

/// Ask one question of the memory.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MemoryAskRequest {
    pub about: String,
    pub question: String,
    pub answer_policy: MemoryAnswerPolicy,
    pub dimension_kinds: Vec<String>,
    pub scoped_to_about: bool,
    pub token_budget: u32,
    pub depth: u32,
    pub max_tier: Option<MemoryTier>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_default_answer_policy_is_the_strict_one() {
        assert_eq!(
            MemoryAnswerPolicy::default(),
            MemoryAnswerPolicy::EvidenceOrUnknown,
            "a memory that answers beyond its evidence is worse than one that \
             says it does not know"
        );
    }

    #[test]
    fn a_request_survives_the_wire() {
        let request = MemoryWakeRequest {
            about: "project:checkout".to_string(),
            role: "resumer".to_string(),
            intent: "resume after restart".to_string(),
            dimension_kinds: vec!["timeline".to_string()],
            scoped_to_about: true,
            token_budget: 4096,
            depth: 2,
            max_tier: Some(MemoryTier::EvidencePack),
            max_entries: Some(50),
        };
        let bytes = serde_json::to_vec(&request).expect("serializes");
        assert_eq!(
            serde_json::from_slice::<MemoryWakeRequest>(&bytes).expect("deserializes"),
            request
        );
    }
}
