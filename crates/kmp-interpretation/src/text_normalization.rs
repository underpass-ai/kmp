use serde::{Deserialize, Serialize};

use kmp_plugin_api::{EvidenceSegmentKind, TextSpan};

pub type DetectedTextKind = EvidenceSegmentKind;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DetectedTextSpan {
    pub kind: DetectedTextKind,
    pub span: TextSpan,
    pub raw: String,
    pub normalized: String,
    pub protected: bool,
}

impl DetectedTextSpan {
    fn new(kind: DetectedTextKind, text: &str, start: usize, end: usize) -> Self {
        let raw = text[start..end].to_string();
        Self {
            kind,
            span: TextSpan { start, end },
            normalized: normalize_fragment(&raw),
            raw,
            protected: kind != DetectedTextKind::Text,
        }
    }

    pub fn is_interpretable_text(&self) -> bool {
        self.kind.is_interpretable_text()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NormalizedText {
    pub raw: String,
    pub normalized: String,
    pub spans: Vec<DetectedTextSpan>,
    pub diagnostics: Vec<String>,
}

impl NormalizedText {
    pub fn interpretable_spans(&self) -> Vec<TextSpan> {
        self.spans
            .iter()
            .filter(|span| span.is_interpretable_text())
            .map(|span| span.span)
            .collect()
    }

    pub fn protected_spans(&self) -> Vec<TextSpan> {
        self.spans
            .iter()
            .filter(|span| span.protected)
            .map(|span| span.span)
            .collect()
    }
}

#[derive(Debug, Clone, Copy, Default)]
pub struct TextNormalizationPipeline;

impl TextNormalizationPipeline {
    pub fn normalize(&self, text: &str) -> NormalizedText {
        let protected = collect_protected_spans(text);
        let spans = split_text_by_protected_spans(text, &protected);
        NormalizedText {
            raw: text.to_string(),
            normalized: normalize_fragment(text),
            spans,
            diagnostics: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ProtectedCandidate {
    kind: DetectedTextKind,
    start: usize,
    end: usize,
    priority: u8,
}

fn collect_protected_spans(text: &str) -> Vec<ProtectedCandidate> {
    let mut candidates = Vec::new();
    collect_fenced_code(text, &mut candidates);
    collect_inline_code(text, &mut candidates);
    collect_math(text, &mut candidates);
    collect_urls(text, &mut candidates);

    candidates.sort_by_key(|candidate| (candidate.start, candidate.priority, candidate.end));

    let mut accepted: Vec<ProtectedCandidate> = Vec::new();
    for candidate in candidates {
        if candidate.start >= candidate.end {
            continue;
        }
        if accepted.iter().any(|span| spans_overlap(candidate, *span)) {
            continue;
        }
        accepted.push(candidate);
    }
    accepted.sort_by_key(|candidate| candidate.start);
    accepted
}

fn collect_fenced_code(text: &str, candidates: &mut Vec<ProtectedCandidate>) {
    let mut search = 0;
    while let Some(relative_start) = text[search..].find("```") {
        let start = search + relative_start;
        let body_start = start + 3;
        let end = text[body_start..]
            .find("```")
            .map(|relative_end| body_start + relative_end + 3)
            .unwrap_or(text.len());
        candidates.push(protected(DetectedTextKind::SourceCode, start, end));
        search = end;
    }
}

fn collect_inline_code(text: &str, candidates: &mut Vec<ProtectedCandidate>) {
    let mut search = 0;
    while let Some(relative_start) = text[search..].find('`') {
        let start = search + relative_start;
        if text[start..].starts_with("```") {
            search = start + 3;
            continue;
        }
        let body_start = start + 1;
        let Some(relative_end) = text[body_start..].find('`') else {
            break;
        };
        let end = body_start + relative_end + 1;
        candidates.push(protected(DetectedTextKind::SourceCode, start, end));
        search = end;
    }
}

fn collect_math(text: &str, candidates: &mut Vec<ProtectedCandidate>) {
    collect_delimited(text, "$$", "$$", DetectedTextKind::Math, candidates);
    collect_delimited(text, "\\(", "\\)", DetectedTextKind::Math, candidates);
    collect_delimited(text, "\\[", "\\]", DetectedTextKind::Math, candidates);

    let mut search = 0;
    while let Some(relative_start) = text[search..].find('$') {
        let start = search + relative_start;
        if text[start..].starts_with("$$") || previous_char(text, start) == Some('\\') {
            search = start + 1;
            continue;
        }
        let body_start = start + 1;
        let Some(relative_end) = text[body_start..].find('$') else {
            break;
        };
        let end = body_start + relative_end + 1;
        let body = &text[body_start..end - 1];
        if looks_like_inline_math(body) {
            candidates.push(protected(DetectedTextKind::Math, start, end));
            search = end;
        } else {
            search = start + 1;
        }
    }
}

fn collect_delimited(
    text: &str,
    open: &str,
    close: &str,
    kind: DetectedTextKind,
    candidates: &mut Vec<ProtectedCandidate>,
) {
    let mut search = 0;
    while let Some(relative_start) = text[search..].find(open) {
        let start = search + relative_start;
        let body_start = start + open.len();
        let end = text[body_start..]
            .find(close)
            .map(|relative_end| body_start + relative_end + close.len())
            .unwrap_or(text.len());
        candidates.push(protected(kind, start, end));
        search = end;
    }
}

fn collect_urls(text: &str, candidates: &mut Vec<ProtectedCandidate>) {
    for prefix in ["https://", "http://"] {
        let mut search = 0;
        while let Some(relative_start) = text[search..].find(prefix) {
            let start = search + relative_start;
            let mut end = text.len();
            for (relative_index, char) in text[start..].char_indices() {
                if char.is_whitespace() {
                    end = start + relative_index;
                    break;
                }
            }
            end = trim_url_end(text, start, end);
            candidates.push(protected(DetectedTextKind::Url, start, end));
            search = end.max(start + prefix.len());
        }
    }
}

fn split_text_by_protected_spans(
    text: &str,
    protected_spans: &[ProtectedCandidate],
) -> Vec<DetectedTextSpan> {
    let mut spans = Vec::new();
    let mut cursor = 0;

    for protected_span in protected_spans {
        if cursor < protected_span.start {
            spans.push(DetectedTextSpan::new(
                DetectedTextKind::Text,
                text,
                cursor,
                protected_span.start,
            ));
        }
        spans.push(DetectedTextSpan::new(
            protected_span.kind,
            text,
            protected_span.start,
            protected_span.end,
        ));
        cursor = protected_span.end;
    }

    if cursor < text.len() {
        spans.push(DetectedTextSpan::new(
            DetectedTextKind::Text,
            text,
            cursor,
            text.len(),
        ));
    }

    if spans.is_empty() {
        spans.push(DetectedTextSpan::new(
            DetectedTextKind::Text,
            text,
            0,
            text.len(),
        ));
    }

    spans
}

fn protected(kind: DetectedTextKind, start: usize, end: usize) -> ProtectedCandidate {
    ProtectedCandidate {
        kind,
        start,
        end,
        priority: kind.precedence(),
    }
}

fn spans_overlap(left: ProtectedCandidate, right: ProtectedCandidate) -> bool {
    left.start < right.end && right.start < left.end
}

fn looks_like_inline_math(body: &str) -> bool {
    let trimmed = body.trim();
    if trimmed.is_empty() || trimmed.contains('\n') || trimmed.len() > 120 {
        return false;
    }
    if trimmed.chars().any(|char| {
        matches!(
            char,
            '\\' | '=' | '+' | '-' | '*' | '/' | '^' | '_' | '{' | '}' | '<' | '>'
        )
    }) {
        return true;
    }
    if trimmed
        .chars()
        .all(|char| char.is_ascii_digit() || char == '.' || char == ',' || char.is_whitespace())
    {
        return true;
    }
    !trimmed.contains(char::is_whitespace)
        && trimmed.chars().all(|char| char.is_ascii_alphanumeric())
        && trimmed.chars().any(|char| char.is_ascii_alphabetic())
}

fn normalize_fragment(value: &str) -> String {
    let mut normalized = String::with_capacity(value.len());
    let mut previous_was_space = false;
    for char in value.chars() {
        let next = match char {
            '\u{00a0}' | '\u{2007}' | '\u{202f}' => ' ',
            '\u{2018}' | '\u{2019}' => '\'',
            '\u{201c}' | '\u{201d}' => '"',
            _ if char.is_whitespace() => ' ',
            _ => char,
        };
        if next == ' ' {
            if !previous_was_space {
                normalized.push(next);
            }
            previous_was_space = true;
        } else {
            normalized.push(next);
            previous_was_space = false;
        }
    }
    normalized.trim().to_string()
}

fn trim_url_end(text: &str, start: usize, mut end: usize) -> usize {
    while end > start {
        let Some((char_start, char)) = previous_char_with_start(text, end) else {
            break;
        };
        if matches!(char, '.' | ',' | ';' | ':' | '!' | '?' | ')') {
            end = char_start;
        } else {
            break;
        }
    }
    end
}

fn previous_char(text: &str, boundary: usize) -> Option<char> {
    previous_char_with_start(text, boundary).map(|(_, char)| char)
}

fn previous_char_with_start(text: &str, boundary: usize) -> Option<(usize, char)> {
    text[..boundary].char_indices().next_back()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizer_detects_text_code_math_and_urls() {
        let pipeline = TextNormalizationPipeline;
        let normalized = pipeline.normalize(
            "Budget $70, formula $2n$, code `$90`, and https://example.test/path?x=2026-05-06.",
        );

        assert!(
            normalized
                .spans
                .iter()
                .any(|span| span.kind == DetectedTextKind::Text && span.raw.contains("Budget"))
        );
        assert!(
            normalized
                .spans
                .iter()
                .any(|span| span.kind == DetectedTextKind::Math && span.raw == "$2n$")
        );
        assert!(
            normalized
                .spans
                .iter()
                .any(|span| span.kind == DetectedTextKind::SourceCode && span.raw == "`$90`")
        );
        assert!(
            normalized
                .spans
                .iter()
                .any(|span| span.kind == DetectedTextKind::Url
                    && span.raw == "https://example.test/path?x=2026-05-06")
        );
    }

    #[test]
    fn inline_currency_without_closing_delimiter_stays_text() {
        let pipeline = TextNormalizationPipeline;
        let text = "Budget is $70 and shipping is $20.";
        let normalized = pipeline.normalize(text);

        assert_eq!(normalized.protected_spans(), Vec::new());
        assert_eq!(
            normalized.interpretable_spans(),
            vec![TextSpan {
                start: 0,
                end: text.len()
            }]
        );
    }
}
