//! Labels as a writer meets them: the catalogue an about already holds and
//! the question every new label must answer first — does one resemble it?
//!
//! The kernel never renames in silence. It folds spelling, compares, and
//! says what it found; the writer decides whether to reuse, to pick another
//! value, or to insist. The threshold is high on purpose: only case and
//! separators are forgiven, so a flag is never a guess.

mod entry_labels;
mod resemblance;

/// The metadata key a writer sets on a dimension to insist on a new label
/// even where one resembles it: `"true"` means the writer read the
/// catalogue and means something else. The kernel strips it before storing.
pub const INTENDED_NEW_LABEL_METADATA_KEY: &str = "writer_intended_new";

pub use entry_labels::labels_by_entry;
pub use resemblance::{
    LabelResemblance, ResemblanceKind, label_resemblances, normalized_label_token,
};
