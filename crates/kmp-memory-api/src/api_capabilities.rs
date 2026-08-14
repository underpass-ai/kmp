use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};

/// What an implementation says it is and what it can do.
///
/// Reported by the implementation, never inferred by the consumer. The point
/// of checking this at startup is that a missing capability surfaces as a
/// message telling the operator what to update — instead of as a failure
/// inside whatever the consumer was doing when it first needed the capability.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ApiCapabilities {
    contract_version: u32,
    library_version: String,
    capabilities: BTreeSet<String>,
}

impl ApiCapabilities {
    pub fn new(
        contract_version: u32,
        library_version: impl Into<String>,
        capabilities: impl IntoIterator<Item = impl Into<String>>,
    ) -> Self {
        Self {
            contract_version,
            library_version: library_version.into(),
            capabilities: capabilities.into_iter().map(Into::into).collect(),
        }
    }

    pub fn contract_version(&self) -> u32 {
        self.contract_version
    }

    pub fn library_version(&self) -> &str {
        &self.library_version
    }

    pub fn supports(&self, capability: &str) -> bool {
        self.capabilities.contains(capability)
    }

    pub fn capabilities(&self) -> impl Iterator<Item = &str> {
        self.capabilities.iter().map(String::as_str)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_report_names_its_contract_its_release_and_what_it_can_do() {
        let report = ApiCapabilities::new(1, "0.1.0", ["wake", "ask"]);
        assert_eq!(report.contract_version(), 1);
        assert_eq!(report.library_version(), "0.1.0");
        assert!(report.supports("wake"));
        assert!(
            !report.supports("forget"),
            "a capability nobody declared must read as absent, not assumed"
        );
    }

    #[test]
    fn a_report_survives_the_wire() {
        let report = ApiCapabilities::new(1, "0.1.0", ["wake"]);
        let bytes = serde_json::to_vec(&report).expect("serializes");
        assert_eq!(
            serde_json::from_slice::<ApiCapabilities>(&bytes).expect("deserializes"),
            report
        );
    }
}
