//! Test and evaluation hooks: switches that replace a real part of the
//! engine with a recording, a canned answer or a measurement mode.
//!
//! They exist for the test suite and the memory bench, never for a user. A
//! debug build carries them (that is what `cargo test` runs), and so does a
//! build with the `eval-hooks` cargo feature (what the bench builds). A
//! release build carries neither the names nor the code: setting one of
//! these variables on a shipped binary changes nothing, and a hook that is
//! also a value of a public switch (`KMP_MCP_BACKEND=fixture`,
//! `KMP_LEXICAL_INDEX=shadow`) is refused there like any other unknown value.

/// Whether this build carries the hooks.
pub const ENABLED: bool = cfg!(any(debug_assertions, feature = "eval-hooks"));

#[cfg(any(debug_assertions, feature = "eval-hooks"))]
mod carried {
    use crate::environment::EnvironmentSwitch;
    use crate::environment::on_off::{ON_OFF_ACCEPTED, parse_on_off};

    /// A file of recorded TypeSafe judgements. With
    /// `KMP_TYPESAFE_CASSETTE_MODE=record` real answers are appended to it;
    /// in the default `replay` mode they are answered from it, with no
    /// network and no key, and an unrecorded request fails instead of being
    /// guessed.
    pub(crate) const TYPESAFE_CASSETTE: &str = "KMP_TYPESAFE_CASSETTE";
    pub(crate) const TYPESAFE_CASSETTE_MODE: &str = "KMP_TYPESAFE_CASSETTE_MODE";
    /// The largest about a review without `focus` asks Jev for its orphans'
    /// partners in (2–512 facts), over the store's `curate.json`.
    pub(crate) const EVAL_PARTNER_FACTS: &str = "KMP_EVAL_PARTNER_FACTS";
    /// `off` lifts the deadlines a first ask or wake page waits for Jev
    /// (recording a cassette against the real provider, where a slow answer
    /// must be kept, not degraded).
    pub(crate) const JUDGEMENT_DEADLINES: &str = "KMP_JUDGEMENT_DEADLINES";
    /// Stands in for the latest GitHub release in `plugin notice`.
    pub(crate) const LATEST_VERSION: &str = "KMP_LATEST_VERSION";

    pub(crate) fn switches() -> Vec<EnvironmentSwitch> {
        vec![
            EnvironmentSwitch::hook(
                TYPESAFE_CASSETTE,
                "unset",
                "a file path",
                "judgements are replayed from, or recorded into, that file",
                |_| true,
            ),
            EnvironmentSwitch::hook(
                TYPESAFE_CASSETTE_MODE,
                "replay",
                "replay, record (case does not matter)",
                "whether the cassette answers or records",
                |value| matches!(value.to_ascii_lowercase().as_str(), "replay" | "record"),
            ),
            EnvironmentSwitch::hook(
                EVAL_PARTNER_FACTS,
                "the store's curate.json, else 120",
                "a whole number from 2 to 512",
                "partner-round cap of a kmp_curate review",
                |value| crate::curate::domain::partner_cap::PartnerCap::named(value).is_some(),
            ),
            EnvironmentSwitch::hook(
                JUDGEMENT_DEADLINES,
                "on",
                ON_OFF_ACCEPTED,
                "off lifts the waits a first page gives Jev",
                |value| parse_on_off(value).is_some(),
            ),
            EnvironmentSwitch::hook(
                LATEST_VERSION,
                "unset (ask GitHub)",
                "a release version such as 1.0.0",
                "stands in for the latest release in plugin notice",
                |value| {
                    crate::lifecycle::domain::release_version::ReleaseVersion::parse(value).is_ok()
                },
            ),
        ]
    }

    pub(crate) fn read(name: &str) -> Option<String> {
        crate::serving::environment::optional_env_string(name)
    }
}

/// The hook switches this build carries: none in a release build.
pub(crate) fn switches() -> Vec<super::EnvironmentSwitch> {
    #[cfg(any(debug_assertions, feature = "eval-hooks"))]
    {
        carried::switches()
    }
    #[cfg(not(any(debug_assertions, feature = "eval-hooks")))]
    {
        Vec::new()
    }
}

/// The judgement cassette and its mode, when this build carries the hook.
pub(crate) fn typesafe_cassette() -> (Option<String>, Option<String>) {
    #[cfg(any(debug_assertions, feature = "eval-hooks"))]
    {
        (
            carried::read(carried::TYPESAFE_CASSETTE),
            carried::read(carried::TYPESAFE_CASSETTE_MODE),
        )
    }
    #[cfg(not(any(debug_assertions, feature = "eval-hooks")))]
    {
        (None, None)
    }
}

/// The partner-round cap an evaluation names, when this build carries it.
pub(crate) fn partner_facts() -> Option<String> {
    #[cfg(any(debug_assertions, feature = "eval-hooks"))]
    {
        carried::read(carried::EVAL_PARTNER_FACTS)
    }
    #[cfg(not(any(debug_assertions, feature = "eval-hooks")))]
    {
        None
    }
}

/// Whether an evaluation lifted the deadlines a first page waits for Jev.
pub(crate) fn judgement_deadlines_lifted() -> bool {
    #[cfg(any(debug_assertions, feature = "eval-hooks"))]
    {
        carried::read(carried::JUDGEMENT_DEADLINES)
            .as_deref()
            .and_then(super::on_off::parse_on_off)
            == Some(false)
    }
    #[cfg(not(any(debug_assertions, feature = "eval-hooks")))]
    {
        false
    }
}

/// The release a test injects in place of GitHub's latest, when this build
/// carries the hook.
pub(crate) fn latest_version() -> Option<String> {
    #[cfg(any(debug_assertions, feature = "eval-hooks"))]
    {
        carried::read(carried::LATEST_VERSION)
    }
    #[cfg(not(any(debug_assertions, feature = "eval-hooks")))]
    {
        None
    }
}
