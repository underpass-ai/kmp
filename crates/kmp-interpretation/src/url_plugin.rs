use kmp_plugin_api::{
    EvidenceFragment, EvidenceInterpretationInput, EvidenceInterpretationOutput,
    EvidenceValuePlugin, InterpretationError, InterpretedValue, InterpretedValueMention,
};

use crate::text_normalization::{DetectedTextKind, DetectedTextSpan, TextNormalizationPipeline};

#[derive(Debug, Clone, Copy, Default)]
pub struct UrlValuePlugin;

impl EvidenceValuePlugin for UrlValuePlugin {
    fn id(&self) -> &'static str {
        "url-value-v1"
    }

    fn interpret(
        &self,
        input: &EvidenceInterpretationInput,
    ) -> Result<EvidenceInterpretationOutput, InterpretationError> {
        let mut values = Vec::new();
        for fragment in &input.fragments {
            values.extend(extract_url_mentions(self.id(), fragment));
        }

        Ok(EvidenceInterpretationOutput {
            plugin: self.id().to_string(),
            values,
            diagnostics: Vec::new(),
        })
    }
}

fn extract_url_mentions(plugin: &str, fragment: &EvidenceFragment) -> Vec<InterpretedValueMention> {
    let normalized = TextNormalizationPipeline.normalize(&fragment.text);
    normalized
        .spans
        .iter()
        .filter(|span| span.kind == DetectedTextKind::Url)
        .map(|span| url_mention(plugin, fragment, span))
        .collect()
}

fn url_mention(
    plugin: &str,
    fragment: &EvidenceFragment,
    span: &DetectedTextSpan,
) -> InterpretedValueMention {
    InterpretedValueMention {
        plugin: plugin.to_string(),
        ref_id: fragment.ref_id.clone(),
        raw: span.raw.clone(),
        span: span.span,
        value: InterpretedValue::url(span.raw.clone()),
        confidence: 1.0,
    }
}

#[cfg(test)]
mod tests {
    use kmp_plugin_api::{EvidenceInterpretationInput, InterpretedValue, TextSpan};

    use super::*;

    #[test]
    fn url_plugin_extracts_url_text_and_span() {
        let plugin = UrlValuePlugin;
        let text = "Docs: https://example.test/path?x=1.";
        let input = EvidenceInterpretationInput::new(vec![EvidenceFragment::new("turn:url", text)]);

        let output = plugin
            .interpret(&input)
            .expect("url detection should succeed");

        assert_eq!(output.values.len(), 1);
        let mention = &output.values[0];
        assert_eq!(mention.plugin, "url-value-v1");
        assert_eq!(mention.ref_id, "turn:url");
        assert_eq!(mention.raw, "https://example.test/path?x=1");
        assert_eq!(mention.span, TextSpan { start: 6, end: 35 });
        assert_eq!(
            mention.value,
            InterpretedValue::Url {
                url: "https://example.test/path?x=1".to_string(),
            }
        );
    }

    #[test]
    fn url_plugin_keeps_multiple_url_segments_distinct() {
        let plugin = UrlValuePlugin;
        let input = EvidenceInterpretationInput::new(vec![EvidenceFragment::new(
            "turn:urls",
            "Open https://a.test and http://b.test/page",
        )]);

        let output = plugin
            .interpret(&input)
            .expect("url detection should succeed");

        let urls = output
            .values
            .iter()
            .map(|mention| mention.raw.as_str())
            .collect::<Vec<_>>();
        assert_eq!(urls, vec!["https://a.test", "http://b.test/page"]);
    }

    #[test]
    fn url_plugin_does_not_reinterpret_urls_inside_code_segments() {
        let plugin = UrlValuePlugin;
        let input = EvidenceInterpretationInput::new(vec![EvidenceFragment::new(
            "turn:code-url",
            "Code ```text\nhttps://inside.test\n``` then https://outside.test.",
        )]);

        let output = plugin
            .interpret(&input)
            .expect("url detection should succeed");

        let urls = output
            .values
            .iter()
            .map(|mention| mention.raw.as_str())
            .collect::<Vec<_>>();
        assert_eq!(urls, vec!["https://outside.test"]);
    }
}
