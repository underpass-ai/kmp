//! The environment switches kmp-mcp reads, held to one rule: a value the
//! engine does not accept is refused at startup, naming what it accepts,
//! instead of being read as the default.
//!
//! An empty or blank value means unset for every switch (a host that exports
//! blank variables must not kill the server); `KMP_VIEWER_ADDR` is the one
//! documented exception, where an empty value turns the viewer off. On/off
//! switches share one spelling set ([`on_off::ON_OFF_ACCEPTED`]). Test and
//! evaluation hooks are compiled into debug and `eval-hooks` builds only
//! ([`eval_hooks`]).

mod environment_switch;
pub mod eval_hooks;
pub mod on_off;
mod switch_catalogue;
mod switch_refusal;

pub use environment_switch::EnvironmentSwitch;
pub(crate) use switch_catalogue::BACKEND_ACCEPTED;
pub use switch_catalogue::switches;
pub use switch_refusal::SwitchRefusal;

/// Refuses the first switch that holds a value kmp-mcp does not accept.
/// Every verb that opens a store or a session calls this before anything
/// reads a switch.
pub fn check_environment() -> Result<(), SwitchRefusal> {
    for switch in switches() {
        switch.read()?;
    }
    Ok(())
}

/// Every switch the environment sets, with its value or its refusal, for
/// `doctor` and `info`.
pub fn switches_set() -> Vec<(EnvironmentSwitch, Result<String, SwitchRefusal>)> {
    switches()
        .into_iter()
        .filter_map(|switch| match switch.read() {
            Ok(Some(value)) => Some((switch, Ok(value))),
            Ok(None) => None,
            Err(refusal) => Some((switch, Err(refusal))),
        })
        .collect()
}

/// `KMP_*` names in the environment that this build reads nowhere: a typo,
/// a switch that was removed (`KMP_MCP_ENGINE`, `KMP_LEXICAL_MAXSCORE`), a
/// test hook a release build does not carry, or a variable of another binary
/// (`KMP_DATA_DIR` is kmp-server's).
pub fn unread_names() -> Vec<String> {
    let known = switches()
        .into_iter()
        .map(|switch| switch.name())
        .collect::<Vec<_>>();
    let mut unread = std::env::vars_os()
        .filter_map(|(name, _)| name.into_string().ok())
        .filter(|name| name.starts_with("KMP_") && !known.contains(&name.as_str()))
        .filter(|name| !LAUNCHER_ONLY.contains(&name.as_str()))
        .collect::<Vec<_>>();
    unread.sort();
    unread
}

/// Whether the environment selects the kernel in this process: no
/// `KMP_MCP_BACKEND` and no `KMP_KERNEL_GRPC_ENDPOINT`, or `embedded` by
/// name. The CLI verbs that read the local store refuse to run otherwise
/// (E14): with a remote kernel selected they used to answer from a store
/// the session never uses, and say all was well.
pub fn local_backend_selected() -> bool {
    use crate::serving::backend_name::BackendName;
    use crate::serving::environment::{GRPC_ENDPOINT_ENV, MCP_BACKEND_ENV, optional_env_string};
    match optional_env_string(MCP_BACKEND_ENV) {
        Some(value) => BackendName::named(&value) == Some(BackendName::Embedded),
        None => optional_env_string(GRPC_ENDPOINT_ENV).is_none(),
    }
}

/// Read by the plugin's shell launchers, not by this binary.
const LAUNCHER_ONLY: &[&str] = &["KMP_MCP_BIN", "KMP_GUIDE_PLUGIN_ROOT", "KMP_VERSION"];
