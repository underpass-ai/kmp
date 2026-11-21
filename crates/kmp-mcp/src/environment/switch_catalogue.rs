//! Every environment switch this build reads, in the order `doctor` lists
//! them. The reference table in `docs/embedded/configuration.md` is this
//! list, written out.

use super::EnvironmentSwitch;
use super::eval_hooks;
use super::on_off::{ON_OFF_ACCEPTED, parse_on_off};
use crate::serving::environment::{
    GRPC_ENDPOINT_ENV, GRPC_TLS_CA_PATH_ENV, GRPC_TLS_CERT_PATH_ENV, GRPC_TLS_DOMAIN_NAME_ENV,
    GRPC_TLS_KEY_PATH_ENV, GRPC_TLS_MODE_ENV, LEXICAL_INDEX_ENV, MCP_BACKEND_ENV,
    TYPESAFE_API_KEY_ENV,
};

/// Words of `KMP_MCP_BACKEND`, as a refusal names them.
#[cfg(any(debug_assertions, feature = "eval-hooks"))]
pub(crate) const BACKEND_ACCEPTED: &str =
    "embedded, grpc, fixture (test builds only) (case does not matter)";
#[cfg(not(any(debug_assertions, feature = "eval-hooks")))]
pub(crate) const BACKEND_ACCEPTED: &str = "embedded, grpc (case does not matter)";

/// Words of `KMP_LEXICAL_INDEX`, as a refusal names them.
#[cfg(any(debug_assertions, feature = "eval-hooks"))]
pub(crate) const LEXICAL_INDEX_ACCEPTED: &str =
    "on, off, true, false, 1, 0, yes, no, and shadow or verify in test builds (case does not matter)";
#[cfg(not(any(debug_assertions, feature = "eval-hooks")))]
pub(crate) const LEXICAL_INDEX_ACCEPTED: &str = ON_OFF_ACCEPTED;

fn any_text(_: &str) -> bool {
    true
}

fn on_off(value: &str) -> bool {
    parse_on_off(value).is_some()
}

/// The user-facing switches, then the hooks this build carries.
pub fn switches() -> Vec<EnvironmentSwitch> {
    let mut switches = vec![
        EnvironmentSwitch::new(
            MCP_BACKEND_ENV,
            "embedded; grpc when KMP_KERNEL_GRPC_ENDPOINT is set",
            BACKEND_ACCEPTED,
            "which kernel answers: the one in this process, or a remote gRPC kernel (experimental)",
            |value| crate::serving::backend_name::BackendName::named(value).is_some(),
        ),
        EnvironmentSwitch::new(
            GRPC_ENDPOINT_ENV,
            "unset",
            "a URL such as http://kernel:50054; https:// implies server TLS",
            "selects the remote gRPC kernel (experimental) and where it is",
            any_text,
        ),
        EnvironmentSwitch::new(
            GRPC_TLS_MODE_ENV,
            "mutual when a cert or key path is set, server when a CA path, domain name or \
             https:// endpoint is, else disabled",
            "disabled, plaintext, server, tls, mutual, mtls (case does not matter)",
            "TLS to the gRPC kernel; the same words kmp-server's KMP_GRPC_TLS_MODE accepts",
            |value| crate::serving::grpc_tls_config::parse_tls_mode(value).is_some(),
        ),
        EnvironmentSwitch::new(
            GRPC_TLS_CA_PATH_ENV,
            "unset",
            "a file path",
            "CA that verifies the gRPC kernel",
            any_text,
        ),
        EnvironmentSwitch::new(
            GRPC_TLS_CERT_PATH_ENV,
            "unset",
            "a file path",
            "client certificate for mutual TLS",
            any_text,
        ),
        EnvironmentSwitch::new(
            GRPC_TLS_KEY_PATH_ENV,
            "unset",
            "a file path",
            "client key for mutual TLS",
            any_text,
        ),
        EnvironmentSwitch::new(
            GRPC_TLS_DOMAIN_NAME_ENV,
            "unset",
            "a host name",
            "name the kernel's certificate must carry",
            any_text,
        ),
        EnvironmentSwitch::new(
            kmp_embedded::DATA_DIR_ENV,
            "the nearest project, then the saved selection, then the per-user store",
            "an absolute path (no ~, not relative)",
            "which store opens, above every other rule",
            |value| kmp_embedded::data_dir_override_refusal(value).is_none(),
        )
        .explained_by(kmp_embedded::data_dir_override_refusal),
        EnvironmentSwitch::new(
            kmp_viewer::VIEWER_ADDR_ENV,
            "127.0.0.1:7317, or a free loopback port when it is busy",
            "a loopback host:port, or off, none or an empty value for no viewer",
            "where ChronoLoom listens",
            crate::viewer::viewer_addr_accepted,
        )
        .explained_by(crate::viewer::viewer_addr_refusal),
        EnvironmentSwitch::new(
            "KMP_MCP_PASSAGES",
            "inline",
            "inline, shared (case does not matter)",
            "shared-passage tables in read results",
            |value| crate::serving::passage_projection::passages_named(value).is_some(),
        ),
        EnvironmentSwitch::new(
            crate::serving::OUTPUT_SCHEMAS_ENV,
            "off",
            ON_OFF_ACCEPTED,
            "outputSchema in tools/list",
            on_off,
        ),
        EnvironmentSwitch::new(
            LEXICAL_INDEX_ENV,
            "on",
            LEXICAL_INDEX_ACCEPTED,
            "whether asks are answered from the lexical index; off is the kill switch",
            |value| crate::serving::lexical_index_mode::LexicalIndexMode::named(value).is_some(),
        ),
        EnvironmentSwitch::new(
            crate::serving::LEXICAL_BRIDGE_ENV,
            "<store>/lexical-bridge.kmpb, then <data home>/kmp/lexical-bridge.kmpb",
            "a file path, or none for no table",
            "the cross-language word table ask uses",
            any_text,
        ),
        EnvironmentSwitch::new(
            TYPESAFE_API_KEY_ENV,
            "unset, then typesafe.env",
            "a token of at most 512 characters, no whitespace",
            "key for TypeSafe Jev; read only when the store has typesafe.json",
            any_text,
        )
        .secret(),
        EnvironmentSwitch::new(
            crate::plugin_notice::adapters::cached_github_latest_release_source::VERSION_CHECK_ENV,
            "on",
            ON_OFF_ACCEPTED,
            "whether plugin notice asks GitHub for the latest release (the only default-on \
             network call)",
            on_off,
        ),
        EnvironmentSwitch::new(
            "KMP_PLUGIN_ROOT",
            "set by the plugin launchers",
            "a directory",
            "the plugin tree the engine belongs to",
            any_text,
        ),
        EnvironmentSwitch::new(
            "KMP_INSTALL_DIR",
            "~/.local/bin",
            "a directory",
            "default of setup and update --engine-dir",
            any_text,
        ),
    ];
    switches.extend(eval_hooks::switches());
    switches
}
