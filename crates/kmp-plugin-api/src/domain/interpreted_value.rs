use serde::{Deserialize, Serialize};

use super::calendar_date::CalendarDate;
use super::currency_code::CurrencyCode;
use super::math_expression_notation::MathExpressionNotation;
use super::source_code_segment_kind::SourceCodeSegmentKind;

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
