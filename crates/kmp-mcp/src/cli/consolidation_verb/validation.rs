use kmp_domain::consolidation::{ConsolidationWrite, MAX_SOURCE_BYTES, MAX_SOURCES};
use std::collections::BTreeSet;

fn bounded(field: &str, value: &str, max: usize) -> Result<(), String> {
    if value.trim().is_empty() || value.len() > max {
        return Err(format!("{field} must contain 1..{max} bytes"));
    }
    Ok(())
}

pub(super) fn identifier(field: &str, value: &str) -> Result<(), String> {
    bounded(field, value, 512)
}

pub(super) fn source_refs<'a>(refs: impl Iterator<Item = &'a str>) -> Result<(), String> {
    let refs: Vec<_> = refs.collect();
    if refs.is_empty()
        || refs.len() > MAX_SOURCES
        || refs.iter().collect::<BTreeSet<_>>().len() != refs.len()
        || refs.iter().any(|reference| reference.trim().is_empty())
    {
        return Err("capture requires 1..64 distinct nonempty source refs".into());
    }
    Ok(())
}

pub(super) fn write(command: &ConsolidationWrite) -> Result<(), String> {
    for (field, value) in [
        ("about", &command.about),
        ("view", &command.view),
        ("author", &command.author),
        ("idempotency_key", &command.idempotency_key),
    ] {
        identifier(field, value)?;
    }
    if command.expect_revision == u64::MAX {
        return Err("view revision overflow".into());
    }
    source_refs(command.sources.keys().map(String::as_str))?;
    for stamp in command.sources.values() {
        if !stamp.strip_prefix("sha256:").is_some_and(|digest| {
            digest.len() == 64 && digest.bytes().all(|byte| byte.is_ascii_hexdigit())
        }) {
            return Err("source stamp must be a sha256: hex digest".into());
        }
    }
    if command.assertions.is_empty() || command.assertions.len() > 256 {
        return Err("a view requires 1..256 source-backed assertions".into());
    }
    for assertion in &command.assertions {
        if !command.sources.contains_key(&assertion.source_ref) {
            return Err("assertion source is not a declared dependency".into());
        }
        bounded(
            "assertion quote",
            &assertion.quote,
            MAX_SOURCE_BYTES as usize,
        )?;
        bounded("assertion why", &assertion.why, 4096)?;
        let claim = &assertion.claim;
        for value in [
            &claim.referent,
            &claim.predicate,
            &claim.value,
            &claim.temporal_scope,
        ] {
            bounded("claim identity coordinate", value, 4096)?;
        }
        if claim.qualifiers.len() > 32 {
            return Err("at most 32 nonempty qualifiers of 4096 bytes each".into());
        }
        for qualifier in &claim.qualifiers {
            bounded("claim qualifier", qualifier, 4096)?;
        }
    }
    Ok(())
}
