//! Which rules a refused call broke, and where: the `code` and `field` of
//! each validation feedback item, never its reason, action or any value.

use serde_json::Value;

/// Items kept in the line; the rest are counted, not listed.
const LISTED: usize = 32;
const CODE_CHARS: usize = 48;
const FIELD_CHARS: usize = 96;
const SEGMENT_CHARS: usize = 40;
/// A field segment that is not a schema-like name (a key the caller
/// invented, say) is logged as this.
const UNNAMED: &str = "?";

/// `feedback[]` of a refused call reduced to `CODE@field` pairs.
#[derive(Debug, Default, Eq, PartialEq)]
pub(crate) struct FeedbackCodes {
    /// Items with a code, listed or not.
    pub(crate) count: usize,
    /// `CODE@field` in the order returned, comma-joined, the field path with
    /// its record indexes (`memories[2].summary_en`); `CODE` alone for a
    /// packet-wide item; `+N` closes a list longer than 32.
    pub(crate) entries: String,
}

impl FeedbackCodes {
    /// `None` when the call returned no feedback with a code.
    pub(crate) fn from_feedback(feedback: &[Value]) -> Option<Self> {
        let pairs: Vec<String> = feedback
            .iter()
            .filter_map(|item| {
                let code = code(item.get("code")?.as_str()?)?;
                Some(
                    match item
                        .get("field")
                        .and_then(Value::as_str)
                        .filter(|path| !path.is_empty())
                        .map(field)
                    {
                        Some(field) if !field.is_empty() => format!("{code}@{field}"),
                        _ => code,
                    },
                )
            })
            .collect();
        if pairs.is_empty() {
            return None;
        }
        let mut entries = pairs.iter().take(LISTED).cloned().collect::<Vec<_>>();
        if pairs.len() > LISTED {
            entries.push(format!("+{}", pairs.len() - LISTED));
        }
        Some(Self {
            count: pairs.len(),
            entries: entries.join(","),
        })
    }
}

fn code(text: &str) -> Option<String> {
    if text.is_empty() {
        return None;
    }
    let valid = text.len() <= CODE_CHARS
        && text
            .bytes()
            .all(|byte| byte.is_ascii_uppercase() || byte.is_ascii_digit() || byte == b'_');
    Some(if valid { text } else { "OTHER" }.to_string())
}

/// Each dot-separated segment kept when it is a lower-case name with
/// optional `[n]` indexes, replaced by `?` otherwise; the whole bounded.
fn field(path: &str) -> String {
    let path = path
        .split('.')
        .map(|segment| {
            if is_named_segment(segment) {
                segment
            } else {
                UNNAMED
            }
        })
        .collect::<Vec<_>>()
        .join(".");
    if path.len() <= FIELD_CHARS {
        path
    } else {
        UNNAMED.to_string()
    }
}

fn is_named_segment(segment: &str) -> bool {
    let name_end = segment.find('[').unwrap_or(segment.len());
    let (name, indexes) = segment.split_at(name_end);
    !name.is_empty()
        && name.len() <= SEGMENT_CHARS
        && name
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'_')
        && indexes.split_terminator(']').all(|index| {
            index.strip_prefix('[').is_some_and(|digits| {
                !digits.is_empty() && digits.bytes().all(|b| b.is_ascii_digit())
            })
        })
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::FeedbackCodes;

    #[test]
    fn keeps_codes_and_field_paths_and_drops_every_text() {
        let codes = FeedbackCodes::from_feedback(&[
            json!({"code": "SUMMARY_EN_REQUIRED", "field": "memories[2].summary_en",
                   "reason": "private reason", "action": {"arguments": {"summary": "private"}}}),
            json!({"code": "LABELS_REQUIRED", "field": "labels"}),
            json!({"code": "WRITE_OPERATION_REQUIRED", "field": ""}),
            json!({"code": "UNKNOWN_ARGUMENT", "field": "memories[0].My Private Key"}),
            json!({"code": "odd code", "field": "connect_to[1].class"}),
            json!({"severity": "error", "reason": "no code"}),
        ])
        .expect("codes");

        assert_eq!(codes.count, 5);
        assert_eq!(
            codes.entries,
            "SUMMARY_EN_REQUIRED@memories[2].summary_en,LABELS_REQUIRED@labels,\
             WRITE_OPERATION_REQUIRED,UNKNOWN_ARGUMENT@memories[0].?,OTHER@connect_to[1].class"
        );
        assert!(!codes.entries.contains("private"));
    }

    #[test]
    fn a_long_list_is_counted_and_cut_and_no_code_is_none() {
        let many: Vec<_> = (0..40)
            .map(|i| json!({"code": "INVALID_TYPE", "field": format!("memories[{i}].kind")}))
            .collect();
        let codes = FeedbackCodes::from_feedback(&many).expect("codes");
        assert_eq!(codes.count, 40);
        assert!(codes.entries.ends_with(",+8"));
        assert_eq!(codes.entries.matches("INVALID_TYPE").count(), 32);

        assert_eq!(FeedbackCodes::from_feedback(&[]), None);
        assert_eq!(FeedbackCodes::from_feedback(&[json!({"field": "x"})]), None);
        let odd = FeedbackCodes::from_feedback(&[json!({"code": "C", "field": "a[x].b[]"})]);
        assert_eq!(odd.expect("codes").entries, "C@?.?");
    }
}
