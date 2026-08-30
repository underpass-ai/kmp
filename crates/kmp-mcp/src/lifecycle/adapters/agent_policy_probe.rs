use crate::lifecycle::domain::diagnostic_severity::DiagnosticSeverity;
use crate::lifecycle::domain::lifecycle_finding::LifecycleFinding;

pub(crate) fn agent_policy_finding() -> LifecycleFinding {
    match crate::agent_policy::load() {
        Ok(policy) => {
            let languages = if policy.ask_fallback_languages.is_empty() {
                "none".to_string()
            } else {
                policy.ask_fallback_languages.join(", ")
            };
            LifecycleFinding::new(
                DiagnosticSeverity::Ok,
                format!(
                    "semantic Ask fallback: {languages} ({})",
                    policy.source_label()
                ),
            )
            .with_detail(format!("config: {}", policy.path.display()))
            .with_detail("temporal intent bypasses Ask and navigates time first")
        }
        Err(error) => LifecycleFinding::new(DiagnosticSeverity::Warn, "agent policy is invalid")
            .with_detail(error)
            .with_detail("repair it with `kmp-mcp config ask-fallback-languages en`"),
    }
}
