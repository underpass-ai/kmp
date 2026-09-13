use kmp_embedded::EmbeddedKernelStore;
use serde_json::Value;

use crate::guide;
use crate::projection::summaries_audit_page;
use crate::serving::{ToolError, tool_success_result};
use crate::summaries::{AuditScope, SummaryAudit};

/// Reads where this store's memories stand with respect to their English
/// search summaries.
///
/// It reads the store's own event log and writes nothing: no ranking, no
/// model, no network, no memory touched. The guide's own abouts are left out
/// of the reading the same way the doctor and the terminal leave them out —
/// they are content the store did not author, and their summaries are not
/// this store's debt.
pub(crate) struct EmbeddedSummariesAuditTool<'a> {
    store: &'a EmbeddedKernelStore,
}

impl<'a> EmbeddedSummariesAuditTool<'a> {
    pub(crate) fn new(store: &'a EmbeddedKernelStore) -> Self {
        Self { store }
    }

    pub(crate) async fn call(&self, arguments: &Value) -> Result<Value, ToolError> {
        let scope = Self::scope(arguments)?;
        let bundle = self
            .store
            .export_bundle_excluding_abouts(&guide::abouts_owned())
            .await
            .map_err(|error| ToolError::backend(error.to_string()))?;
        let audit = SummaryAudit::read(&bundle, &scope).map_err(ToolError::backend)?;
        Ok(tool_success_result(summaries_audit_page(
            &audit, &scope, arguments,
        )?))
    }

    /// Which abouts the caller asked for, in the surface's own words.
    fn scope(arguments: &Value) -> Result<AuditScope, ToolError> {
        let about = arguments
            .get("about")
            .and_then(Value::as_str)
            .filter(|about| !about.is_empty());
        let named = arguments
            .pointer("/dimensions/scope")
            .and_then(Value::as_str)
            .unwrap_or("current_about");
        match named {
            "current_about" => about
                .map(|about| AuditScope::CurrentAbout(about.to_string()))
                .ok_or_else(|| {
                    ToolError::invalid_argument(
                        "kmp_summaries_audit needs `about`: the memory whose summaries to read. To \
                     read several, set dimensions.scope to `abouts` and name them in \
                     dimensions.abouts; to sweep every anchor, set it to `all_abouts`",
                    )
                }),
            "abouts" => {
                let mut abouts = arguments
                    .pointer("/dimensions/abouts")
                    .and_then(Value::as_array)
                    .map(|abouts| {
                        abouts
                            .iter()
                            .filter_map(Value::as_str)
                            .map(str::to_string)
                            .collect::<Vec<_>>()
                    })
                    .unwrap_or_default();
                if let Some(about) = about
                    && !abouts.iter().any(|named| named == about)
                {
                    abouts.push(about.to_string());
                }
                if abouts.is_empty() {
                    return Err(ToolError::invalid_argument(
                        "dimensions.scope `abouts` needs dimensions.abouts: the abouts to read \
                         together",
                    ));
                }
                Ok(AuditScope::Abouts(abouts))
            }
            "all_abouts" => Ok(AuditScope::AllAbouts),
            other => Err(ToolError::invalid_argument(format!(
                "`{other}` is not a scope; they are current_about, abouts and all_abouts"
            ))),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn the_default_scope_is_one_about_and_says_so_when_it_is_missing() {
        let error = EmbeddedSummariesAuditTool::scope(&json!({})).expect_err("no about");

        assert!(error.message.contains("all_abouts"), "{error:?}");
        assert_eq!(
            EmbeddedSummariesAuditTool::scope(&json!({"about": "project:a"})).expect("one about"),
            AuditScope::CurrentAbout("project:a".to_string())
        );
    }

    #[test]
    fn a_named_set_reads_the_list_and_keeps_the_current_about_in_it() {
        let scope = EmbeddedSummariesAuditTool::scope(&json!({
            "about": "project:a",
            "dimensions": {"scope": "abouts", "abouts": ["project:b"]}
        }))
        .expect("a named set");

        assert_eq!(
            scope,
            AuditScope::Abouts(vec!["project:b".to_string(), "project:a".to_string()])
        );
    }

    #[test]
    fn a_named_set_without_a_list_is_refused() {
        let error = EmbeddedSummariesAuditTool::scope(&json!({"dimensions": {"scope": "abouts"}}))
            .expect_err("no list");

        assert!(error.message.contains("dimensions.abouts"), "{error:?}");
    }

    #[test]
    fn every_anchor_is_the_explicit_opt_in_and_needs_no_about() {
        assert_eq!(
            EmbeddedSummariesAuditTool::scope(&json!({"dimensions": {"scope": "all_abouts"}}))
                .expect("all abouts"),
            AuditScope::AllAbouts
        );
    }
}
