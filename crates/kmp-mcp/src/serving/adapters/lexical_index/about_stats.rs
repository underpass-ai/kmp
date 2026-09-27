use kmp_proto_mapping::v1beta1::LanguageSignals;

/// An about's totals in the lexical sidecar (DESIGN L6 `LexStats`).
///
/// `documents`, the three part lengths and `rows_digest` are what BM25's
/// collection statistics and the shadow comparison read: N, Σlen per field
/// and the wrapping sum of every row's fingerprint. `signals` and
/// `summaries` decide the about's language as the ranker would, and
/// `language` is the one its rows were read in. `next_ordinal` numbers the
/// next candidate for the postings.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub(super) struct AboutStats {
    pub(super) documents: u64,
    pub(super) text_length: i64,
    pub(super) summary_length: i64,
    pub(super) extra_length: i64,
    pub(super) rows_digest: u64,
    pub(super) next_ordinal: u64,
    pub(super) signals: LanguageSignals,
    pub(super) summaries: u64,
    pub(super) language: Option<String>,
}

const STATS_LAYOUT: &str = "1";

impl AboutStats {
    pub(super) fn content_length(&self) -> i64 {
        self.text_length + self.summary_length
    }

    pub(super) fn direct_length(&self) -> i64 {
        self.content_length() + self.extra_length
    }

    pub(super) fn encode(&self) -> String {
        serde_json::json!({
            "layout": STATS_LAYOUT,
            "documents": self.documents,
            "text_length": self.text_length,
            "summary_length": self.summary_length,
            "extra_length": self.extra_length,
            "rows_digest": self.rows_digest.to_string(),
            "next_ordinal": self.next_ordinal,
            "signals": hex(&self.signals.encode()),
            "summaries": self.summaries,
            "language": self.language,
        })
        .to_string()
    }

    pub(super) fn decode(text: &str) -> Result<Self, String> {
        let value: serde_json::Value = serde_json::from_str(text).map_err(|e| e.to_string())?;
        if value.get("layout").and_then(serde_json::Value::as_str) != Some(STATS_LAYOUT) {
            return Err("lexical about stats have an unknown layout".into());
        }
        let unsigned = |key: &str| {
            value
                .get(key)
                .and_then(serde_json::Value::as_u64)
                .ok_or_else(|| format!("lexical about stats lack `{key}`"))
        };
        let signed = |key: &str| {
            value
                .get(key)
                .and_then(serde_json::Value::as_i64)
                .ok_or_else(|| format!("lexical about stats lack `{key}`"))
        };
        Ok(Self {
            documents: unsigned("documents")?,
            text_length: signed("text_length")?,
            summary_length: signed("summary_length")?,
            extra_length: signed("extra_length")?,
            rows_digest: value
                .get("rows_digest")
                .and_then(serde_json::Value::as_str)
                .and_then(|digest| digest.parse().ok())
                .ok_or("lexical about stats lack `rows_digest`")?,
            next_ordinal: unsigned("next_ordinal")?,
            signals: LanguageSignals::decode(&unhex(
                value
                    .get("signals")
                    .and_then(serde_json::Value::as_str)
                    .ok_or("lexical about stats lack `signals`")?,
            )?)?,
            summaries: unsigned("summaries")?,
            language: value
                .get("language")
                .and_then(serde_json::Value::as_str)
                .map(str::to_string),
        })
    }
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn unhex(text: &str) -> Result<Vec<u8>, String> {
    if !text.len().is_multiple_of(2) {
        return Err("odd hex length".into());
    }
    (0..text.len())
        .step_by(2)
        .map(|at| u8::from_str_radix(&text[at..at + 2], 16).map_err(|e| e.to_string()))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stats_round_trip() {
        let stats = AboutStats {
            documents: 3,
            text_length: 30,
            summary_length: -1,
            extra_length: 12,
            rows_digest: u64::MAX - 7,
            next_ordinal: 9,
            signals: LanguageSignals::of_texts(["la válvula de la noche"]),
            summaries: 1,
            language: Some("spanish".into()),
        };
        assert_eq!(AboutStats::decode(&stats.encode()).expect("fixture"), stats);
        assert_eq!((stats.content_length(), stats.direct_length()), (29, 41));
        assert!(AboutStats::decode("{}").is_err());
    }
}
