use kmp_domain::{
    DimensionScopeMode, DimensionSelection, DimensionSelectionMode, LabelSelector,
    LabelSelectorOperator,
};
use kmp_proto::v1beta1::{
    DimensionScopeMode as ProtoDimensionScopeMode, DimensionSelection as ProtoDimensionSelection,
    DimensionSelectionMode as ProtoDimensionSelectionMode, LabelSelector as ProtoLabelSelector,
    LabelSelectorOperator as ProtoLabelSelectorOperator,
};

use super::scalars::{ProtoMappingResult, invalid_argument};

pub(super) fn proto_dimension_selection_from_domain(
    selection: &DimensionSelection,
) -> ProtoDimensionSelection {
    let mode = match selection.mode() {
        DimensionSelectionMode::All => ProtoDimensionSelectionMode::All,
        DimensionSelectionMode::Only => ProtoDimensionSelectionMode::Only,
        DimensionSelectionMode::Except => ProtoDimensionSelectionMode::Except,
    };
    ProtoDimensionSelection {
        mode: mode as i32,
        include: if selection.mode() == DimensionSelectionMode::Only {
            selection.dimensions().iter().cloned().collect()
        } else {
            Vec::new()
        },
        exclude: if selection.mode() == DimensionSelectionMode::Except {
            selection.dimensions().iter().cloned().collect()
        } else {
            Vec::new()
        },
        scope: proto_dimension_scope_mode(selection.scope_mode()) as i32,
        abouts: selection.abouts().iter().cloned().collect(),
        scope_ids: selection.scope_ids().iter().cloned().collect(),
        selectors: selection
            .selectors()
            .iter()
            .map(proto_label_selector_from_domain)
            .collect(),
    }
}

fn proto_label_selector_from_domain(selector: &LabelSelector) -> ProtoLabelSelector {
    let op = match selector.operator() {
        LabelSelectorOperator::In => ProtoLabelSelectorOperator::In,
        LabelSelectorOperator::NotIn => ProtoLabelSelectorOperator::NotIn,
        LabelSelectorOperator::Exists => ProtoLabelSelectorOperator::Exists,
        LabelSelectorOperator::NotExists => ProtoLabelSelectorOperator::NotExists,
    };
    ProtoLabelSelector {
        key: selector.key().to_string(),
        op: op as i32,
        values: selector.values().iter().cloned().collect(),
    }
}

fn domain_label_selector(value: ProtoLabelSelector) -> ProtoMappingResult<LabelSelector> {
    let operator = match value.op() {
        ProtoLabelSelectorOperator::In => LabelSelectorOperator::In,
        ProtoLabelSelectorOperator::NotIn => LabelSelectorOperator::NotIn,
        ProtoLabelSelectorOperator::Exists => LabelSelectorOperator::Exists,
        ProtoLabelSelectorOperator::NotExists => LabelSelectorOperator::NotExists,
        ProtoLabelSelectorOperator::Unspecified => {
            return Err(invalid_argument(format!(
                "label selector `{}` requires op: in, notin, exists or notexists",
                value.key.trim()
            )));
        }
    };
    LabelSelector::new(value.key, operator, value.values)
        .map_err(|error| invalid_argument(error.to_string()))
}

pub(super) fn domain_dimension_selection(
    value: Option<ProtoDimensionSelection>,
) -> ProtoMappingResult<DimensionSelection> {
    let Some(value) = value else {
        return Ok(DimensionSelection::all());
    };
    let scope = value.scope();
    let abouts = value.abouts.clone();
    let scope_ids = value.scope_ids.clone();
    let selectors = value
        .selectors
        .clone()
        .into_iter()
        .map(domain_label_selector)
        .collect::<ProtoMappingResult<Vec<_>>>()?;
    let selection = match value.mode() {
        ProtoDimensionSelectionMode::Only => {
            if value.include.is_empty() {
                return Err(invalid_argument(
                    "dimension selection mode ONLY requires include values",
                ));
            }
            if !value.exclude.is_empty() {
                return Err(invalid_argument(
                    "dimension selection mode ONLY must not set exclude values",
                ));
            }
            DimensionSelection::only(value.include)
        }
        ProtoDimensionSelectionMode::Except => {
            if value.exclude.is_empty() {
                return Err(invalid_argument(
                    "dimension selection mode EXCEPT requires exclude values",
                ));
            }
            if !value.include.is_empty() {
                return Err(invalid_argument(
                    "dimension selection mode EXCEPT must not set include values",
                ));
            }
            DimensionSelection::except(value.exclude)
        }
        ProtoDimensionSelectionMode::All | ProtoDimensionSelectionMode::Unspecified => {
            if !value.include.is_empty() || !value.exclude.is_empty() {
                return Err(invalid_argument(
                    "dimension selection mode ALL must not set include or exclude values",
                ));
            }
            DimensionSelection::all()
        }
    };
    let selection = apply_dimension_scope(selection, scope, abouts)?;
    Ok(selection
        .with_scope_ids(scope_ids)
        .with_selectors(selectors))
}

fn apply_dimension_scope(
    selection: DimensionSelection,
    scope: ProtoDimensionScopeMode,
    abouts: Vec<String>,
) -> ProtoMappingResult<DimensionSelection> {
    let abouts = abouts
        .into_iter()
        .map(|about| about.trim().to_string())
        .filter(|about| !about.is_empty())
        .collect::<Vec<_>>();
    match scope {
        ProtoDimensionScopeMode::Unspecified | ProtoDimensionScopeMode::CurrentAbout => {
            if !abouts.is_empty() {
                return Err(invalid_argument(
                    "dimension scope CURRENT_ABOUT must not set abouts",
                ));
            }
            Ok(selection.with_current_about_scope())
        }
        ProtoDimensionScopeMode::Abouts => {
            if abouts.is_empty() {
                return Err(invalid_argument(
                    "dimension scope ABOUTS requires at least one about",
                ));
            }
            Ok(selection.with_about_scope(abouts))
        }
        ProtoDimensionScopeMode::AllAbouts => {
            if !abouts.is_empty() {
                return Err(invalid_argument(
                    "dimension scope ALL_ABOUTS must not set abouts",
                ));
            }
            Ok(selection.with_all_about_scope())
        }
    }
}

fn proto_dimension_scope_mode(value: DimensionScopeMode) -> ProtoDimensionScopeMode {
    match value {
        DimensionScopeMode::CurrentAbout => ProtoDimensionScopeMode::CurrentAbout,
        DimensionScopeMode::Abouts => ProtoDimensionScopeMode::Abouts,
        DimensionScopeMode::AllAbouts => ProtoDimensionScopeMode::AllAbouts,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn selectors_round_trip_through_the_wire() {
        let proto = ProtoDimensionSelection {
            selectors: vec![
                ProtoLabelSelector {
                    key: "env".to_string(),
                    op: ProtoLabelSelectorOperator::In as i32,
                    values: vec!["prod".to_string(), "staging".to_string()],
                },
                ProtoLabelSelector {
                    key: "task".to_string(),
                    op: ProtoLabelSelectorOperator::NotExists as i32,
                    values: Vec::new(),
                },
            ],
            ..Default::default()
        };
        let domain = domain_dimension_selection(Some(proto)).expect("selectors should map");
        assert_eq!(
            domain
                .selectors()
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>(),
            vec!["env in (prod, staging)".to_string(), "!task".to_string()]
        );
        let back = proto_dimension_selection_from_domain(&domain);
        assert_eq!(back.selectors.len(), 2);
        assert_eq!(back.selectors[0].key, "env");
        assert_eq!(back.selectors[0].op, ProtoLabelSelectorOperator::In as i32);
        assert_eq!(
            back.selectors[1].op,
            ProtoLabelSelectorOperator::NotExists as i32
        );
    }

    #[test]
    fn a_selector_without_an_operator_or_with_the_wrong_values_is_refused() {
        let missing_op = ProtoDimensionSelection {
            selectors: vec![ProtoLabelSelector {
                key: "env".to_string(),
                op: ProtoLabelSelectorOperator::Unspecified as i32,
                values: vec!["prod".to_string()],
            }],
            ..Default::default()
        };
        let error = domain_dimension_selection(Some(missing_op)).expect_err("op is required");
        assert!(
            error.message().contains("requires op"),
            "{}",
            error.message()
        );

        let exists_with_values = ProtoDimensionSelection {
            selectors: vec![ProtoLabelSelector {
                key: "env".to_string(),
                op: ProtoLabelSelectorOperator::Exists as i32,
                values: vec!["prod".to_string()],
            }],
            ..Default::default()
        };
        let error =
            domain_dimension_selection(Some(exists_with_values)).expect_err("exists takes none");
        assert!(
            error.message().contains("takes no values"),
            "{}",
            error.message()
        );
    }
}
