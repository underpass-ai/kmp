use serde_json::{Map, Value};

use sha2::{Digest, Sha256};

pub(super) const GENERATED_REF_SEGMENT_MAX: usize = 80;
pub(super) const GENERATED_REF_HASH_LEN: usize = 16;
pub(super) const GENERATED_REF_SLUG_MAX: usize =
    GENERATED_REF_SEGMENT_MAX - GENERATED_REF_HASH_LEN - 1;

pub(super) fn generated_entry_ref(
    about: &str,
    kind: &str,
    summary: &str,
    write_identity: &str,
    role: &str,
) -> String {
    let slug = sanitize_ref_segment(summary);
    let identity = short_hash(&format!("{write_identity}\0{role}"));
    let suffix = if slug.is_empty() {
        identity
    } else {
        format!("{slug}-{identity}")
    };
    format!("{about}:entry:{kind}:{suffix}")
}

pub(super) fn stable_idempotency_key(arguments: &Map<String, Value>) -> String {
    let mut stable = Value::Object(arguments.clone());
    if let Some(options) = stable.get_mut("options").and_then(Value::as_object_mut) {
        options.remove("dry_run");
    }
    format!("write:{}", short_hash(&stable.to_string()))
}

pub(super) fn short_hash(value: &str) -> String {
    let digest = Sha256::digest(value.as_bytes());
    format!("{digest:x}").chars().take(16).collect()
}

pub(super) fn sanitize_ref_segment(input: &str) -> String {
    let mut output = String::with_capacity(input.len().min(GENERATED_REF_SLUG_MAX));
    let mut previous_was_separator = false;
    for ch in input.trim().chars() {
        let normalized = if ch.is_ascii_alphanumeric() {
            ch.to_ascii_lowercase()
        } else {
            '-'
        };

        if normalized == '-' {
            if !previous_was_separator {
                output.push(normalized);
            }
            previous_was_separator = true;
        } else {
            output.push(normalized);
            previous_was_separator = false;
        }
        if output.len() >= GENERATED_REF_SLUG_MAX {
            break;
        }
    }
    output.trim_matches('-').to_string()
}
