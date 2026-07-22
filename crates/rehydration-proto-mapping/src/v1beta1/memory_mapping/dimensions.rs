use rehydration_domain::{DimensionScopeMode, DimensionSelection, DimensionSelectionMode};
use rehydration_proto::v1beta1::{
    DimensionScopeMode as ProtoDimensionScopeMode, DimensionSelection as ProtoDimensionSelection,
    DimensionSelectionMode as ProtoDimensionSelectionMode,
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
    }
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
    Ok(selection.with_scope_ids(scope_ids))
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
