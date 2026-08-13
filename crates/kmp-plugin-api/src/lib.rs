//! Public plugin API for the Rehydration Kernel.
//!
//! This crate is intentionally small. A plugin implementation can depend on it
//! without depending on kernel domain aggregates, ports, adapters, storage
//! clients, gRPC, MCP, or runtime infrastructure.
//!
//! The kernel owns this contract. Runtime crates may re-export it through
//! `kmp_domain::plugins`, but external plugin crates should prefer
//! depending on `kmp-plugin-api` directly.
//!
//! Current plugin contracts are compile-time Rust traits. They are reusable
//! crate boundaries, not a dynamic ABI or runtime plugin registry.
//!
//! Architecture:
//!
//! - value plugins implement [`EvidenceValuePlugin`] and convert retrieved
//!   evidence fragments into typed mentions;
//! - derivation plugins implement [`EvidenceDerivationPlugin`] and compute
//!   deterministic results from explicit operands;
//! - readers or agents decide which mentions are included, excluded, or kept as
//!   context for the current question;
//! - the kernel remains responsible for storage, traversal, refs, provenance,
//!   trace, and inspect, not for domain arithmetic or preference semantics.

use std::error::Error;
use std::fmt;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EvidenceFragment {
    pub ref_id: String,
    pub text: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
}

impl EvidenceFragment {
    pub fn new(ref_id: impl Into<String>, text: impl Into<String>) -> Self {
        Self {
            ref_id: ref_id.into(),
            text: text.into(),
            source: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EvidenceInterpretationInput {
    pub fragments: Vec<EvidenceFragment>,
}

impl EvidenceInterpretationInput {
    pub fn new(fragments: Vec<EvidenceFragment>) -> Self {
        Self { fragments }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EvidenceInterpretationOutput {
    pub plugin: String,
    pub values: Vec<InterpretedValueMention>,
    pub diagnostics: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct InterpretedValueMention {
    pub plugin: String,
    pub ref_id: String,
    pub raw: String,
    pub span: TextSpan,
    pub value: InterpretedValue,
    pub confidence: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct TextSpan {
    pub start: usize,
    pub end: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EvidenceSegmentKind {
    SourceCode,
    Math,
    Url,
    Text,
}

impl EvidenceSegmentKind {
    pub fn precedence(self) -> u8 {
        match self {
            Self::SourceCode => 0,
            Self::Math => 1,
            Self::Url => 2,
            Self::Text => 3,
        }
    }

    pub fn is_interpretable_text(self) -> bool {
        self == Self::Text
    }

    pub fn is_protected(self) -> bool {
        !self.is_interpretable_text()
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum InterpretedValue {
    Money {
        currency: CurrencyCode,
        amount_minor: i64,
        amount: f64,
    },
    Date {
        date: CalendarDate,
    },
    Number {
        value: f64,
        #[serde(skip_serializing_if = "Option::is_none")]
        unit: Option<String>,
    },
    MathExpression {
        notation: MathExpressionNotation,
        expression: String,
    },
    SourceCode {
        #[serde(skip_serializing_if = "Option::is_none")]
        language: Option<String>,
        segment_kind: SourceCodeSegmentKind,
        text: String,
    },
    Url {
        url: String,
    },
}

impl InterpretedValue {
    pub fn number(value: f64, unit: Option<String>) -> Self {
        Self::Number { value, unit }
    }

    pub fn math_expression(
        notation: MathExpressionNotation,
        expression: impl Into<String>,
    ) -> Self {
        Self::MathExpression {
            notation,
            expression: expression.into(),
        }
    }

    pub fn source_code(
        language: Option<String>,
        segment_kind: SourceCodeSegmentKind,
        text: impl Into<String>,
    ) -> Self {
        Self::SourceCode {
            language,
            segment_kind,
            text: text.into(),
        }
    }

    pub fn url(url: impl Into<String>) -> Self {
        Self::Url { url: url.into() }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MathExpressionNotation {
    InlineDollar,
    DisplayDollar,
    InlineParen,
    DisplayBracket,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SourceCodeSegmentKind {
    FencedBlock,
    Inline,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct CurrencyCode(String);

impl CurrencyCode {
    pub fn new(value: impl AsRef<str>) -> Result<Self, InterpretationError> {
        let normalized = value.as_ref().trim().to_ascii_uppercase();
        if normalized.len() != 3 || !normalized.chars().all(|char| char.is_ascii_uppercase()) {
            return Err(InterpretationError::new(format!(
                "invalid currency code `{}`",
                value.as_ref()
            )));
        }
        Ok(Self(normalized))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for CurrencyCode {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct CalendarDate {
    pub year: i32,
    pub month: u8,
    pub day: u8,
}

impl CalendarDate {
    pub fn new(year: i32, month: u8, day: u8) -> Result<Self, InterpretationError> {
        if !(1..=12).contains(&month) {
            return Err(InterpretationError::new(format!(
                "invalid calendar month `{month}`"
            )));
        }
        let max_day = days_in_month(year, month);
        if day == 0 || day > max_day {
            return Err(InterpretationError::new(format!(
                "invalid calendar day `{day}` for {year:04}-{month:02}"
            )));
        }
        Ok(Self { year, month, day })
    }

    pub fn ordinal_days(&self) -> i64 {
        days_from_civil(self.year, self.month, self.day)
    }
}

impl fmt::Display for CalendarDate {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "{:04}-{:02}-{:02}",
            self.year, self.month, self.day
        )
    }
}

pub trait EvidenceValuePlugin: Send + Sync {
    fn id(&self) -> &'static str;

    fn interpret(
        &self,
        input: &EvidenceInterpretationInput,
    ) -> Result<EvidenceInterpretationOutput, InterpretationError>;
}

pub trait EvidenceDerivationPlugin: Send + Sync {
    fn id(&self) -> &'static str;

    fn derive(&self, request: &DerivationRequest) -> Result<DerivationResult, InterpretationError>;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OperandLabel {
    Include,
    Exclude,
    Context,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OperandRole {
    Addend,
    AverageMember,
    CountedItem,
    Minuend,
    Subtrahend,
    Candidate,
    Context,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DerivationOperation {
    Sum,
    Count,
    Average,
    Difference,
    MaxBy,
    List,
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DerivationOperand {
    pub ref_id: String,
    pub label: OperandLabel,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub role: Option<OperandRole>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub entity: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub value: Option<InterpretedValue>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub raw: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

impl DerivationOperand {
    pub fn included(ref_id: impl Into<String>, value: InterpretedValue) -> Self {
        Self {
            ref_id: ref_id.into(),
            label: OperandLabel::Include,
            role: None,
            entity: None,
            value: Some(value),
            raw: None,
            reason: None,
        }
    }

    pub fn with_role(mut self, role: OperandRole) -> Self {
        self.role = Some(role);
        self
    }

    pub fn with_entity(mut self, entity: impl Into<String>) -> Self {
        self.entity = Some(entity.into());
        self
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DerivationRequest {
    pub question: String,
    pub operation: DerivationOperation,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub unit: Option<String>,
    pub operands: Vec<DerivationOperand>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DerivationResult {
    pub plugin: String,
    pub operation: DerivationOperation,
    pub answer: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub value: Option<InterpretedValue>,
    pub included_refs: Vec<String>,
    pub excluded_refs: Vec<String>,
    pub diagnostics: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InterpretationError {
    message: String,
}

impl InterpretationError {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

impl fmt::Display for InterpretationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl Error for InterpretationError {}

fn days_in_month(year: i32, month: u8) -> u8 {
    match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if is_leap_year(year) => 29,
        2 => 28,
        _ => 0,
    }
}

fn is_leap_year(year: i32) -> bool {
    (year % 4 == 0 && year % 100 != 0) || year % 400 == 0
}

fn days_from_civil(year: i32, month: u8, day: u8) -> i64 {
    let adjusted_year = year - i32::from(month <= 2);
    let era = if adjusted_year >= 0 {
        adjusted_year
    } else {
        adjusted_year - 399
    } / 400;
    let year_of_era = adjusted_year - era * 400;
    let month_i32 = i32::from(month);
    let day_i32 = i32::from(day);
    let day_of_year =
        (153 * (month_i32 + if month_i32 > 2 { -3 } else { 9 }) + 2) / 5 + day_i32 - 1;
    let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;
    i64::from(era * 146_097 + day_of_era)
}

#[cfg(test)]
mod tests {
    use super::{
        CalendarDate, CurrencyCode, DerivationOperand, EvidenceSegmentKind, InterpretedValue,
        MathExpressionNotation, OperandLabel, OperandRole, SourceCodeSegmentKind,
    };

    #[test]
    fn currency_code_normalizes_to_uppercase_iso_like_code() {
        let code = CurrencyCode::new(" usd ").expect("currency code should be valid");

        assert_eq!(code.as_str(), "USD");
    }

    #[test]
    fn currency_code_rejects_non_iso_like_values() {
        let error = CurrencyCode::new("US").expect_err("currency code should be invalid");

        assert_eq!(error.to_string(), "invalid currency code `US`");
    }

    #[test]
    fn calendar_date_rejects_invalid_day_for_month() {
        let error = CalendarDate::new(2026, 2, 29).expect_err("date should be invalid");

        assert_eq!(error.to_string(), "invalid calendar day `29` for 2026-02");
    }

    #[test]
    fn derivation_operand_builder_preserves_explicit_role_and_entity() {
        let operand = DerivationOperand::included(
            "turn:42",
            InterpretedValue::number(3.0, Some("items".to_string())),
        )
        .with_role(OperandRole::CountedItem)
        .with_entity("payment-service");

        assert_eq!(operand.ref_id, "turn:42");
        assert_eq!(operand.label, OperandLabel::Include);
        assert_eq!(operand.role, Some(OperandRole::CountedItem));
        assert_eq!(operand.entity.as_deref(), Some("payment-service"));
    }

    #[test]
    fn source_code_value_preserves_language_and_segment_kind() {
        let value = InterpretedValue::source_code(
            Some("rust".to_string()),
            SourceCodeSegmentKind::FencedBlock,
            "fn main() {}",
        );

        assert_eq!(
            value,
            InterpretedValue::SourceCode {
                language: Some("rust".to_string()),
                segment_kind: SourceCodeSegmentKind::FencedBlock,
                text: "fn main() {}".to_string(),
            }
        );
    }

    #[test]
    fn math_expression_value_preserves_notation_and_expression() {
        let value =
            InterpretedValue::math_expression(MathExpressionNotation::InlineDollar, "2n + 1");

        assert_eq!(
            value,
            InterpretedValue::MathExpression {
                notation: MathExpressionNotation::InlineDollar,
                expression: "2n + 1".to_string(),
            }
        );
    }

    #[test]
    fn url_value_preserves_url_text() {
        let value = InterpretedValue::url("https://example.test/path");

        assert_eq!(
            value,
            InterpretedValue::Url {
                url: "https://example.test/path".to_string(),
            }
        );
    }

    #[test]
    fn evidence_segment_kind_models_deterministic_precedence() {
        assert!(
            EvidenceSegmentKind::SourceCode.precedence() < EvidenceSegmentKind::Math.precedence()
        );
        assert!(EvidenceSegmentKind::Math.precedence() < EvidenceSegmentKind::Url.precedence());
        assert!(EvidenceSegmentKind::Url.precedence() < EvidenceSegmentKind::Text.precedence());
        assert!(EvidenceSegmentKind::SourceCode.is_protected());
        assert!(EvidenceSegmentKind::Text.is_interpretable_text());
    }
}
