//! The lines a review draws on Jev's answers.

/// A declared relation whose reason Jev supports less than this is suspect.
pub(crate) const DOUBT_BELOW: f64 = 0.3;
/// A different type Jev prefers at least this confidently makes it suspect.
pub(crate) const RETYPE_AT: f64 = 0.7;
/// A partner Jev picks for an orphan below this confidence is not proposed.
pub(crate) const PARTNER_AT: f64 = 0.5;
/// The option that means no relation.
pub(crate) const NONE: &str = "none";
/// Relations that read the same both ways; asking their direction says
/// nothing.
pub(crate) const SYMMETRIC: [&str; 4] =
    ["same_event_as", "same_entity_as", "contradicts", "restates"];
