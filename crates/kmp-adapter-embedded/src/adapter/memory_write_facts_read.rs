//! The facts one memory write reads about its about, point by point
//! (DESIGN L6, write in O(delta)): the same answers the write used to take
//! from the about's depth-1 neighbourhood, without materializing it.

use kmp_domain::{MemoryWriteFacts, MemoryWriteFactsRequest, PortError};

use super::engine::{Key, ReadTx, Table};
use std::borrow::Cow;

use super::serdes::{NodeRecord, decode};

const MEMORY_DIMENSION_KIND: &str = "memory_dimension";
const HAS_DIMENSION: &str = "has_dimension";
const CONTAINS_ENTRY: &str = "contains_entry";
/// Rows read per page of an adjacency.
const PAGE: u32 = 512;
/// Above every relation type: the upper bound of a prefix lookup.
const LAST_TYPE: &str = "\u{10ffff}";

/// Reads `request` over one snapshot.
///
/// - A ref stands one hop from the anchor when an edge from the anchor
///   reaches it (every edge endpoint is a stored node).
/// - The dimensions are the `memory_dimension` nodes the anchor holds by
///   `has_dimension`, which is how every dimension is projected.
/// - A coordinate's frontier is the highest sequence of the `contains_entry`
///   edges out of its scope whose explanation names the same dimension and
///   scope, which is where every coordinate is projected.
pub(super) fn read(
    tx: &dyn ReadTx,
    request: &MemoryWriteFactsRequest,
) -> Result<MemoryWriteFacts, PortError> {
    let about = request.about.as_str();
    if tx.get(Table::Nodes, Key::Str(about))?.is_none() {
        return Ok(MemoryWriteFacts::default());
    }
    let mut facts = MemoryWriteFacts {
        exists: true,
        ..MemoryWriteFacts::default()
    };
    let mut after: Option<(String, String)> = None;
    loop {
        let rows = tx.scan_str3_page(
            Table::Relations,
            about,
            after.as_ref().map(|(a, b)| (a.as_str(), b.as_str())),
            PAGE,
            Some(HAS_DIMENSION),
        )?;
        let full = rows.len() == PAGE as usize;
        for ((_, target, relation), _) in rows {
            after = Some((target.clone(), relation));
            let Some(raw) = tx.get(Table::Nodes, Key::Str(&target))? else {
                continue;
            };
            let node = decode::<NodeRecord>("graph node", &raw)?.into_projection()?;
            if node.node_kind == MEMORY_DIMENSION_KIND {
                facts
                    .dimensions
                    .insert(target, node.properties.get("dimension_kind").cloned());
            }
        }
        if !full {
            break;
        }
    }
    for reference in &request.refs {
        let present = reference == about
            || tx
                .last_str3_before(Table::RelationsByTarget, reference, about, LAST_TYPE)?
                .is_some();
        if present {
            facts.present_refs.insert(reference.clone());
        } else {
            facts.absent_refs.insert(reference.clone());
        }
    }
    for key in &request.sequence_keys {
        facts
            .sequence_frontiers
            .insert(key.clone(), frontier(tx, about, key)?);
    }
    Ok(facts)
}

fn frontier(tx: &dyn ReadTx, about: &str, key: &(String, String)) -> Result<u32, PortError> {
    let (dimension, scope) = key;
    // A scope the anchor does not hold keeps no entry of this about.
    if tx
        .get(Table::Relations, Key::Str3(about, scope, HAS_DIMENSION))?
        .is_none()
    {
        return Ok(0);
    }
    let mut highest = 0u32;
    let mut after: Option<(String, String)> = None;
    loop {
        let rows = tx.scan_str3_page(
            Table::Relations,
            scope,
            after.as_ref().map(|(a, b)| (a.as_str(), b.as_str())),
            PAGE,
            Some(CONTAINS_ENTRY),
        )?;
        let full = rows.len() == PAGE as usize;
        for ((_, target, relation), raw) in rows {
            after = Some((target, relation));
            let coordinate =
                serde_json::from_slice::<StoredCoordinate<'_>>(&raw).map_err(|error| {
                    PortError::InvalidState(format!(
                        "embedded store could not decode relation explanation: {error}"
                    ))
                })?;
            if coordinate.dimension.as_deref() == Some(dimension.as_str())
                && coordinate.scope_id.as_deref() == Some(scope.as_str())
                && let Some(sequence) = coordinate.sequence()?
            {
                highest = highest.max(sequence);
            }
        }
        if !full {
            break;
        }
    }
    Ok(highest)
}

/// The three properties of a stored explanation a frontier reads, borrowed
/// where they need no unescaping: `RelationExplanation::from_properties`
/// reads the same keys (`order` standing in for an absent `sequence`).
#[derive(serde::Deserialize)]
struct StoredCoordinate<'a> {
    #[serde(borrow, default)]
    dimension: Option<Cow<'a, str>>,
    #[serde(borrow, default)]
    scope_id: Option<Cow<'a, str>>,
    #[serde(borrow, default)]
    sequence: Option<Cow<'a, str>>,
    #[serde(borrow, default)]
    order: Option<Cow<'a, str>>,
}

impl StoredCoordinate<'_> {
    fn sequence(&self) -> Result<Option<u32>, PortError> {
        self.sequence
            .as_ref()
            .or(self.order.as_ref())
            .map(|value| {
                value.parse::<u32>().map_err(|error| {
                    PortError::InvalidState(format!("invalid relation sequence `{value}`: {error}"))
                })
            })
            .transpose()
    }
}
