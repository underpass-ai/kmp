use kmp_plugin_api::{
    EvidenceFragment, EvidenceInterpretationInput, EvidenceInterpretationOutput,
    EvidenceValuePlugin, InterpretationError, InterpretedValue, InterpretedValueMention,
    SourceCodeSegmentKind,
};

use crate::text_normalization::{DetectedTextKind, DetectedTextSpan, TextNormalizationPipeline};

#[derive(Debug, Clone, Copy, Default)]
pub struct SourceCodeValuePlugin;

impl EvidenceValuePlugin for SourceCodeValuePlugin {
    fn id(&self) -> &'static str {
        "source-code-value-v1"
    }

    fn interpret(
        &self,
        input: &EvidenceInterpretationInput,
    ) -> Result<EvidenceInterpretationOutput, InterpretationError> {
        let mut values = Vec::new();
        for fragment in &input.fragments {
            values.extend(extract_source_code_mentions(self.id(), fragment));
        }

        Ok(EvidenceInterpretationOutput {
            plugin: self.id().to_string(),
            values,
            diagnostics: Vec::new(),
        })
    }
}

fn extract_source_code_mentions(
    plugin: &str,
    fragment: &EvidenceFragment,
) -> Vec<InterpretedValueMention> {
    let normalized = TextNormalizationPipeline.normalize(&fragment.text);
    normalized
        .spans
        .iter()
        .filter(|span| span.kind == DetectedTextKind::SourceCode)
        .map(|span| source_code_mention(plugin, fragment, span))
        .collect()
}

fn source_code_mention(
    plugin: &str,
    fragment: &EvidenceFragment,
    span: &DetectedTextSpan,
) -> InterpretedValueMention {
    let parsed = parse_source_code_span(&span.raw);
    InterpretedValueMention {
        plugin: plugin.to_string(),
        ref_id: fragment.ref_id.clone(),
        raw: span.raw.clone(),
        span: span.span,
        value: InterpretedValue::source_code(parsed.language, parsed.segment_kind, parsed.text),
        confidence: 1.0,
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ParsedSourceCodeSpan {
    language: Option<String>,
    segment_kind: SourceCodeSegmentKind,
    text: String,
}

fn parse_source_code_span(raw: &str) -> ParsedSourceCodeSpan {
    if raw.starts_with("```") {
        let (declared_language, content) = parse_fenced_code(raw);
        let text = trim_trailing_newline(content).to_string();
        return ParsedSourceCodeSpan {
            language: declared_language.or_else(|| infer_language(&text)),
            segment_kind: SourceCodeSegmentKind::FencedBlock,
            text,
        };
    }

    let text = strip_inline_code_delimiters(raw).to_string();
    ParsedSourceCodeSpan {
        language: infer_language(&text),
        segment_kind: SourceCodeSegmentKind::Inline,
        text,
    }
}

fn parse_fenced_code(raw: &str) -> (Option<String>, &str) {
    let body = raw.strip_prefix("```").unwrap_or(raw);
    let Some(first_newline) = body.find('\n') else {
        return (language_from_fence_info(body), "");
    };

    let fence_info = &body[..first_newline];
    let content_with_closing = &body[first_newline + 1..];
    let content = content_with_closing
        .rsplit_once("```")
        .map(|(content, _)| content)
        .unwrap_or(content_with_closing);

    (language_from_fence_info(fence_info), content)
}

fn strip_inline_code_delimiters(raw: &str) -> &str {
    raw.strip_prefix('`')
        .and_then(|value| value.strip_suffix('`'))
        .unwrap_or(raw)
}

fn trim_trailing_newline(value: &str) -> &str {
    value
        .strip_suffix("\r\n")
        .or_else(|| value.strip_suffix('\n'))
        .unwrap_or(value)
}

fn language_from_fence_info(info: &str) -> Option<String> {
    let token = info
        .split_whitespace()
        .next()?
        .trim_matches(|char| matches!(char, '`' | '{' | '}' | '.'));
    canonical_language(token)
}

fn infer_language(source: &str) -> Option<String> {
    let lower = source.to_ascii_lowercase();
    let trimmed = lower.trim();
    if trimmed.is_empty() {
        return None;
    }

    if trimmed.starts_with("#!/bin/bash")
        || trimmed.starts_with("#!/usr/bin/env bash")
        || trimmed.starts_with("kubectl ")
        || trimmed.starts_with("cargo ")
    {
        return Some("bash".to_string());
    }
    if trimmed.contains("fn ") || trimmed.contains("let mut ") || trimmed.contains("::") {
        return Some("rust".to_string());
    }
    if trimmed.contains("def ") || trimmed.contains("print(") {
        return Some("python".to_string());
    }
    if trimmed.contains("function ") || trimmed.contains("console.log") || trimmed.contains("=>") {
        return Some("javascript".to_string());
    }
    if trimmed.contains("select ") && trimmed.contains(" from ") {
        return Some("sql".to_string());
    }
    if trimmed.contains("<html") || trimmed.contains("</") {
        return Some("html".to_string());
    }

    None
}

fn canonical_language(value: &str) -> Option<String> {
    let normalized = value
        .trim()
        .trim_matches(|char| matches!(char, '`' | '{' | '}' | '.'))
        .to_ascii_lowercase();
    if normalized.is_empty()
        || !normalized
            .chars()
            .all(|char| char.is_ascii_alphanumeric() || matches!(char, '#' | '+' | '-' | '_'))
    {
        return None;
    }

    let canonical = match normalized.as_str() {
        "rs" => "rust",
        "py" => "python",
        "js" | "jsx" => "javascript",
        "ts" | "tsx" => "typescript",
        "sh" | "shell" => "bash",
        "yml" => "yaml",
        "c++" => "cpp",
        "c#" => "csharp",
        other => other,
    };
    Some(canonical.to_string())
}

#[cfg(test)]
mod tests {
    use kmp_plugin_api::{EvidenceInterpretationInput, InterpretedValue, TextSpan};

    use super::*;

    #[test]
    fn source_code_plugin_extracts_fenced_code_language_and_span() {
        let plugin = SourceCodeValuePlugin;
        let text = "Patch:\n```rust\nfn main() {}\n```\nDone.";
        let input =
            EvidenceInterpretationInput::new(vec![EvidenceFragment::new("turn:code", text)]);

        let output = plugin
            .interpret(&input)
            .expect("source code detection should succeed");

        assert_eq!(output.values.len(), 1);
        let mention = &output.values[0];
        assert_eq!(mention.plugin, "source-code-value-v1");
        assert_eq!(mention.ref_id, "turn:code");
        assert_eq!(mention.raw, "```rust\nfn main() {}\n```");
        assert_eq!(mention.span, TextSpan { start: 7, end: 31 });
        assert_eq!(
            mention.value,
            InterpretedValue::SourceCode {
                language: Some("rust".to_string()),
                segment_kind: SourceCodeSegmentKind::FencedBlock,
                text: "fn main() {}".to_string(),
            }
        );
    }

    #[test]
    fn source_code_plugin_canonicalizes_fence_language_aliases() {
        let plugin = SourceCodeValuePlugin;
        let input = EvidenceInterpretationInput::new(vec![EvidenceFragment::new(
            "turn:alias",
            "```rs\nlet value = 1;\n```",
        )]);

        let output = plugin
            .interpret(&input)
            .expect("source code detection should succeed");

        assert_eq!(
            output.values[0].value,
            InterpretedValue::SourceCode {
                language: Some("rust".to_string()),
                segment_kind: SourceCodeSegmentKind::FencedBlock,
                text: "let value = 1;".to_string(),
            }
        );
    }

    #[test]
    fn source_code_plugin_infers_missing_fence_language_conservatively() {
        let plugin = SourceCodeValuePlugin;
        let input = EvidenceInterpretationInput::new(vec![EvidenceFragment::new(
            "turn:infer",
            "```\ndef run():\n    print('ok')\n```",
        )]);

        let output = plugin
            .interpret(&input)
            .expect("source code detection should succeed");

        assert_eq!(
            output.values[0].value,
            InterpretedValue::SourceCode {
                language: Some("python".to_string()),
                segment_kind: SourceCodeSegmentKind::FencedBlock,
                text: "def run():\n    print('ok')".to_string(),
            }
        );
    }

    #[test]
    fn source_code_plugin_extracts_inline_code_without_inventing_language() {
        let plugin = SourceCodeValuePlugin;
        let input = EvidenceInterpretationInput::new(vec![EvidenceFragment::new(
            "turn:inline",
            "Run `foo` after deploy.",
        )]);

        let output = plugin
            .interpret(&input)
            .expect("source code detection should succeed");

        assert_eq!(output.values.len(), 1);
        assert_eq!(output.values[0].raw, "`foo`");
        assert_eq!(
            output.values[0].value,
            InterpretedValue::SourceCode {
                language: None,
                segment_kind: SourceCodeSegmentKind::Inline,
                text: "foo".to_string(),
            }
        );
    }

    #[test]
    fn source_code_plugin_keeps_url_text_inside_code_segment() {
        let plugin = SourceCodeValuePlugin;
        let input = EvidenceInterpretationInput::new(vec![EvidenceFragment::new(
            "turn:code-url",
            "```text\nhttps://inside.test\n```",
        )]);

        let output = plugin
            .interpret(&input)
            .expect("source code detection should succeed");

        assert_eq!(
            output.values[0].value,
            InterpretedValue::SourceCode {
                language: Some("text".to_string()),
                segment_kind: SourceCodeSegmentKind::FencedBlock,
                text: "https://inside.test".to_string(),
            }
        );
    }
}
