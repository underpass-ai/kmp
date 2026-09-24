//! The lines a review draws on Jev's answers.

/// A declared relation whose reason Jev supports less than this is suspect.
pub(crate) const DOUBT_BELOW: f64 = 0.3;
/// A different type Jev prefers at least this confidently makes it suspect.
pub(crate) const RETYPE_AT: f64 = 0.7;
/// Two facts Jev reads as contradicting at least this much are proposed as
/// `contradicts`.
pub(crate) const CONTRADICTION_AT: f64 = 0.7;
/// A partner Jev picks for an orphan below this confidence is not proposed.
pub(crate) const PARTNER_AT: f64 = 0.5;
/// The option that means no relation.
pub(crate) const NONE: &str = "none";
