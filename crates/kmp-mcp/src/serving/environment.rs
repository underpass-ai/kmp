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
/// Evaluation only: a file of recorded TypeSafe judgements. With
/// `KMP_TYPESAFE_CASSETTE_MODE=record` real answers are appended to it; in
/// the default `replay` mode they are answered from it, with no network and
/// no key, and an unrecorded request fails instead of being guessed.
pub const TYPESAFE_CASSETTE_ENV: &str = "KMP_TYPESAFE_CASSETTE";
pub const TYPESAFE_CASSETTE_MODE_ENV: &str = "KMP_TYPESAFE_CASSETTE_MODE";
/// Evaluation only: the largest about a review without `focus` asks Jev for
/// its orphans' partners in (2–255 facts). Unset, the measured default.
pub const EVAL_PARTNER_FACTS_ENV: &str = "KMP_EVAL_PARTNER_FACTS";

/// `off` lifts the deadlines a first ask or wake page waits for Jev
/// (recording a cassette against the real provider, where a slow answer
/// must be kept, not degraded). Any other value, or none, keeps them.
pub const JUDGEMENT_DEADLINES_ENV: &str = "KMP_JUDGEMENT_DEADLINES";

/// How long `site` waits for Jev on a first page, unless the operator
/// lifted the deadlines.
pub(crate) fn judgement_deadline(
    site: crate::serving::judgement_site::JudgementSite,
) -> Option<std::time::Duration> {
    if optional_env_string(JUDGEMENT_DEADLINES_ENV).as_deref() == Some("off") {
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
