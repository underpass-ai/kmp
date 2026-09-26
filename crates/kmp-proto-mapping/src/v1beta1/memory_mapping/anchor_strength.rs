/// Whether an anchor a question names must be found for it to be answered.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(super) enum AnchorStrength {
    /// A quantity, a date, a short bare number or a range end: searched like
    /// any other word, never required.
    Soft,
    /// An identifier the question is about: `C6.4`, `#188`, `v0.7.0`.
    Hard,
}
