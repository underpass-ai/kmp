/// Reject ref spellings that can escape the graph namespace or become
/// ambiguous when rendered, logged, or used as storage keys.
pub fn validate_ref_token(path: &str, value: &str) -> Result<(), String> {
    let unsafe_character = value
        .chars()
        .any(|ch| ch.is_control() || ch.is_whitespace() || matches!(ch, '/' | '\\'));
    let unsafe_segment = value
        .split(':')
        .any(|segment| segment.is_empty() || matches!(segment, "." | ".."));
    if unsafe_character || unsafe_segment {
        return Err(format!(
            "invalid `{path}` `{value}`; memory refs cannot contain whitespace, control characters, path separators, or empty/dot path segments"
        ));
    }
    Ok(())
}

/// Entry ids are strict descendants of the about they mutate. The about
/// anchor itself is deliberately excluded: ingesting an entry must never
/// change the root node's kind or payload.
pub fn validate_supplied_entry_ref(about: &str, path: &str, entry_ref: &str) -> Result<(), String> {
    validate_ref_token(path, entry_ref)?;
    let owned_prefix = format!("{about}:");
    if !entry_ref.starts_with(&owned_prefix) {
        return Err(format!(
            "`{path}` `{entry_ref}` does not belong to about `{about}`; it must start with `{owned_prefix}` and cannot replace the about anchor or a node from another about. Omit {path} to generate a safe ref for a new memory"
        ));
    }
    Ok(())
}

/// Evidence nodes use a distinct prefix, but remain owned by exactly one
/// about through the entry ref embedded after `evidence:`.
pub fn validate_supplied_evidence_ref(
    about: &str,
    path: &str,
    evidence_ref: &str,
) -> Result<(), String> {
    validate_ref_token(path, evidence_ref)?;
    let owned_prefix = format!("evidence:{about}:");
    if !evidence_ref.starts_with(&owned_prefix) {
        return Err(format!(
            "`{path}` `{evidence_ref}` does not belong to about `{about}`; evidence ids must start with `{owned_prefix}`"
        ));
    }
    Ok(())
}

/// A relation-like ref may name the about anchor, one of its entry nodes, an
/// owned evidence node, or the canonical namespace used for its dimensions.
pub fn validate_supplied_member_ref(
    about: &str,
    path: &str,
    member_ref: &str,
) -> Result<(), String> {
    validate_ref_token(path, member_ref)?;
    let entry_prefix = format!("{about}:");
    let evidence_prefix = format!("evidence:{about}:");
    let dimension_prefix = format!("about:{about}:dimension:");
    if member_ref != about
        && !member_ref.starts_with(&entry_prefix)
        && !member_ref.starts_with(&evidence_prefix)
        && !member_ref.starts_with(&dimension_prefix)
    {
        return Err(format!(
            "`{path}` `{member_ref}` does not belong to about `{about}`"
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{
        validate_ref_token, validate_supplied_entry_ref, validate_supplied_evidence_ref,
        validate_supplied_member_ref,
    };

    const ABOUT: &str = "incident:alfa";
    const HOSTILE_REFS: &[&str] = &[
        "incident:gamma:entry:observation:foreign",
        "incident:beta",
        "incident:alfa:entry:x\nincident:beta:entry:y",
        "../../incident:beta:entry:x",
    ];

    #[test]
    fn every_owned_ref_validator_rejects_the_shared_hostile_vectors() {
        for hostile in HOSTILE_REFS {
            assert!(
                validate_supplied_entry_ref(ABOUT, "entry", hostile).is_err(),
                "entry validator accepted {hostile:?}"
            );
            assert!(
                validate_supplied_evidence_ref(ABOUT, "evidence", hostile).is_err(),
                "evidence validator accepted {hostile:?}"
            );
            assert!(
                validate_supplied_member_ref(ABOUT, "member", hostile).is_err(),
                "member validator accepted {hostile:?}"
            );
        }
    }

    #[test]
    fn owned_graph_namespaces_remain_valid() {
        validate_ref_token("about", ABOUT).expect("safe about");
        validate_supplied_entry_ref(ABOUT, "entry", "incident:alfa:entry:decision:one")
            .expect("owned entry");
        validate_supplied_evidence_ref(
            ABOUT,
            "evidence",
            "evidence:incident:alfa:entry:decision:one:current",
        )
        .expect("owned evidence");
        for member in [
            ABOUT,
            "incident:alfa:entry:decision:one",
            "evidence:incident:alfa:entry:decision:one:current",
            "about:incident:alfa:dimension:agentic_process:run",
        ] {
            validate_supplied_member_ref(ABOUT, "member", member).expect("owned member");
        }
    }
}
