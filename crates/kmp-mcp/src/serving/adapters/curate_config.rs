use std::path::Path;

use serde::Deserialize;

use crate::curate::domain::partner_cap::PartnerCap;
use crate::curate::domain::partner_filter::PartnerFilter;
use crate::curate::domain::paths_corridor::PathsCorridor;

/// The file beside a store that tunes the review without `focus` of
/// `kmp_curate`.
pub(super) const CURATE_FILE: &str = "curate.json";

/// Per-store settings of the review without `focus`:
/// `{"partner_facts": 512}` raises the largest about whose orphans get a Jev
/// partner round (default 120, at most 512; past 255 facts the round reads
/// the about in windows), and `"partner_filter"` (`off` by default,
/// `rare_term` or `confirm`) names what the round's pairs must pass before
/// they are typed. `"paths_corridor"` (`off` by default, or `on`) says
/// whether a path search with a goal asks about the corridor between its
/// ends (DESIGN L7) or, as before, about the facts near them; the corridor
/// is opt-in while its cost in `proposed_hops_right` awaits a decision.
/// Absent, the store gets the defaults; unreadable or unknown, it is
/// ignored and reported so, and the defaults apply.
#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct CurateConfig {
    #[serde(default)]
    partner_facts: Option<usize>,
    #[serde(default)]
    partner_filter: Option<String>,
    #[serde(default)]
    paths_corridor: Option<String>,
}

impl CurateConfig {
    /// The partner cap and filter this store reviews with: the defaults
    /// without the file, what the file says with it, an error naming why a
    /// present file cannot apply.
    pub(super) fn load(
        data_dir: &Path,
    ) -> Result<(PartnerCap, PartnerFilter, PathsCorridor), String> {
        let path = data_dir.join(CURATE_FILE);
        if !path.is_file() {
            return Ok((PartnerCap::DEFAULT, PartnerFilter::Off, PathsCorridor::Off));
        }
        let text = std::fs::read_to_string(&path)
            .map_err(|error| format!("{CURATE_FILE} is unreadable: {error}"))?;
        Self::parse(&text)
    }

    fn parse(text: &str) -> Result<(PartnerCap, PartnerFilter, PathsCorridor), String> {
        let config: Self = serde_json::from_str(text)
            .map_err(|error| format!("{CURATE_FILE} is not a curate configuration: {error}"))?;
        let cap = match config.partner_facts {
            None => PartnerCap::DEFAULT,
            Some(facts) => PartnerCap::named(&facts.to_string()).ok_or_else(|| {
                format!(
                    "{CURATE_FILE} names partner_facts {facts}; it must be between 2 and {}",
                    PartnerCap::MAX_FACTS
                )
            })?,
        };
        let filter = match config.partner_filter.as_deref() {
            None => PartnerFilter::Off,
            Some(name) => PartnerFilter::named(name).ok_or_else(|| {
                format!(
                    "{CURATE_FILE} names partner_filter `{name}`; this kmp-mcp knows `off`, `rare_term` and `confirm`"
                )
            })?,
        };
        let corridor = match config.paths_corridor.as_deref() {
            None => PathsCorridor::Off,
            Some(name) => PathsCorridor::named(name).ok_or_else(|| {
                format!(
                    "{CURATE_FILE} names paths_corridor `{name}`; this kmp-mcp knows `on` and `off`"
                )
            })?,
        };
        Ok((cap, filter, corridor))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_store_may_raise_the_partner_cap_to_512_and_name_a_filter() {
        assert_eq!(
            CurateConfig::parse("{}"),
            Ok((PartnerCap::DEFAULT, PartnerFilter::Off, PathsCorridor::Off))
        );
        assert_eq!(
            CurateConfig::parse(r#"{"partner_facts":512}"#),
            Ok((
                PartnerCap::named("512").expect("cap"),
                PartnerFilter::Off,
                PathsCorridor::Off
            ))
        );
        assert_eq!(
            CurateConfig::parse(r#"{"partner_facts":300,"partner_filter":"confirm"}"#),
            Ok((
                PartnerCap::named("300").expect("cap"),
                PartnerFilter::Confirm,
                PathsCorridor::Off
            ))
        );
        assert_eq!(
            CurateConfig::parse(r#"{"paths_corridor":"on"}"#).map(|(_, _, corridor)| corridor),
            Ok(PathsCorridor::On)
        );
        assert_eq!(
            CurateConfig::parse(r#"{"paths_corridor":"off"}"#).map(|(_, _, corridor)| corridor),
            Ok(PathsCorridor::Off)
        );
        for refused in [
            r#"{"partner_facts":513}"#,
            r#"{"partner_facts":1}"#,
            r#"{"partner_filter":"idf"}"#,
            r#"{"partners":512}"#,
            r#"{"paths_corridor":"near"}"#,
            "not json",
        ] {
            assert!(CurateConfig::parse(refused).is_err(), "{refused}");
        }
    }

    #[test]
    fn a_store_without_the_file_reviews_with_the_defaults() {
        let dir = tempfile::tempdir().expect("dir");
        assert_eq!(
            CurateConfig::load(dir.path()),
            Ok((PartnerCap::DEFAULT, PartnerFilter::Off, PathsCorridor::Off))
        );
        std::fs::write(dir.path().join(CURATE_FILE), r#"{"partner_facts":512}"#).expect("write");
        assert_eq!(
            CurateConfig::load(dir.path()).map(|(cap, _, _)| cap.facts()),
            Ok(512)
        );
        std::fs::write(dir.path().join(CURATE_FILE), "{").expect("write");
        assert!(CurateConfig::load(dir.path()).is_err());
    }
}
