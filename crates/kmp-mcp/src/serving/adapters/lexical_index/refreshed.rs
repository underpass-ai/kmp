use super::about_change::AboutChange;

/// What a refresh of one about ends with.
pub(super) enum Refreshed {
    Changed(AboutChange),
    /// The about's language moved, so every row must be read again.
    LanguageMoved,
}
