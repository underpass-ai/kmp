use kmp_plugin_api::{
    EvidenceFragment, EvidenceInterpretationInput, EvidenceInterpretationOutput,
    EvidenceValuePlugin, InterpretationError, InterpretedValue, InterpretedValueMention,
    MathExpressionNotation,
};

use crate::text_normalization::{DetectedTextKind, DetectedTextSpan, TextNormalizationPipeline};

#[derive(Debug, Clone, Copy, Default)]
pub struct MathExpressionValuePlugin;

impl EvidenceValuePlugin for MathExpressionValuePlugin {
    fn id(&self) -> &'static str {
        "math-expression-value-v1"
    }

    fn interpret(
        &self,
        input: &EvidenceInterpretationInput,
    ) -> Result<EvidenceInterpretationOutput, InterpretationError> {
        let mut values = Vec::new();
        for fragment in &input.fragments {
            values.extend(extract_math_mentions(self.id(), fragment));
        }

        Ok(EvidenceInterpretationOutput {
            plugin: self.id().to_string(),
            values,
            diagnostics: Vec::new(),
        })
    }
}

fn extract_math_mentions(
    plugin: &str,
    fragment: &EvidenceFragment,
) -> Vec<InterpretedValueMention> {
    let normalized = TextNormalizationPipeline.normalize(&fragment.text);
    normalized
        .spans
        .iter()
        .filter(|span| span.kind == DetectedTextKind::Math)
        .map(|span| math_mention(plugin, fragment, span))
        .collect()
}

fn math_mention(
    plugin: &str,
    fragment: &EvidenceFragment,
    span: &DetectedTextSpan,
) -> InterpretedValueMention {
    let parsed = parse_math_span(&span.raw);
    InterpretedValueMention {
        plugin: plugin.to_string(),
        ref_id: fragment.ref_id.clone(),
        raw: span.raw.clone(),
        span: span.span,
        value: InterpretedValue::math_expression(parsed.notation, parsed.expression),
        confidence: 1.0,
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ParsedMathExpression {
    notation: MathExpressionNotation,
    expression: String,
}

fn parse_math_span(raw: &str) -> ParsedMathExpression {
    let (notation, expression) = if raw.starts_with("$$") {
        (
            MathExpressionNotation::DisplayDollar,
            strip_math_delimiters(raw, "$$", "$$"),
        )
    } else if raw.starts_with("\\(") {
        (
            MathExpressionNotation::InlineParen,
            strip_math_delimiters(raw, "\\(", "\\)"),
        )
    } else if raw.starts_with("\\[") {
        (
            MathExpressionNotation::DisplayBracket,
            strip_math_delimiters(raw, "\\[", "\\]"),
        )
    } else {
        (
            MathExpressionNotation::InlineDollar,
            strip_math_delimiters(raw, "$", "$"),
        )
    };

    ParsedMathExpression {
        notation,
        expression: expression.trim().to_string(),
    }
}

fn strip_math_delimiters<'a>(raw: &'a str, open: &str, close: &str) -> &'a str {
    let without_open = raw.strip_prefix(open).unwrap_or(raw);
    without_open.strip_suffix(close).unwrap_or(without_open)
}

#[cfg(test)]
mod tests {
    use kmp_plugin_api::{
        EvidenceInterpretationInput, InterpretedValue, MathExpressionNotation, TextSpan,
    };

    use super::*;

    #[test]
    fn math_plugin_extracts_inline_dollar_expression_and_span() {
        let plugin = MathExpressionValuePlugin;
        let text = "Formula $2n + 1$ done.";
        let input =
            EvidenceInterpretationInput::new(vec![EvidenceFragment::new("turn:math", text)]);

        let output = plugin
            .interpret(&input)
            .expect("math extraction should succeed");

        assert_eq!(output.values.len(), 1);
        let mention = &output.values[0];
        assert_eq!(mention.plugin, "math-expression-value-v1");
        assert_eq!(mention.ref_id, "turn:math");
        assert_eq!(mention.raw, "$2n + 1$");
        assert_eq!(mention.span, TextSpan { start: 8, end: 16 });
        assert_eq!(
            mention.value,
            InterpretedValue::MathExpression {
                notation: MathExpressionNotation::InlineDollar,
                expression: "2n + 1".to_string(),
            }
        );
    }

    #[test]
    fn math_plugin_preserves_display_and_latex_delimiter_notation() {
        let plugin = MathExpressionValuePlugin;
        let input = EvidenceInterpretationInput::new(vec![EvidenceFragment::new(
            "turn:math-delimiters",
            "Block $$\na=b\n$$ and inline \\(x+y\\) plus display \\[z^2\\].",
        )]);

        let output = plugin
            .interpret(&input)
            .expect("math extraction should succeed");

        let values = output
            .values
            .iter()
            .map(|mention| &mention.value)
            .collect::<Vec<_>>();
        assert_eq!(
            values,
            vec![
                &InterpretedValue::MathExpression {
                    notation: MathExpressionNotation::DisplayDollar,
                    expression: "a=b".to_string(),
                },
                &InterpretedValue::MathExpression {
                    notation: MathExpressionNotation::InlineParen,
                    expression: "x+y".to_string(),
                },
                &InterpretedValue::MathExpression {
                    notation: MathExpressionNotation::DisplayBracket,
                    expression: "z^2".to_string(),
                },
            ]
        );
    }

    #[test]
    fn math_plugin_does_not_reinterpret_math_inside_code_segments() {
        let plugin = MathExpressionValuePlugin;
        let input = EvidenceInterpretationInput::new(vec![EvidenceFragment::new(
            "turn:code-math",
            "Code `$2n$` then formula $3n$.",
        )]);

        let output = plugin
            .interpret(&input)
            .expect("math extraction should succeed");

        let expressions = output
            .values
            .iter()
            .map(|mention| mention.raw.as_str())
            .collect::<Vec<_>>();
        assert_eq!(expressions, vec!["$3n$"]);
    }

    #[test]
    fn math_plugin_takes_precedence_over_urls_inside_math_segments() {
        let plugin = MathExpressionValuePlugin;
        let input = EvidenceInterpretationInput::new(vec![EvidenceFragment::new(
            "turn:math-url",
            "Expression $https://example.test/a/b$ then https://outside.test.",
        )]);

        let output = plugin
            .interpret(&input)
            .expect("math extraction should succeed");

        assert_eq!(output.values.len(), 1);
        assert_eq!(output.values[0].raw, "$https://example.test/a/b$");
        assert_eq!(
            output.values[0].value,
            InterpretedValue::MathExpression {
                notation: MathExpressionNotation::InlineDollar,
                expression: "https://example.test/a/b".to_string(),
            }
        );
    }
}
