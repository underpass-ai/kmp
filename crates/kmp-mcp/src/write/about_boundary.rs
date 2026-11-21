//! The about boundary of a raw `kmp_ingest`.

use serde_json::Value;

/// The about boundary of a raw `kmp_ingest`, and nothing else: a relation
/// endpoint or an evidence support that names another about is refused
/// here, before any backend sees it. The kernel admits one relation across
/// abouts — an equivalence a writer declared from a `kmp_relate` proposal —
/// and only `kmp_write_memory` may declare it. Everything else about the
/// payload, dimension aliases included, is the kernel's to judge.
pub(crate) fn reject_refs_outside_about(arguments: &Value) -> Result<(), String> {
    let arguments = arguments
        .as_object()
        .ok_or_else(|| "tool arguments must be a JSON object".to_string())?;
    let about = arguments
        .get("about")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|about| !about.is_empty())
        .ok_or_else(|| "missing required argument `about`".to_string())?;
    let Some(memory) = arguments.get("memory").and_then(Value::as_object) else {
        return Ok(());
    };
    let items = |key: &str| {
        memory
            .get(key)
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default()
    };
    let check = |path: &str, reference: &str| -> Result<(), String> {
        if names_another_about(about, reference) {
            return Err(format!(
                "`{path}` `{reference}` does not belong to about `{about}`; raw kmp_ingest never crosses an about — a writer declares an equivalence with kmp_write_memory"
            ));
        }
        Ok(())
    };
    for relation in items("relations") {
        for (key, path) in [
            ("from", "memory.relations[].from"),
            ("to", "memory.relations[].to"),
        ] {
            if let Some(reference) = relation.get(key).and_then(Value::as_str) {
                check(path, reference)?;
            }
        }
    }
    for evidence in items("evidence") {
        for supported in evidence
            .get("supports")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
        {
            check("memory.evidence[].supports[]", supported)?;
        }
    }
    Ok(())
}

/// Whether a ref names a node of another about: three or more segments
/// whose leading two are not this about. A bare dimension id has fewer,
/// and this about's own refs and namespaced dimensions start with it.
fn names_another_about(about: &str, reference: &str) -> bool {
    let reference = reference.trim();
    if reference == about
        || reference.starts_with(&format!("{about}:"))
        || reference.starts_with(&format!("evidence:{about}:"))
        || kmp_domain::MemoryDimensionIdentity::resolve(about, reference).is_some()
    {
        return false;
    }
    reference.split(':').count() >= 3
}

#[cfg(test)]
mod boundary_tests {
    use super::reject_refs_outside_about;
    use serde_json::json;

    #[test]
    fn a_raw_ingest_may_name_its_own_refs_and_dimensions_but_never_another_about() {
        let own = json!({"about": "service:alpha", "memory": {
            "dimensions": [],
            "entries": [{"id": "claim:short", "kind": "claim", "text": "t"}],
            "relations": [{"from": "conversation:rachel", "to": "service:alpha:claim:x", "rel": "contains_entry", "class": "structural"}],
            "evidence": [{"id": "evidence:service:alpha:e", "supports": ["service:alpha:claim:x", "label:v1:service%3Aalpha:work:work%3Amain"], "text": "t"}]
        }});
        assert!(reject_refs_outside_about(&own).is_ok());

        let foreign = json!({"about": "service:alpha", "memory": {
            "relations": [{"from": "service:alpha:claim:x", "to": "service:beta:outcome:freeze", "rel": "same_event_as", "class": "evidential", "method": "kmp_relate:identifier"}]
        }});
        let error = reject_refs_outside_about(&foreign).expect_err("another about");
        assert!(
            error.contains("does not belong to about `service:alpha`"),
            "{error}"
        );
        assert!(error.contains("kmp_write_memory"), "{error}");
    }
}
