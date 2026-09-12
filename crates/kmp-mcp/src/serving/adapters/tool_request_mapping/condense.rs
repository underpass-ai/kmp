use super::json_fields::JsonFieldReader;
use kmp_proto::v1beta1::CondenseRequest;
use serde_json::Value;

use crate::contract::validator::{optional_string, required_string, validate_required_arguments};

/// Builds the condense request from tool arguments.
///
/// `authored_at` is absent on purpose: the kernel stamps it. A caller-supplied
/// instant could place a card before a cut it never existed at.
pub(crate) struct CondenseRequestMapper;

impl CondenseRequestMapper {
    pub(crate) fn from_arguments(arguments: &Value) -> Result<CondenseRequest, String> {
        // Only the string-valued arguments: this helper reads strings, and
        // `source` and `expect` are objects, checked below with what is wrong
        // about them rather than as a bare absence.
        validate_required_arguments(arguments, &["about", "ref", "language", "scope", "card"])?;
        let source = JsonFieldReader::object(
            arguments.get("source").ok_or("condense requires source")?,
            "source",
        )?;
        let revision = source
            .get("revision")
            .and_then(Value::as_u64)
            .filter(|revision| *revision > 0)
            .ok_or("source.revision is the positive body revision the card was written from")?;
        let record_digest =
            JsonFieldReader::optional_string_field(source, "record_digest", "source")?
                .filter(|digest| !digest.trim().is_empty())
                .ok_or(
                    "source.record_digest identifies the exact body version this card stands for",
                )?;
        let expect = JsonFieldReader::object(
            arguments.get("expect").ok_or("condense requires expect")?,
            "expect",
        )?;
        let expect_absent = expect
            .get("absent")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        let expect_card_revision = expect
            .get("card_revision")
            .and_then(Value::as_u64)
            .unwrap_or(0);
        Ok(CondenseRequest {
            about: required_string(arguments, "about")?,
            r#ref: required_string(arguments, "ref")?,
            language: required_string(arguments, "language")?,
            scope: required_string(arguments, "scope")?,
            card: required_string(arguments, "card")?,
            source_revision: revision,
            source_content_hash: JsonFieldReader::optional_string_field(
                source,
                "content_hash",
                "source",
            )?
            .unwrap_or_default(),
            source_record_digest: record_digest,
            expect_card_revision,
            expect_absent,
            actor: optional_string(arguments, "actor").unwrap_or_default(),
        })
    }
}
