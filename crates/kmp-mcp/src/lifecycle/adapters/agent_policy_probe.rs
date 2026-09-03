use crate::lifecycle::domain::diagnostic_severity::DiagnosticSeverity;
use crate::lifecycle::domain::lifecycle_finding::LifecycleFinding;

pub(crate) fn agent_policy_finding() -> LifecycleFinding {
    match crate::agent_policy::load() {
        Ok(policy) => {
            let mut finding = LifecycleFinding::new(
                DiagnosticSeverity::Ok,
                format!(
                    "memory routing: {} ({})",
                    policy.memory_routing.label(),
                    policy.routing_source_label()
                ),
            )
            .with_detail(format!("config: {}", policy.path.display()))
            .with_detail(
                "a semantic question is asked in English with the user's words as asked_as; \
                 temporal intent bypasses Ask and navigates time first",
            );
            if let Some(notice) = policy.retired_setting_notice() {
                finding = finding.with_detail(notice);
            }
            finding
        }
        Err(error) => LifecycleFinding::new(DiagnosticSeverity::Warn, "agent policy is invalid")
            .with_detail(error)
            .with_detail("repair it with `kmp-mcp config memory-routing on-request`"),
    }
}
