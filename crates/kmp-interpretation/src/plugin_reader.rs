use std::collections::BTreeSet;

use kmp_plugin_api::{
    DerivationRequest, DerivationResult, EvidenceDerivationPlugin, EvidenceInterpretationInput,
    EvidenceInterpretationOutput, EvidenceValuePlugin, InterpretationError,
    InterpretedValueMention,
};
use serde::Serialize;

use crate::{
    CurrencyDerivationPlugin, DateDerivationPlugin, DateValuePlugin, MathExpressionValuePlugin,
    MoneyValuePlugin, SourceCodeValuePlugin, UrlValuePlugin, ValueOperationPlugin,
};

pub struct ComposedEvidenceReader {
    value_plugins: Vec<Box<dyn EvidenceValuePlugin>>,
    derivation_plugins: Vec<Box<dyn EvidenceDerivationPlugin>>,
}

impl ComposedEvidenceReader {
    pub fn configure() -> EvidenceReaderPluginConfigurator {
        EvidenceReaderPluginConfigurator::new()
    }

    pub fn try_new(
        value_plugins: Vec<Box<dyn EvidenceValuePlugin>>,
        derivation_plugins: Vec<Box<dyn EvidenceDerivationPlugin>>,
    ) -> Result<Self, InterpretationError> {
        ensure_unique_value_plugin_ids(&value_plugins)?;
        ensure_unique_derivation_plugin_ids(&derivation_plugins)?;
        Ok(Self {
            value_plugins,
            derivation_plugins,
        })
    }

    pub fn kernel_default() -> Self {
        EvidenceReaderPluginConfigurator::kernel_default()
            .build()
            .expect("kernel default plugin ids must be unique")
    }

    pub fn value_plugin_ids(&self) -> Vec<&'static str> {
        self.value_plugins
            .iter()
            .map(|plugin| plugin.id())
            .collect()
    }

    pub fn derivation_plugin_ids(&self) -> Vec<&'static str> {
        self.derivation_plugins
            .iter()
            .map(|plugin| plugin.id())
            .collect()
    }

    pub fn configuration(&self) -> EvidenceReaderPluginConfiguration {
        EvidenceReaderPluginConfiguration::from_plugins(
            &self.value_plugins,
            &self.derivation_plugins,
        )
    }

    pub fn read(
        &self,
        request: &EvidenceReaderRequest,
    ) -> Result<EvidenceReaderOutput, InterpretationError> {
        let mut value_outputs = Vec::new();
        let mut values = Vec::new();
        let mut diagnostics = Vec::new();
        let mut execution_order = Vec::new();

        for (index, plugin) in self.value_plugins.iter().enumerate() {
            execution_order.push(EvidenceReaderPluginExecution::new(
                EvidenceReaderPluginPhase::ValueInterpretation,
                execution_order.len(),
                index,
                plugin.id(),
            ));
            let output = plugin.interpret(&request.evidence)?;
            diagnostics.extend(output.diagnostics.clone());
            values.extend(output.values.clone());
            value_outputs.push(output);
        }

        let mut derivation_results = Vec::new();
        for (index, step) in request.derivations.iter().enumerate() {
            let plugin = self
                .derivation_plugins
                .iter()
                .find(|plugin| plugin.id() == step.plugin_id)
                .ok_or_else(|| {
                    InterpretationError::new(format!(
                        "derivation plugin `{}` is not registered",
                        step.plugin_id
                    ))
                })?;
            execution_order.push(EvidenceReaderPluginExecution::new(
                EvidenceReaderPluginPhase::Derivation,
                execution_order.len(),
                index,
                plugin.id(),
            ));
            let result = plugin.derive(&step.request)?;
            diagnostics.extend(result.diagnostics.clone());
            derivation_results.push(result);
        }

        Ok(EvidenceReaderOutput {
            reader: "composed-evidence-reader-v1",
            value_plugin_ids: self.value_plugin_ids(),
            derivation_plugin_ids: self.derivation_plugin_ids(),
            plugin_configuration: self.configuration(),
            execution_order,
            value_outputs,
            values,
            derivation_results,
            diagnostics,
        })
    }
}

#[derive(Default)]
pub struct EvidenceReaderPluginConfigurator {
    value_plugins: Vec<Box<dyn EvidenceValuePlugin>>,
    derivation_plugins: Vec<Box<dyn EvidenceDerivationPlugin>>,
}

impl EvidenceReaderPluginConfigurator {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn kernel_default() -> Self {
        Self::new()
            .with_value_plugin(SourceCodeValuePlugin)
            .with_value_plugin(MathExpressionValuePlugin)
            .with_value_plugin(UrlValuePlugin)
            .with_value_plugin(MoneyValuePlugin)
            .with_value_plugin(DateValuePlugin)
            .with_derivation_plugin(ValueOperationPlugin)
            .with_derivation_plugin(CurrencyDerivationPlugin)
            .with_derivation_plugin(DateDerivationPlugin)
    }

    pub fn with_value_plugin<P>(self, plugin: P) -> Self
    where
        P: EvidenceValuePlugin + 'static,
    {
        self.with_boxed_value_plugin(Box::new(plugin))
    }

    pub fn with_boxed_value_plugin(mut self, plugin: Box<dyn EvidenceValuePlugin>) -> Self {
        self.value_plugins.push(plugin);
        self
    }

    pub fn with_derivation_plugin<P>(self, plugin: P) -> Self
    where
        P: EvidenceDerivationPlugin + 'static,
    {
        self.with_boxed_derivation_plugin(Box::new(plugin))
    }

    pub fn with_boxed_derivation_plugin(
        mut self,
        plugin: Box<dyn EvidenceDerivationPlugin>,
    ) -> Self {
        self.derivation_plugins.push(plugin);
        self
    }

    pub fn configuration(&self) -> EvidenceReaderPluginConfiguration {
        EvidenceReaderPluginConfiguration::from_plugins(
            &self.value_plugins,
            &self.derivation_plugins,
        )
    }

    pub fn build(self) -> Result<ComposedEvidenceReader, InterpretationError> {
        ComposedEvidenceReader::try_new(self.value_plugins, self.derivation_plugins)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct EvidenceReaderRequest {
    pub evidence: EvidenceInterpretationInput,
    pub derivations: Vec<EvidenceReaderDerivation>,
}

impl EvidenceReaderRequest {
    pub fn new(evidence: EvidenceInterpretationInput) -> Self {
        Self {
            evidence,
            derivations: Vec::new(),
        }
    }

    pub fn with_derivation(mut self, derivation: EvidenceReaderDerivation) -> Self {
        self.derivations.push(derivation);
        self
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct EvidenceReaderDerivation {
    pub plugin_id: String,
    pub request: DerivationRequest,
}

impl EvidenceReaderDerivation {
    pub fn new(plugin_id: impl Into<String>, request: DerivationRequest) -> Self {
        Self {
            plugin_id: plugin_id.into(),
            request,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct EvidenceReaderPluginConfiguration {
    pub plugin_order: Vec<EvidenceReaderPluginExecution>,
    pub value_plugins: Vec<&'static str>,
    pub derivation_plugins: Vec<&'static str>,
}

impl EvidenceReaderPluginConfiguration {
    fn from_plugins(
        value_plugins: &[Box<dyn EvidenceValuePlugin>],
        derivation_plugins: &[Box<dyn EvidenceDerivationPlugin>],
    ) -> Self {
        let value_order = value_plugins.iter().enumerate().map(|(index, plugin)| {
            EvidenceReaderPluginExecution::new(
                EvidenceReaderPluginPhase::ValueInterpretation,
                index,
                index,
                plugin.id(),
            )
        });
        let derivation_order = derivation_plugins
            .iter()
            .enumerate()
            .map(|(index, plugin)| {
                EvidenceReaderPluginExecution::new(
                    EvidenceReaderPluginPhase::Derivation,
                    value_plugins.len() + index,
                    index,
                    plugin.id(),
                )
            });
        Self {
            plugin_order: value_order.chain(derivation_order).collect(),
            value_plugins: value_plugins.iter().map(|plugin| plugin.id()).collect(),
            derivation_plugins: derivation_plugins
                .iter()
                .map(|plugin| plugin.id())
                .collect(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum EvidenceReaderPluginPhase {
    ValueInterpretation,
    Derivation,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct EvidenceReaderPluginExecution {
    pub phase: EvidenceReaderPluginPhase,
    pub global_order: usize,
    pub phase_order: usize,
    pub plugin_id: &'static str,
}

impl EvidenceReaderPluginExecution {
    pub fn new(
        phase: EvidenceReaderPluginPhase,
        global_order: usize,
        phase_order: usize,
        plugin_id: &'static str,
    ) -> Self {
        Self {
            phase,
            global_order,
            phase_order,
            plugin_id,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct EvidenceReaderOutput {
    pub reader: &'static str,
    pub value_plugin_ids: Vec<&'static str>,
    pub derivation_plugin_ids: Vec<&'static str>,
    pub plugin_configuration: EvidenceReaderPluginConfiguration,
    pub execution_order: Vec<EvidenceReaderPluginExecution>,
    pub value_outputs: Vec<EvidenceInterpretationOutput>,
    pub values: Vec<InterpretedValueMention>,
    pub derivation_results: Vec<DerivationResult>,
    pub diagnostics: Vec<String>,
}

fn ensure_unique_value_plugin_ids(
    plugins: &[Box<dyn EvidenceValuePlugin>],
) -> Result<(), InterpretationError> {
    let mut seen = BTreeSet::new();
    for plugin in plugins {
        let id = plugin.id();
        if !seen.insert(id) {
            return Err(InterpretationError::new(format!(
                "duplicate value plugin id `{id}`"
            )));
        }
    }
    Ok(())
}

fn ensure_unique_derivation_plugin_ids(
    plugins: &[Box<dyn EvidenceDerivationPlugin>],
) -> Result<(), InterpretationError> {
    let mut seen = BTreeSet::new();
    for plugin in plugins {
        let id = plugin.id();
        if !seen.insert(id) {
            return Err(InterpretationError::new(format!(
                "duplicate derivation plugin id `{id}`"
            )));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use kmp_plugin_api::{
        CurrencyCode, DerivationOperand, DerivationOperation, DerivationRequest, EvidenceFragment,
        EvidenceInterpretationInput, InterpretedValue,
    };

    use super::*;

    #[test]
    fn default_reader_applies_all_registered_value_plugins() {
        let reader = ComposedEvidenceReader::kernel_default();
        let input = EvidenceInterpretationInput::new(vec![EvidenceFragment::new(
            "turn:1",
            "Budget is $70, formula $x+y$, code `cargo test`, and https://example.test.",
        )]);

        let output = reader
            .read(&EvidenceReaderRequest::new(input))
            .expect("reader should apply plugins");

        assert_eq!(
            output.value_plugin_ids,
            vec![
                "source-code-value-v1",
                "math-expression-value-v1",
                "url-value-v1",
                "money-value-v1",
                "date-value-v1"
            ]
        );
        assert_eq!(
            output
                .execution_order
                .iter()
                .map(|step| step.plugin_id)
                .collect::<Vec<_>>(),
            vec![
                "source-code-value-v1",
                "math-expression-value-v1",
                "url-value-v1",
                "money-value-v1",
                "date-value-v1"
            ]
        );
        assert_eq!(
            output.plugin_configuration.plugin_order[0],
            EvidenceReaderPluginExecution::new(
                EvidenceReaderPluginPhase::ValueInterpretation,
                0,
                0,
                "source-code-value-v1"
            )
        );
        assert!(
            output
                .values
                .iter()
                .any(|mention| matches!(mention.value, InterpretedValue::Money { .. }))
        );
        assert!(
            output
                .values
                .iter()
                .any(|mention| matches!(mention.value, InterpretedValue::MathExpression { .. }))
        );
        assert!(
            output
                .values
                .iter()
                .any(|mention| matches!(mention.value, InterpretedValue::SourceCode { .. }))
        );
        assert!(
            output
                .values
                .iter()
                .any(|mention| matches!(mention.value, InterpretedValue::Url { .. }))
        );
    }

    #[test]
    fn reader_routes_explicit_derivation_to_requested_plugin() {
        let reader = ComposedEvidenceReader::kernel_default();
        let request = EvidenceReaderRequest::new(EvidenceInterpretationInput::new(Vec::new()))
            .with_derivation(EvidenceReaderDerivation::new(
                "currency-derivation-v1",
                DerivationRequest {
                    question: "total".to_string(),
                    operation: DerivationOperation::Sum,
                    unit: None,
                    operands: vec![
                        DerivationOperand::included(
                            "a",
                            InterpretedValue::Money {
                                currency: CurrencyCode::new("USD").expect("valid currency"),
                                amount_minor: 1_200,
                                amount: 12.0,
                            },
                        ),
                        DerivationOperand::included(
                            "b",
                            InterpretedValue::Money {
                                currency: CurrencyCode::new("USD").expect("valid currency"),
                                amount_minor: 300,
                                amount: 3.0,
                            },
                        ),
                    ],
                },
            ));

        let output = reader.read(&request).expect("derivation should route");

        assert_eq!(output.derivation_results.len(), 1);
        assert_eq!(
            output
                .execution_order
                .iter()
                .map(|step| step.plugin_id)
                .collect::<Vec<_>>(),
            vec![
                "source-code-value-v1",
                "math-expression-value-v1",
                "url-value-v1",
                "money-value-v1",
                "date-value-v1",
                "currency-derivation-v1"
            ]
        );
        assert_eq!(
            output.derivation_results[0].plugin,
            "currency-derivation-v1"
        );
        assert_eq!(
            output.derivation_results[0].answer.as_deref(),
            Some("USD 15")
        );
    }

    #[test]
    fn reader_fails_fast_for_unregistered_derivation_plugin() {
        let reader = ComposedEvidenceReader::kernel_default();
        let request = EvidenceReaderRequest::new(EvidenceInterpretationInput::new(Vec::new()))
            .with_derivation(EvidenceReaderDerivation::new(
                "missing-plugin",
                DerivationRequest {
                    question: "total".to_string(),
                    operation: DerivationOperation::Sum,
                    unit: None,
                    operands: Vec::new(),
                },
            ));

        let error = reader
            .read(&request)
            .expect_err("missing plugin must fail fast");

        assert_eq!(
            error.to_string(),
            "derivation plugin `missing-plugin` is not registered"
        );
    }

    #[test]
    fn reader_rejects_duplicate_value_plugin_ids() {
        let result = ComposedEvidenceReader::try_new(
            vec![Box::new(MoneyValuePlugin), Box::new(MoneyValuePlugin)],
            Vec::new(),
        );
        let Err(error) = result else {
            panic!("duplicate plugin ids should fail");
        };

        assert_eq!(
            error.to_string(),
            "duplicate value plugin id `money-value-v1`"
        );
    }

    #[test]
    fn configurator_preserves_custom_plugin_order() {
        let reader = ComposedEvidenceReader::configure()
            .with_value_plugin(UrlValuePlugin)
            .with_value_plugin(MoneyValuePlugin)
            .with_derivation_plugin(DateDerivationPlugin)
            .build()
            .expect("custom configuration should build");

        let configuration = reader.configuration();

        assert_eq!(
            configuration
                .plugin_order
                .iter()
                .map(|step| (
                    step.phase,
                    step.global_order,
                    step.phase_order,
                    step.plugin_id
                ))
                .collect::<Vec<_>>(),
            vec![
                (
                    EvidenceReaderPluginPhase::ValueInterpretation,
                    0,
                    0,
                    "url-value-v1"
                ),
                (
                    EvidenceReaderPluginPhase::ValueInterpretation,
                    1,
                    1,
                    "money-value-v1"
                ),
                (
                    EvidenceReaderPluginPhase::Derivation,
                    2,
                    0,
                    "date-derivation-v1"
                )
            ]
        );
    }

    #[test]
    fn reader_rejects_duplicate_derivation_plugin_ids() {
        let result = ComposedEvidenceReader::try_new(
            Vec::new(),
            vec![
                Box::new(ValueOperationPlugin),
                Box::new(ValueOperationPlugin),
            ],
        );
        let Err(error) = result else {
            panic!("duplicate plugin ids should fail");
        };

        assert_eq!(
            error.to_string(),
            "duplicate derivation plugin id `value-operation-v1`"
        );
    }
}
