use std::collections::BTreeSet;

pub use kmp_plugin_api::{
    CalendarDate, CurrencyCode, DerivationOperand, DerivationOperation, DerivationRequest,
    DerivationResult, EvidenceDerivationPlugin, EvidenceFragment, EvidenceInterpretationInput,
    EvidenceInterpretationOutput, EvidenceSegmentKind, EvidenceValuePlugin, InterpretationError,
    InterpretedValue, InterpretedValueMention, MathExpressionNotation, OperandLabel, OperandRole,
    SourceCodeSegmentKind, TextSpan,
};

use crate::text_normalization::{DetectedTextKind, TextNormalizationPipeline};

#[derive(Debug, Clone, Copy, Default)]
pub struct MoneyValuePlugin;

impl EvidenceValuePlugin for MoneyValuePlugin {
    fn id(&self) -> &'static str {
        "money-value-v1"
    }

    fn interpret(
        &self,
        input: &EvidenceInterpretationInput,
    ) -> Result<EvidenceInterpretationOutput, InterpretationError> {
        let mut values = Vec::new();
        for fragment in &input.fragments {
            for text_slice in interpretable_text_slices(fragment) {
                values.extend(offset_mentions(
                    extract_symbol_money(self.id(), &text_slice.fragment)?,
                    text_slice.offset,
                ));
                values.extend(offset_mentions(
                    extract_code_money(self.id(), &text_slice.fragment)?,
                    text_slice.offset,
                ));
            }
        }

        Ok(EvidenceInterpretationOutput {
            plugin: self.id().to_string(),
            values,
            diagnostics: Vec::new(),
        })
    }
}

#[derive(Debug, Clone, Copy, Default)]
pub struct DateValuePlugin;

impl EvidenceValuePlugin for DateValuePlugin {
    fn id(&self) -> &'static str {
        "date-value-v1"
    }

    fn interpret(
        &self,
        input: &EvidenceInterpretationInput,
    ) -> Result<EvidenceInterpretationOutput, InterpretationError> {
        let mut values = Vec::new();
        for fragment in &input.fragments {
            for text_slice in interpretable_text_slices(fragment) {
                values.extend(offset_mentions(
                    extract_iso_dates(self.id(), &text_slice.fragment)?,
                    text_slice.offset,
                ));
                values.extend(offset_mentions(
                    extract_named_dates(self.id(), &text_slice.fragment)?,
                    text_slice.offset,
                ));
            }
        }

        Ok(EvidenceInterpretationOutput {
            plugin: self.id().to_string(),
            values,
            diagnostics: Vec::new(),
        })
    }
}

#[derive(Debug, Clone, Copy, Default)]
pub struct ValueOperationPlugin;

impl ValueOperationPlugin {
    pub fn id(&self) -> &'static str {
        "value-operation-v1"
    }

    pub fn derive(
        &self,
        request: &DerivationRequest,
    ) -> Result<DerivationResult, InterpretationError> {
        let included = included_operands(&request.operands);
        let excluded_refs = refs_for_label(&request.operands, OperandLabel::Exclude);
        let mut diagnostics = Vec::new();

        match request.operation {
            DerivationOperation::Sum => {
                let values = numeric_operands(&included, "sum")?;
                let derived = sum_numeric(&values)?;
                Ok(result(
                    self.id(),
                    request.operation,
                    Some(format_interpreted_value(&derived.value)),
                    Some(derived.value),
                    refs_for_operands(&included),
                    excluded_refs,
                    diagnostics,
                ))
            }
            DerivationOperation::Count => {
                let counted = distinct_counted_operands(&included);
                let value =
                    InterpretedValue::number(counted.len() as f64, Some("items".to_string()));
                Ok(result(
                    self.id(),
                    request.operation,
                    Some(counted.len().to_string()),
                    Some(value),
                    counted
                        .iter()
                        .map(|operand| operand.ref_id.clone())
                        .collect(),
                    excluded_refs,
                    diagnostics,
                ))
            }
            DerivationOperation::Average => {
                let values = numeric_operands(&included, "average")?;
                let derived = average_numeric(&values)?;
                Ok(result(
                    self.id(),
                    request.operation,
                    Some(format_interpreted_value(&derived.value)),
                    Some(derived.value),
                    refs_for_operands(&included),
                    excluded_refs,
                    diagnostics,
                ))
            }
            DerivationOperation::Difference => {
                let values = difference_operands(&included)?;
                let derived = subtract_numeric(&values)?;
                Ok(result(
                    self.id(),
                    request.operation,
                    Some(format_interpreted_value(&derived.value)),
                    Some(derived.value),
                    refs_for_operands(&included),
                    excluded_refs,
                    diagnostics,
                ))
            }
            DerivationOperation::MaxBy => {
                let values = numeric_operands(&included, "max_by")?;
                let winner = values
                    .iter()
                    .max_by(|left, right| left.number.total_cmp(&right.number))
                    .ok_or_else(|| InterpretationError::new("max_by requires operands"))?;
                let entity = winner
                    .operand
                    .entity
                    .as_deref()
                    .or(winner.operand.raw.as_deref())
                    .unwrap_or(winner.operand.ref_id.as_str());
                let answer = format!("{entity}: {}", format_interpreted_value(&winner.value));
                Ok(result(
                    self.id(),
                    request.operation,
                    Some(answer),
                    Some(winner.value.clone()),
                    vec![winner.operand.ref_id.clone()],
                    excluded_refs,
                    diagnostics,
                ))
            }
            DerivationOperation::List => {
                let items = included
                    .iter()
                    .map(|operand| {
                        operand
                            .entity
                            .as_deref()
                            .or(operand.raw.as_deref())
                            .map(ToString::to_string)
                            .or_else(|| operand.value.as_ref().map(format_interpreted_value))
                            .unwrap_or_else(|| operand.ref_id.clone())
                    })
                    .collect::<Vec<_>>();
                if items.is_empty() {
                    return Err(InterpretationError::new(
                        "list derivation requires included operands",
                    ));
                }
                Ok(result(
                    self.id(),
                    request.operation,
                    Some(items.join(", ")),
                    None,
                    refs_for_operands(&included),
                    excluded_refs,
                    diagnostics,
                ))
            }
            DerivationOperation::Unknown => {
                diagnostics.push("operation is unknown; plugin abstained".to_string());
                Ok(result(
                    self.id(),
                    request.operation,
                    None,
                    None,
                    refs_for_operands(&included),
                    excluded_refs,
                    diagnostics,
                ))
            }
        }
    }
}

impl EvidenceDerivationPlugin for ValueOperationPlugin {
    fn id(&self) -> &'static str {
        ValueOperationPlugin::id(self)
    }

    fn derive(&self, request: &DerivationRequest) -> Result<DerivationResult, InterpretationError> {
        ValueOperationPlugin::derive(self, request)
    }
}

#[derive(Debug, Clone, Copy, Default)]
pub struct CurrencyDerivationPlugin;

impl CurrencyDerivationPlugin {
    pub fn id(&self) -> &'static str {
        "currency-derivation-v1"
    }

    pub fn interpret(
        &self,
        input: &EvidenceInterpretationInput,
    ) -> Result<EvidenceInterpretationOutput, InterpretationError> {
        let mut output = MoneyValuePlugin.interpret(input)?;
        output.plugin = self.id().to_string();
        for mention in &mut output.values {
            mention.plugin = self.id().to_string();
        }
        Ok(output)
    }

    pub fn derive(
        &self,
        request: &DerivationRequest,
    ) -> Result<DerivationResult, InterpretationError> {
        ensure_supported_operation(
            request.operation,
            &[
                DerivationOperation::Sum,
                DerivationOperation::Average,
                DerivationOperation::Difference,
                DerivationOperation::MaxBy,
                DerivationOperation::List,
                DerivationOperation::Unknown,
            ],
            self.id(),
        )?;
        ensure_operand_family(request, self.id(), value_is_money)?;
        let mut result = ValueOperationPlugin.derive(request)?;
        result.plugin = self.id().to_string();
        Ok(result)
    }
}

impl EvidenceValuePlugin for CurrencyDerivationPlugin {
    fn id(&self) -> &'static str {
        CurrencyDerivationPlugin::id(self)
    }

    fn interpret(
        &self,
        input: &EvidenceInterpretationInput,
    ) -> Result<EvidenceInterpretationOutput, InterpretationError> {
        CurrencyDerivationPlugin::interpret(self, input)
    }
}

impl EvidenceDerivationPlugin for CurrencyDerivationPlugin {
    fn id(&self) -> &'static str {
        CurrencyDerivationPlugin::id(self)
    }

    fn derive(&self, request: &DerivationRequest) -> Result<DerivationResult, InterpretationError> {
        CurrencyDerivationPlugin::derive(self, request)
    }
}

#[derive(Debug, Clone, Copy, Default)]
pub struct DateDerivationPlugin;

impl DateDerivationPlugin {
    pub fn id(&self) -> &'static str {
        "date-derivation-v1"
    }

    pub fn interpret(
        &self,
        input: &EvidenceInterpretationInput,
    ) -> Result<EvidenceInterpretationOutput, InterpretationError> {
        let mut output = DateValuePlugin.interpret(input)?;
        output.plugin = self.id().to_string();
        for mention in &mut output.values {
            mention.plugin = self.id().to_string();
        }
        Ok(output)
    }

    pub fn derive(
        &self,
        request: &DerivationRequest,
    ) -> Result<DerivationResult, InterpretationError> {
        ensure_supported_operation(
            request.operation,
            &[
                DerivationOperation::Difference,
                DerivationOperation::MaxBy,
                DerivationOperation::List,
                DerivationOperation::Unknown,
            ],
            self.id(),
        )?;
        ensure_operand_family(request, self.id(), value_is_date)?;
        let mut result = ValueOperationPlugin.derive(request)?;
        result.plugin = self.id().to_string();
        Ok(result)
    }
}

impl EvidenceValuePlugin for DateDerivationPlugin {
    fn id(&self) -> &'static str {
        DateDerivationPlugin::id(self)
    }

    fn interpret(
        &self,
        input: &EvidenceInterpretationInput,
    ) -> Result<EvidenceInterpretationOutput, InterpretationError> {
        DateDerivationPlugin::interpret(self, input)
    }
}

impl EvidenceDerivationPlugin for DateDerivationPlugin {
    fn id(&self) -> &'static str {
        DateDerivationPlugin::id(self)
    }

    fn derive(&self, request: &DerivationRequest) -> Result<DerivationResult, InterpretationError> {
        DateDerivationPlugin::derive(self, request)
    }
}

#[derive(Debug, Clone)]
struct NumericOperand<'a> {
    operand: &'a DerivationOperand,
    number: f64,
    value: InterpretedValue,
    kind: NumericKind,
}

#[derive(Debug, Clone)]
struct DerivedNumericValue {
    value: InterpretedValue,
}

#[derive(Debug, Clone)]
struct TextEvidenceSlice {
    fragment: EvidenceFragment,
    offset: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum NumericKind {
    Money(CurrencyCode),
    DateOrdinal,
    Number(Option<String>),
}

fn result(
    plugin: &str,
    operation: DerivationOperation,
    answer: Option<String>,
    value: Option<InterpretedValue>,
    included_refs: Vec<String>,
    excluded_refs: Vec<String>,
    diagnostics: Vec<String>,
) -> DerivationResult {
    DerivationResult {
        plugin: plugin.to_string(),
        operation,
        answer,
        value,
        included_refs,
        excluded_refs,
        diagnostics,
    }
}

fn ensure_supported_operation(
    operation: DerivationOperation,
    supported: &[DerivationOperation],
    plugin: &str,
) -> Result<(), InterpretationError> {
    if supported.contains(&operation) {
        return Ok(());
    }
    Err(InterpretationError::new(format!(
        "{plugin} does not support operation {operation:?}"
    )))
}

fn ensure_operand_family(
    request: &DerivationRequest,
    plugin: &str,
    expected: fn(&InterpretedValue) -> bool,
) -> Result<(), InterpretationError> {
    for operand in request
        .operands
        .iter()
        .filter(|operand| operand.label == OperandLabel::Include)
    {
        let Some(value) = operand.value.as_ref() else {
            continue;
        };
        if !expected(value) {
            return Err(InterpretationError::new(format!(
                "{plugin} received incompatible operand {}",
                operand.ref_id
            )));
        }
    }
    Ok(())
}

fn value_is_money(value: &InterpretedValue) -> bool {
    matches!(value, InterpretedValue::Money { .. })
}

fn value_is_date(value: &InterpretedValue) -> bool {
    matches!(value, InterpretedValue::Date { .. })
}

fn interpretable_text_slices(fragment: &EvidenceFragment) -> Vec<TextEvidenceSlice> {
    let normalized = TextNormalizationPipeline.normalize(&fragment.text);
    normalized
        .spans
        .into_iter()
        .filter(|span| span.kind == DetectedTextKind::Text)
        .filter(|span| !span.raw.trim().is_empty())
        .map(|span| TextEvidenceSlice {
            fragment: EvidenceFragment {
                ref_id: fragment.ref_id.clone(),
                text: span.raw,
                source: fragment.source.clone(),
            },
            offset: span.span.start,
        })
        .collect()
}

fn offset_mentions(
    mentions: Vec<InterpretedValueMention>,
    offset: usize,
) -> Vec<InterpretedValueMention> {
    mentions
        .into_iter()
        .map(|mut mention| {
            mention.span.start += offset;
            mention.span.end += offset;
            mention
        })
        .collect()
}

fn included_operands(operands: &[DerivationOperand]) -> Vec<&DerivationOperand> {
    operands
        .iter()
        .filter(|operand| operand.label == OperandLabel::Include)
        .collect()
}

fn refs_for_operands(operands: &[&DerivationOperand]) -> Vec<String> {
    operands
        .iter()
        .map(|operand| operand.ref_id.clone())
        .collect()
}

fn refs_for_label(operands: &[DerivationOperand], label: OperandLabel) -> Vec<String> {
    operands
        .iter()
        .filter(|operand| operand.label == label)
        .map(|operand| operand.ref_id.clone())
        .collect()
}

fn distinct_counted_operands<'a>(operands: &[&'a DerivationOperand]) -> Vec<&'a DerivationOperand> {
    let mut seen = BTreeSet::new();
    let mut counted = Vec::new();
    for operand in operands {
        let key = operand
            .entity
            .as_deref()
            .map(normalize_entity_key)
            .filter(|key| !key.is_empty())
            .unwrap_or_else(|| operand.ref_id.clone());
        if seen.insert(key) {
            counted.push(*operand);
        }
    }
    counted
}

fn numeric_operands<'a>(
    operands: &[&'a DerivationOperand],
    context: &str,
) -> Result<Vec<NumericOperand<'a>>, InterpretationError> {
    let mut values = Vec::new();
    for operand in operands {
        values.push(numeric_operand(operand, context)?);
    }
    if values.is_empty() {
        return Err(InterpretationError::new(format!(
            "{context} derivation requires at least one included value operand"
        )));
    }
    Ok(values)
}

fn numeric_operand<'a>(
    operand: &'a DerivationOperand,
    context: &str,
) -> Result<NumericOperand<'a>, InterpretationError> {
    let value = operand.value.as_ref().ok_or_else(|| {
        InterpretationError::new(format!(
            "{context} operand {} has no interpreted value",
            operand.ref_id
        ))
    })?;

    match value {
        InterpretedValue::Money {
            currency,
            amount,
            amount_minor: _,
        } => Ok(NumericOperand {
            operand,
            number: *amount,
            value: value.clone(),
            kind: NumericKind::Money(currency.clone()),
        }),
        InterpretedValue::Date { date } => Ok(NumericOperand {
            operand,
            number: date.ordinal_days() as f64,
            value: value.clone(),
            kind: NumericKind::DateOrdinal,
        }),
        InterpretedValue::Number { value, unit } => Ok(NumericOperand {
            operand,
            number: *value,
            value: InterpretedValue::Number {
                value: *value,
                unit: unit.clone(),
            },
            kind: NumericKind::Number(unit.clone()),
        }),
        InterpretedValue::MathExpression { .. } => Err(InterpretationError::new(format!(
            "{context} operand {} is a math expression, not a numeric value",
            operand.ref_id
        ))),
        InterpretedValue::SourceCode { .. } => Err(InterpretationError::new(format!(
            "{context} operand {} is source code, not a numeric value",
            operand.ref_id
        ))),
        InterpretedValue::Url { .. } => Err(InterpretationError::new(format!(
            "{context} operand {} is a URL, not a numeric value",
            operand.ref_id
        ))),
    }
}

fn sum_numeric(values: &[NumericOperand<'_>]) -> Result<DerivedNumericValue, InterpretationError> {
    let first = values
        .first()
        .ok_or_else(|| InterpretationError::new("sum requires operands"))?;
    ensure_compatible_numeric_kinds(first.kind.clone(), values, "sum")?;
    if first.kind == NumericKind::DateOrdinal {
        return Err(InterpretationError::new(
            "sum does not support date operands",
        ));
    }
    let total = values.iter().map(|value| value.number).sum::<f64>();
    Ok(owned_numeric_result(total, &first.kind, "sum"))
}

fn average_numeric(
    values: &[NumericOperand<'_>],
) -> Result<DerivedNumericValue, InterpretationError> {
    let first = values
        .first()
        .ok_or_else(|| InterpretationError::new("average requires operands"))?;
    ensure_compatible_numeric_kinds(first.kind.clone(), values, "average")?;
    if first.kind == NumericKind::DateOrdinal {
        return Err(InterpretationError::new(
            "average does not support date operands",
        ));
    }
    let total = values.iter().map(|value| value.number).sum::<f64>();
    Ok(owned_numeric_result(
        total / values.len() as f64,
        &first.kind,
        "average",
    ))
}

fn difference_operands<'a>(
    operands: &[&'a DerivationOperand],
) -> Result<Vec<NumericOperand<'a>>, InterpretationError> {
    let minuends = operands
        .iter()
        .filter(|operand| operand.role == Some(OperandRole::Minuend))
        .copied()
        .collect::<Vec<_>>();
    let subtrahends = operands
        .iter()
        .filter(|operand| operand.role == Some(OperandRole::Subtrahend))
        .copied()
        .collect::<Vec<_>>();

    if minuends.len() == 1 && subtrahends.len() == 1 {
        return Ok(vec![
            numeric_operand(minuends[0], "difference minuend")?,
            numeric_operand(subtrahends[0], "difference subtrahend")?,
        ]);
    }

    if operands.len() == 2 {
        return Ok(vec![
            numeric_operand(operands[0], "difference first operand")?,
            numeric_operand(operands[1], "difference second operand")?,
        ]);
    }

    Err(InterpretationError::new(
        "difference requires one minuend and one subtrahend, or exactly two included operands",
    ))
}

fn subtract_numeric(
    values: &[NumericOperand<'_>],
) -> Result<DerivedNumericValue, InterpretationError> {
    if values.len() != 2 {
        return Err(InterpretationError::new(
            "difference requires exactly two operands",
        ));
    }
    let first = &values[0];
    let second = &values[1];
    ensure_compatible_numeric_kinds(first.kind.clone(), values, "difference")?;
    let delta = first.number - second.number;
    if first.kind == NumericKind::DateOrdinal {
        return Ok(owned_numeric_result(
            delta,
            &NumericKind::Number(Some("days".to_string())),
            "difference",
        ));
    }
    Ok(owned_numeric_result(delta, &first.kind, "difference"))
}

fn ensure_compatible_numeric_kinds(
    expected: NumericKind,
    values: &[NumericOperand<'_>],
    context: &str,
) -> Result<(), InterpretationError> {
    for value in values {
        if value.kind != expected {
            return Err(InterpretationError::new(format!(
                "{context} cannot mix incompatible operand kinds"
            )));
        }
    }
    Ok(())
}

fn owned_numeric_result(value: f64, kind: &NumericKind, _context: &str) -> DerivedNumericValue {
    let interpreted = match kind {
        NumericKind::Money(currency) => InterpretedValue::Money {
            currency: currency.clone(),
            amount_minor: (value * 100.0).round() as i64,
            amount: round_to_cents(value),
        },
        NumericKind::DateOrdinal => InterpretedValue::Number {
            value,
            unit: Some("days".to_string()),
        },
        NumericKind::Number(unit) => InterpretedValue::Number {
            value,
            unit: unit.clone(),
        },
    };

    DerivedNumericValue { value: interpreted }
}

fn format_interpreted_value(value: &InterpretedValue) -> String {
    match value {
        InterpretedValue::Money {
            currency, amount, ..
        } => format!("{currency} {}", format_decimal(*amount)),
        InterpretedValue::Date { date } => date.to_string(),
        InterpretedValue::Number { value, unit } => unit
            .as_deref()
            .map(|unit| format!("{} {unit}", format_decimal(*value)))
            .unwrap_or_else(|| format_decimal(*value)),
        InterpretedValue::MathExpression {
            notation,
            expression,
        } => match notation {
            MathExpressionNotation::InlineDollar => format!("${expression}$"),
            MathExpressionNotation::DisplayDollar => format!("$${expression}$$"),
            MathExpressionNotation::InlineParen => format!("\\({expression}\\)"),
            MathExpressionNotation::DisplayBracket => format!("\\[{expression}\\]"),
        },
        InterpretedValue::SourceCode {
            language,
            segment_kind,
            text: _,
        } => language
            .as_deref()
            .map(|language| format!("{language} {segment_kind:?} source code"))
            .unwrap_or_else(|| format!("{segment_kind:?} source code")),
        InterpretedValue::Url { url } => url.clone(),
    }
}

fn format_decimal(value: f64) -> String {
    if (value.round() - value).abs() < 0.000_001 {
        format!("{}", value.round() as i64)
    } else {
        format!("{value:.2}")
    }
}

fn normalize_entity_key(value: &str) -> String {
    value
        .trim()
        .to_ascii_lowercase()
        .chars()
        .filter(|char| char.is_ascii_alphanumeric() || char.is_ascii_whitespace())
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

fn extract_symbol_money(
    plugin: &str,
    fragment: &EvidenceFragment,
) -> Result<Vec<InterpretedValueMention>, InterpretationError> {
    let mut values = Vec::new();
    let mut iter = fragment.text.char_indices().peekable();
    while let Some((start, char)) = iter.next() {
        let Some(currency) = currency_from_symbol(char) else {
            continue;
        };
        while let Some((_, whitespace)) = iter.peek() {
            if whitespace.is_whitespace() {
                iter.next();
            } else {
                break;
            }
        }
        let Some((amount_start, next_char)) = iter.peek().copied() else {
            continue;
        };
        if !next_char.is_ascii_digit() {
            continue;
        }
        let amount_end = read_amount_end(&fragment.text, amount_start);
        if !symbol_money_context_allowed(&fragment.text, start, amount_end) {
            continue;
        }
        let raw = fragment.text[start..amount_end].to_string();
        let amount_text = &fragment.text[amount_start..amount_end];
        if let Some((amount, amount_minor)) = parse_amount(amount_text) {
            values.push(value_mention(
                plugin,
                fragment,
                raw,
                TextSpan {
                    start,
                    end: amount_end,
                },
                money_value(currency, amount, amount_minor)?,
            ));
        }
    }
    Ok(values)
}

fn symbol_money_context_allowed(text: &str, start: usize, amount_end: usize) -> bool {
    let previous = previous_non_whitespace(text, start);
    let next = next_non_whitespace(text, amount_end);
    let immediate_next = text[amount_end..].chars().next();

    if matches!(previous, Some('\\' | '_' | '^')) {
        return false;
    }
    if immediate_next.is_some_and(|char| char.is_ascii_alphabetic()) {
        return false;
    }
    if matches!(next, Some('$' | '\\' | '_' | '^' | '}')) {
        return false;
    }
    true
}

fn extract_code_money(
    plugin: &str,
    fragment: &EvidenceFragment,
) -> Result<Vec<InterpretedValueMention>, InterpretationError> {
    let words = word_spans(&fragment.text);
    let mut values = Vec::new();

    for window in words.windows(2) {
        let first = &window[0];
        let second = &window[1];
        if let Some(currency) = currency_from_word(first.text)
            && let Some((amount, amount_minor)) = parse_amount(second.text)
        {
            values.push(value_mention(
                plugin,
                fragment,
                fragment.text[first.start..second.end].to_string(),
                TextSpan {
                    start: first.start,
                    end: second.end,
                },
                money_value(currency, amount, amount_minor)?,
            ));
        }
        if let Some(currency) = currency_from_word(second.text)
            && let Some((amount, amount_minor)) = parse_amount(first.text)
        {
            values.push(value_mention(
                plugin,
                fragment,
                fragment.text[first.start..second.end].to_string(),
                TextSpan {
                    start: first.start,
                    end: second.end,
                },
                money_value(currency, amount, amount_minor)?,
            ));
        }
    }

    Ok(values)
}

fn extract_iso_dates(
    plugin: &str,
    fragment: &EvidenceFragment,
) -> Result<Vec<InterpretedValueMention>, InterpretationError> {
    let mut values = Vec::new();
    for word in word_spans(&fragment.text) {
        let cleaned = trim_date_token(word.text);
        if let Some(date) = parse_iso_date(cleaned)? {
            values.push(value_mention(
                plugin,
                fragment,
                cleaned.to_string(),
                TextSpan {
                    start: word.start,
                    end: word.end,
                },
                InterpretedValue::Date { date },
            ));
        }
    }
    Ok(values)
}

fn extract_named_dates(
    plugin: &str,
    fragment: &EvidenceFragment,
) -> Result<Vec<InterpretedValueMention>, InterpretationError> {
    let words = word_spans(&fragment.text);
    let mut values = Vec::new();

    for window in words.windows(3) {
        if let Some(month) = month_number(window[0].text) {
            let day = parse_u8_token(window[1].text);
            let year = parse_i32_token(window[2].text);
            if let (Some(day), Some(year)) = (day, year) {
                let date = CalendarDate::new(year, month, day)?;
                values.push(value_mention(
                    plugin,
                    fragment,
                    fragment.text[window[0].start..window[2].end].to_string(),
                    TextSpan {
                        start: window[0].start,
                        end: window[2].end,
                    },
                    InterpretedValue::Date { date },
                ));
            }
        }
        if let Some(month) = month_number(window[1].text) {
            let day = parse_u8_token(window[0].text);
            let year = parse_i32_token(window[2].text);
            if let (Some(day), Some(year)) = (day, year) {
                let date = CalendarDate::new(year, month, day)?;
                values.push(value_mention(
                    plugin,
                    fragment,
                    fragment.text[window[0].start..window[2].end].to_string(),
                    TextSpan {
                        start: window[0].start,
                        end: window[2].end,
                    },
                    InterpretedValue::Date { date },
                ));
            }
        }
    }

    Ok(values)
}

fn value_mention(
    plugin: &str,
    fragment: &EvidenceFragment,
    raw: String,
    span: TextSpan,
    value: InterpretedValue,
) -> InterpretedValueMention {
    InterpretedValueMention {
        plugin: plugin.to_string(),
        ref_id: fragment.ref_id.clone(),
        raw,
        span,
        value,
        confidence: 1.0,
    }
}

fn money_value(
    currency: &str,
    amount: f64,
    amount_minor: i64,
) -> Result<InterpretedValue, InterpretationError> {
    Ok(InterpretedValue::Money {
        currency: CurrencyCode::new(currency)?,
        amount_minor,
        amount,
    })
}

fn currency_from_symbol(value: char) -> Option<&'static str> {
    match value {
        '$' => Some("USD"),
        '\u{20ac}' => Some("EUR"),
        '\u{00a3}' => Some("GBP"),
        '\u{00a5}' => Some("JPY"),
        _ => None,
    }
}

fn currency_from_word(value: &str) -> Option<&'static str> {
    match trim_word_token(value).to_ascii_lowercase().as_str() {
        "usd" | "dollar" | "dollars" => Some("USD"),
        "eur" | "euro" | "euros" => Some("EUR"),
        "gbp" | "pound" | "pounds" => Some("GBP"),
        "jpy" | "yen" => Some("JPY"),
        _ => None,
    }
}

fn read_amount_end(text: &str, start: usize) -> usize {
    let mut end = start;
    for (index, char) in text[start..].char_indices() {
        let decimal_separator = char == '.'
            && text[start + index + char.len_utf8()..]
                .chars()
                .next()
                .is_some_and(|next| next.is_ascii_digit());
        if char.is_ascii_digit() || char == ',' || decimal_separator {
            end = start + index + char.len_utf8();
        } else {
            break;
        }
    }
    end
}

fn previous_non_whitespace(text: &str, boundary: usize) -> Option<char> {
    text[..boundary]
        .chars()
        .rev()
        .find(|char| !char.is_whitespace())
}

fn next_non_whitespace(text: &str, boundary: usize) -> Option<char> {
    text[boundary..].chars().find(|char| !char.is_whitespace())
}

fn parse_amount(value: &str) -> Option<(f64, i64)> {
    let cleaned = trim_token(value);
    if cleaned.is_empty() || !cleaned.chars().any(|char| char.is_ascii_digit()) {
        return None;
    }
    let normalized = normalize_decimal(cleaned);
    let amount = normalized.parse::<f64>().ok()?;
    Some((round_to_cents(amount), (amount * 100.0).round() as i64))
}

fn normalize_decimal(value: &str) -> String {
    if value.contains(',') && !value.contains('.') {
        let decimal_digits = value.rsplit(',').next().map(str::len).unwrap_or_default();
        if decimal_digits <= 2 {
            return value.replace(',', ".");
        }
    }
    value.replace(',', "")
}

fn round_to_cents(value: f64) -> f64 {
    (value * 100.0).round() / 100.0
}

fn parse_iso_date(value: &str) -> Result<Option<CalendarDate>, InterpretationError> {
    let separator = if value.contains('-') {
        '-'
    } else if value.contains('/') {
        '/'
    } else {
        return Ok(None);
    };
    let parts = value.split(separator).collect::<Vec<_>>();
    if parts.len() != 3 {
        return Ok(None);
    }
    if parts[0].len() == 4 {
        let Some(year) = parse_i32_token(parts[0]) else {
            return Ok(None);
        };
        let Some(month) = parse_u8_token(parts[1]) else {
            return Ok(None);
        };
        let Some(day) = parse_u8_token(parts[2]) else {
            return Ok(None);
        };
        return CalendarDate::new(year, month, day).map(Some);
    }
    if parts[2].len() == 4 {
        let Some(month) = parse_u8_token(parts[0]) else {
            return Ok(None);
        };
        let Some(day) = parse_u8_token(parts[1]) else {
            return Ok(None);
        };
        let Some(year) = parse_i32_token(parts[2]) else {
            return Ok(None);
        };
        return CalendarDate::new(year, month, day).map(Some);
    }
    Ok(None)
}

fn parse_i32_token(value: &str) -> Option<i32> {
    trim_word_token(value).parse::<i32>().ok()
}

fn parse_u8_token(value: &str) -> Option<u8> {
    trim_word_token(value).parse::<u8>().ok()
}

fn trim_token(value: &str) -> &str {
    value.trim_matches(|char: char| {
        char.is_ascii_punctuation() && char != '.' && char != ',' && char != '-' && char != '/'
    })
}

fn trim_date_token(value: &str) -> &str {
    value.trim_matches(|char: char| char.is_ascii_punctuation() && char != '-' && char != '/')
}

fn trim_word_token(value: &str) -> &str {
    value.trim_matches(|char: char| char.is_ascii_punctuation())
}

fn month_number(value: &str) -> Option<u8> {
    match trim_word_token(value).to_ascii_lowercase().as_str() {
        "jan" | "january" => Some(1),
        "feb" | "february" => Some(2),
        "mar" | "march" => Some(3),
        "apr" | "april" => Some(4),
        "may" => Some(5),
        "jun" | "june" => Some(6),
        "jul" | "july" => Some(7),
        "aug" | "august" => Some(8),
        "sep" | "sept" | "september" => Some(9),
        "oct" | "october" => Some(10),
        "nov" | "november" => Some(11),
        "dec" | "december" => Some(12),
        _ => None,
    }
}

#[derive(Debug, Clone, Copy)]
struct WordSpan<'a> {
    text: &'a str,
    start: usize,
    end: usize,
}

fn word_spans(text: &str) -> Vec<WordSpan<'_>> {
    let mut words = Vec::new();
    let mut start = None;
    for (index, char) in text.char_indices() {
        if char.is_whitespace() {
            if let Some(word_start) = start.take() {
                words.push(WordSpan {
                    text: &text[word_start..index],
                    start: word_start,
                    end: index,
                });
            }
        } else if start.is_none() {
            start = Some(index);
        }
    }
    if let Some(word_start) = start {
        words.push(WordSpan {
            text: &text[word_start..],
            start: word_start,
            end: text.len(),
        });
    }
    words
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn money_plugin_extracts_symbols_and_codes() {
        let plugin = MoneyValuePlugin;
        let input = EvidenceInterpretationInput::new(vec![EvidenceFragment::new(
            "turn:1",
            "Paid $1,200.50 for hardware and another 30 EUR for shipping.",
        )]);

        let output = plugin
            .interpret(&input)
            .expect("money extraction should succeed");

        assert_eq!(output.values.len(), 2);
        assert_eq!(
            output.values[0].value,
            money_value("USD", 1200.50, 120_050).expect("valid USD value")
        );
        assert_eq!(
            output.values[1].value,
            money_value("EUR", 30.0, 3_000).expect("valid EUR value")
        );
    }

    #[test]
    fn money_plugin_rejects_latex_math_delimiters() {
        let plugin = MoneyValuePlugin;
        let input = EvidenceInterpretationInput::new(vec![EvidenceFragment::new(
            "turn:latex",
            "The formula $2$ and the term $2\\pi$ and $2n$ are not money, but budget is $70.",
        )]);

        let output = plugin
            .interpret(&input)
            .expect("money extraction should succeed");

        assert_eq!(output.values.len(), 1);
        assert_eq!(output.values[0].raw, "$70");
        assert_eq!(
            output.values[0].value,
            money_value("USD", 70.0, 7_000).expect("valid USD value")
        );
    }

    #[test]
    fn date_plugin_extracts_iso_and_named_dates() {
        let plugin = DateValuePlugin;
        let input = EvidenceInterpretationInput::new(vec![EvidenceFragment::new(
            "turn:2",
            "The outage started on 2026-05-01 and closed May 6, 2026.",
        )]);

        let output = plugin
            .interpret(&input)
            .expect("date extraction should succeed");

        let dates = output
            .values
            .iter()
            .map(|mention| mention.value.clone())
            .collect::<Vec<_>>();
        assert!(dates.contains(&InterpretedValue::Date {
            date: CalendarDate::new(2026, 5, 1).expect("valid date")
        }));
        assert!(dates.contains(&InterpretedValue::Date {
            date: CalendarDate::new(2026, 5, 6).expect("valid date")
        }));
    }

    #[test]
    fn money_and_date_plugins_ignore_protected_code_and_url_segments() {
        let input = EvidenceInterpretationInput::new(vec![EvidenceFragment::new(
            "turn:protected",
            "Ignore code `$90` and https://example.test/2026-05-06; use budget $70 on 2026-05-07.",
        )]);

        let money = MoneyValuePlugin
            .interpret(&input)
            .expect("money extraction should succeed");
        let dates = DateValuePlugin
            .interpret(&input)
            .expect("date extraction should succeed");

        assert_eq!(money.values.len(), 1);
        assert_eq!(money.values[0].raw, "$70");
        assert_eq!(money.values[0].span, TextSpan { start: 66, end: 69 });
        assert_eq!(dates.values.len(), 1);
        assert_eq!(dates.values[0].raw, "2026-05-07");
    }

    #[test]
    fn value_operation_sums_compatible_money() {
        let plugin = ValueOperationPlugin;
        let request = DerivationRequest {
            question: "How much was spent?".to_string(),
            operation: DerivationOperation::Sum,
            unit: None,
            operands: vec![
                DerivationOperand::included(
                    "a",
                    money_value("USD", 120.0, 12_000).expect("valid USD"),
                ),
                DerivationOperand::included(
                    "b",
                    money_value("USD", 65.0, 6_500).expect("valid USD"),
                ),
            ],
        };

        let result = plugin
            .derive(&request)
            .expect("money sum should derive deterministically");

        assert_eq!(result.answer.as_deref(), Some("USD 185"));
        assert_eq!(result.included_refs, vec!["a", "b"]);
    }

    #[test]
    fn value_operation_rejects_mixed_currencies() {
        let plugin = ValueOperationPlugin;
        let request = DerivationRequest {
            question: "How much was spent?".to_string(),
            operation: DerivationOperation::Sum,
            unit: None,
            operands: vec![
                DerivationOperand::included(
                    "a",
                    money_value("USD", 120.0, 12_000).expect("valid USD"),
                ),
                DerivationOperand::included(
                    "b",
                    money_value("EUR", 65.0, 6_500).expect("valid EUR"),
                ),
            ],
        };

        let error = plugin
            .derive(&request)
            .expect_err("mixed currencies must fail fast");

        assert!(error.to_string().contains("cannot mix incompatible"));
    }

    #[test]
    fn value_operation_computes_date_difference_in_days() {
        let plugin = ValueOperationPlugin;
        let request = DerivationRequest {
            question: "How long did it take?".to_string(),
            operation: DerivationOperation::Difference,
            unit: Some("days".to_string()),
            operands: vec![
                DerivationOperand::included(
                    "closed",
                    InterpretedValue::Date {
                        date: CalendarDate::new(2026, 5, 6).expect("valid date"),
                    },
                )
                .with_role(OperandRole::Minuend),
                DerivationOperand::included(
                    "started",
                    InterpretedValue::Date {
                        date: CalendarDate::new(2026, 5, 1).expect("valid date"),
                    },
                )
                .with_role(OperandRole::Subtrahend),
            ],
        };

        let result = plugin
            .derive(&request)
            .expect("date difference should derive");

        assert_eq!(result.answer.as_deref(), Some("5 days"));
    }

    #[test]
    fn value_operation_counts_distinct_entities() {
        let plugin = ValueOperationPlugin;
        let request = DerivationRequest {
            question: "How many vendors?".to_string(),
            operation: DerivationOperation::Count,
            unit: None,
            operands: vec![
                DerivationOperand::included("a", InterpretedValue::number(1.0, None))
                    .with_entity("Acme Inc."),
                DerivationOperand::included("b", InterpretedValue::number(1.0, None))
                    .with_entity("acme inc"),
                DerivationOperand::included("c", InterpretedValue::number(1.0, None))
                    .with_entity("Northwind"),
            ],
        };

        let result = plugin.derive(&request).expect("count should derive");

        assert_eq!(result.answer.as_deref(), Some("2"));
        assert_eq!(result.included_refs, vec!["a", "c"]);
    }

    #[test]
    fn value_operation_selects_max_by_value() {
        let plugin = ValueOperationPlugin;
        let request = DerivationRequest {
            question: "Which option cost the most?".to_string(),
            operation: DerivationOperation::MaxBy,
            unit: None,
            operands: vec![
                DerivationOperand::included(
                    "small",
                    money_value("USD", 40.0, 4_000).expect("valid USD"),
                )
                .with_entity("small kit"),
                DerivationOperand::included(
                    "large",
                    money_value("USD", 90.0, 9_000).expect("valid USD"),
                )
                .with_entity("large kit"),
            ],
        };

        let result = plugin.derive(&request).expect("max_by should derive");

        assert_eq!(result.answer.as_deref(), Some("large kit: USD 90"));
        assert_eq!(result.included_refs, vec!["large"]);
    }

    #[test]
    fn currency_derivation_plugin_owns_currency_operations() {
        let plugin = CurrencyDerivationPlugin;
        let request = DerivationRequest {
            question: "How much was spent?".to_string(),
            operation: DerivationOperation::Sum,
            unit: None,
            operands: vec![
                DerivationOperand::included(
                    "a",
                    money_value("USD", 12.25, 1_225).expect("valid USD"),
                ),
                DerivationOperand::included("b", money_value("USD", 2.75, 275).expect("valid USD")),
            ],
        };

        let result = plugin
            .derive(&request)
            .expect("currency plugin should derive money sums");

        assert_eq!(result.plugin, "currency-derivation-v1");
        assert_eq!(result.answer.as_deref(), Some("USD 15"));
    }

    #[test]
    fn currency_derivation_plugin_rejects_non_currency_operands() {
        let plugin = CurrencyDerivationPlugin;
        let request = DerivationRequest {
            question: "How much was spent?".to_string(),
            operation: DerivationOperation::Sum,
            unit: None,
            operands: vec![DerivationOperand::included(
                "date",
                InterpretedValue::Date {
                    date: CalendarDate::new(2026, 5, 6).expect("valid date"),
                },
            )],
        };

        let error = plugin
            .derive(&request)
            .expect_err("currency plugin must reject date operands");

        assert!(error.to_string().contains("incompatible operand"));
    }

    #[test]
    fn date_derivation_plugin_owns_temporal_operations() {
        let plugin = DateDerivationPlugin;
        let request = DerivationRequest {
            question: "How many days elapsed?".to_string(),
            operation: DerivationOperation::Difference,
            unit: Some("days".to_string()),
            operands: vec![
                DerivationOperand::included(
                    "closed",
                    InterpretedValue::Date {
                        date: CalendarDate::new(2026, 5, 6).expect("valid date"),
                    },
                )
                .with_role(OperandRole::Minuend),
                DerivationOperand::included(
                    "started",
                    InterpretedValue::Date {
                        date: CalendarDate::new(2026, 5, 1).expect("valid date"),
                    },
                )
                .with_role(OperandRole::Subtrahend),
            ],
        };

        let result = plugin
            .derive(&request)
            .expect("date plugin should derive date differences");

        assert_eq!(result.plugin, "date-derivation-v1");
        assert_eq!(result.answer.as_deref(), Some("5 days"));
    }

    #[test]
    fn date_derivation_plugin_rejects_money_operands() {
        let plugin = DateDerivationPlugin;
        let request = DerivationRequest {
            question: "How many days elapsed?".to_string(),
            operation: DerivationOperation::Difference,
            unit: Some("days".to_string()),
            operands: vec![
                DerivationOperand::included(
                    "money",
                    money_value("USD", 5.0, 500).expect("valid USD"),
                )
                .with_role(OperandRole::Minuend),
                DerivationOperand::included(
                    "other",
                    money_value("USD", 3.0, 300).expect("valid USD"),
                )
                .with_role(OperandRole::Subtrahend),
            ],
        };

        let error = plugin
            .derive(&request)
            .expect_err("date plugin must reject currency operands");

        assert!(error.to_string().contains("incompatible operand"));
    }
}
