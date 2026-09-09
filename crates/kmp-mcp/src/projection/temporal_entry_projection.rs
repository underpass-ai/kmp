//! Explicit entry projection never shortens selected text or silently drops proof.
use serde_json::{Value, json};

use crate::contract::temporal_entry_field::TemporalEntryField;
use crate::serving::ToolError;

pub(super) struct TemporalEntryProjection {
    included: Vec<TemporalEntryField>,
    omitted: Vec<TemporalEntryField>,
}

impl TemporalEntryProjection {
    pub(super) fn read(arguments: &Value) -> Result<Option<Self>, ToolError> {
        let Some(fields) = arguments.get("fields") else {
            return Ok(None);
        };
        let invalid = || {
            ToolError::invalid_argument(
                "fields must be an array of distinct temporal entry field names",
            )
            .with_feedback(json!({"code":"READ_INVALID_FIELDS", "field":"fields",
                "allowed":TemporalEntryField::ALL}))
        };
        let requested: Vec<TemporalEntryField> =
            serde_json::from_value(fields.clone()).map_err(|_| invalid())?;
        if requested
            .iter()
            .enumerate()
            .any(|(index, field)| requested[..index].contains(field))
        {
            return Err(invalid());
        }
        let (included, omitted) = TemporalEntryField::ALL
            .into_iter()
            .partition(|field| field.is_identity() || requested.contains(field));
        Ok(Some(Self { included, omitted }))
    }

    pub(super) fn apply(&self, value: &mut Value, arguments: &Value) -> Result<(), ToolError> {
        value["selection"]["fields"] = json!({"included":self.included,"omitted":self.omitted});
        if self.omitted.is_empty() {
            return Ok(());
        }
        let entries = value["entries"]
            .as_array_mut()
            .ok_or_else(|| ToolError::backend("temporal response lacks entries"))?;
        for entry in entries {
            let reference = entry["ref"]
                .as_str()
                .ok_or_else(|| ToolError::backend("temporal entry lacks a ref"))?
                .to_string();
            let object = entry
                .as_object_mut()
                .ok_or_else(|| ToolError::backend("temporal entry is not an object"))?;
            object.retain(|key, _| self.included.iter().any(|field| field.as_str() == key));
            // Goto retains the original about scope, labels and interval. An
            // Inspect action cannot safely guess the owner of a cross-about ref.
            let mut detail = arguments.clone();
            let args = detail.as_object_mut().expect("validated arguments");
            for key in ["fields", "page", "from", "around", "at", "window"] {
                args.remove(key);
            }
            args.insert("at".into(), json!({"ref":reference}));
            detail["limit"]["entries"] = json!(1);
            object.insert(
                "detail_action".into(),
                json!({"tool":"kmp_goto","arguments":detail}),
            );
        }
        Ok(())
    }
}
