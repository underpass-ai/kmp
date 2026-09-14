use super::{
    ClaimIdentity, ConsolidatedClaim, ConsolidatedView, ConsolidationSource, ConsolidationWrite,
    EpistemicStatus,
};
use crate::PortError;
use std::collections::{BTreeMap, BTreeSet};

pub const MAX_SOURCES: usize = 64;
pub const MAX_SOURCE_BYTES: u64 = 1_048_576;
pub const MAX_SOURCE_RELATIONS: u32 = 256;

/// Pure admission. Does not infer identities, clocks or equivalence from text.
pub fn consolidate(
    command: &ConsolidationWrite,
    sources: Vec<ConsolidationSource>,
    authored_at: String,
) -> Result<ConsolidatedView, PortError> {
    let invalid = |text: &str| PortError::InvalidState(text.into());
    for value in [
        &command.about,
        &command.view,
        &command.author,
        &command.idempotency_key,
    ] {
        if value.trim().is_empty() || value.len() > 512 {
            return Err(invalid(
                "view identifiers and author must contain 1..512 bytes",
            ));
        }
    }
    if sources.is_empty() || sources.len() > MAX_SOURCES || command.sources.len() != sources.len() {
        return Err(invalid(
            "a view requires 1..64 distinct source dependencies",
        ));
    }
    let by_ref: BTreeMap<_, _> = sources
        .iter()
        .map(|source| (&source.reference, source))
        .collect();
    for source in &sources {
        if command.sources.get(&source.reference) != Some(&source.stamp) {
            return Err(PortError::Conflict(format!(
                "source moved: {}",
                source.reference
            )));
        }
    }
    if command.assertions.is_empty() || command.assertions.len() > 256 {
        return Err(invalid("a view requires 1..256 source-backed assertions"));
    }
    let mut groups: BTreeMap<ClaimIdentity, Vec<_>> = BTreeMap::new();
    let mut unknown = Vec::new();
    let mut seen = BTreeSet::new();
    for assertion in &command.assertions {
        let source = by_ref
            .get(&assertion.source_ref)
            .ok_or_else(|| invalid("assertion source is not a declared dependency"))?;
        if assertion.quote.trim().is_empty() || !source.body.contains(&assertion.quote) {
            return Err(invalid(
                "assertion quote must be a literal nonempty source passage",
            ));
        }
        if assertion.why.trim().is_empty() || assertion.why.len() > 4096 {
            return Err(invalid("assertion why must contain 1..4096 bytes"));
        }
        let claim = &assertion.claim;
        for value in [
            &claim.referent,
            &claim.predicate,
            &claim.value,
            &claim.temporal_scope,
        ] {
            if value.trim().is_empty() || value.len() > 4096 {
                return Err(invalid(
                    "all claim identity coordinates must contain 1..4096 bytes",
                ));
            }
        }
        if claim.qualifiers.len() > 32
            || claim
                .qualifiers
                .iter()
                .any(|q| q.trim().is_empty() || q.len() > 4096)
        {
            return Err(invalid("at most 32 nonempty qualifiers of 4096 bytes each"));
        }
        // Duplicate supports are not extra corroboration. Do not merge even
        // identical interpretations whose epistemic status is unknown.
        if !seen.insert(assertion) {
            continue;
        }
        if claim.epistemic_status == EpistemicStatus::Unknown {
            unknown.push(ConsolidatedClaim {
                identity: claim.clone(),
                supports: vec![assertion.clone()],
            });
        } else {
            groups
                .entry(claim.clone())
                .or_default()
                .push(assertion.clone());
        }
    }
    let mut claims: Vec<_> = groups
        .into_iter()
        .map(|(identity, mut supports)| {
            supports.sort_by(|a, b| {
                (&a.source_ref, &a.quote, &a.why).cmp(&(&b.source_ref, &b.quote, &b.why))
            });
            ConsolidatedClaim { identity, supports }
        })
        .collect();
    claims.extend(unknown);
    Ok(ConsolidatedView {
        about: command.about.clone(),
        view: command.view.clone(),
        revision: command
            .expect_revision
            .checked_add(1)
            .ok_or_else(|| invalid("view revision overflow"))?,
        authored_at,
        author: command.author.clone(),
        claims,
        sources,
    })
}
