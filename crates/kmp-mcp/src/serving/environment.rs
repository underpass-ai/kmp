//! The environment variables through which a host selects and secures a
//! backend, and the two readings every consumer must agree on.

use std::path::PathBuf;

pub const GRPC_ENDPOINT_ENV: &str = "KMP_KERNEL_GRPC_ENDPOINT";
pub const MCP_BACKEND_ENV: &str = "KMP_MCP_BACKEND";
pub const GRPC_TLS_MODE_ENV: &str = "KMP_KERNEL_GRPC_TLS_MODE";
pub const GRPC_TLS_CA_PATH_ENV: &str = "KMP_KERNEL_GRPC_TLS_CA_PATH";
pub const GRPC_TLS_CERT_PATH_ENV: &str = "KMP_KERNEL_GRPC_TLS_CERT_PATH";
pub const GRPC_TLS_KEY_PATH_ENV: &str = "KMP_KERNEL_GRPC_TLS_KEY_PATH";
pub const GRPC_TLS_DOMAIN_NAME_ENV: &str = "KMP_KERNEL_GRPC_TLS_DOMAIN_NAME";
/// Bearer key for the optional TypeSafe judgement adapter. Never logged.
pub const TYPESAFE_API_KEY_ENV: &str = "TYPESAFE_API_KEY";

/// The lexical sidecar (`lexical-index.sqlite3`, DESIGN L6): on (the
/// default) answers every ask it can hold from its postings; off keeps it
/// closed. It reads the on/off words every switch reads; debug and
/// `eval-hooks` builds also accept the measurement modes `shadow` and
/// `verify` ([`crate::serving::lexical_index_mode::LexicalIndexMode`]). Any
/// other value is refused at startup.
pub const LEXICAL_INDEX_ENV: &str = "KMP_LEXICAL_INDEX";

/// How long `site` waits for Jev on a first page, unless an evaluation lifted
/// the deadlines ([`crate::environment::eval_hooks`]).
pub(crate) fn judgement_deadline(
    site: crate::serving::judgement_site::JudgementSite,
) -> Option<std::time::Duration> {
    if crate::environment::eval_hooks::judgement_deadlines_lifted() {
        return None;
    }
    site.deadline()
}

pub(crate) fn optional_env_path(name: &str) -> Option<PathBuf> {
    optional_env_string(name).map(PathBuf::from)
}

pub(crate) fn optional_env_string(name: &str) -> Option<String> {
    std::env::var(name)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}
