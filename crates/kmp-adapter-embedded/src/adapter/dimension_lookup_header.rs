use kmp_domain::{MemoryDimensionIdentity, PortError};
use serde::Deserialize;
use std::collections::BTreeSet;

use super::serdes::decode;

/// Only the stored fields that admit an endpoint to dimensional lookup.
#[derive(Deserialize)]
pub(super) struct DimensionLookupHeader {
    pub node_id: String,
    pub node_kind: String,
    #[serde(rename = "properties.dimension_kind")]
    dimension_kind: Option<String>,
}

impl DimensionLookupHeader {
    pub fn read(raw: Option<&[u8]>) -> Result<Option<Self>, PortError> {
        raw.map(|raw| decode("dimension lookup header", raw))
            .transpose()
    }

    pub fn matches(&self, terms: &BTreeSet<String>) -> bool {
        self.node_kind == "memory_dimension"
            && (terms.contains(&self.node_id)
                || self
                    .dimension_kind
                    .as_ref()
                    .is_some_and(|kind| terms.contains(kind))
                || MemoryDimensionIdentity::parse(&self.node_id)
                    .is_some_and(|identity| terms.contains(identity.dimension_id())))
    }
}
