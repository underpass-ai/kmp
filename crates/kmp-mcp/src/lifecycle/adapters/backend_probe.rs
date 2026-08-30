use crate::lifecycle::domain::diagnostic_severity::DiagnosticSeverity;
use crate::lifecycle::domain::lifecycle_finding::LifecycleFinding;

pub(crate) fn backend_finding() -> LifecycleFinding {
    let configured = std::env::var(crate::MCP_BACKEND_ENV)
        .ok()
        .map(|value| value.trim().to_ascii_lowercase())
        .filter(|value| !value.is_empty());
    let endpoint = std::env::var(crate::GRPC_ENDPOINT_ENV)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty());

    match configured.as_deref() {
        Some("embedded") => LifecycleFinding::new(
            DiagnosticSeverity::Ok,
            "embedded — the kernel is right here",
        ),
        Some("fixture" | "fixtures") => LifecycleFinding::new(
            DiagnosticSeverity::Warn,
            "fixture — canned answers that look real",
        )
        .with_detail("nothing you write is stored; unset the variable for the real kernel"),
        Some("grpc" | "live") => match endpoint {
            Some(endpoint) => LifecycleFinding::new(
                DiagnosticSeverity::Ok,
                format!("grpc — talking to {endpoint}"),
            ),
            None => {
                LifecycleFinding::new(DiagnosticSeverity::Fail, "grpc, with no kernel to talk to")
                    .with_detail(format!(
                        "set {} , or unset {} and the kernel runs right here",
                        crate::GRPC_ENDPOINT_ENV,
                        crate::MCP_BACKEND_ENV
                    ))
            }
        },
        Some(other) => LifecycleFinding::new(
            DiagnosticSeverity::Fail,
            format!("`{other}` is not a backend"),
        )
        .with_detail("use `embedded` (the default), `grpc` or `fixture`"),
        // Nothing set is not a gap to warn about any more: it is the product.
        // The old text called this "no backend selected" and warned about it,
        // which was a fossil of the Kubernetes-first days — and the second
        // thing a stranger read.
        None => match endpoint {
            Some(endpoint) => LifecycleFinding::new(
                DiagnosticSeverity::Ok,
                format!("grpc — talking to {endpoint}"),
            )
            .with_detail("an endpoint in the environment is how the cluster edition is chosen"),
            None => LifecycleFinding::new(
                DiagnosticSeverity::Ok,
                "embedded — the default, nothing to configure",
            ),
        },
    }
}
