//! What to tell an operator whose memory selection has to change.
//!
//! The engine that resolves memory owns the precedence; it does not own this
//! sentence, because this sentence names `kmp-mcp`'s own verbs. It lives here
//! so the command line and the doctor cannot drift into two different
//! instructions for the same refusal.
//!
//! It names both directions an operator always has, and neither of them
//! touches a store: choose a different directory, or go back to automatic
//! selection. It never offers to convert anything, because KMP does not.

/// The repair every memory-selection refusal ends with.
pub const MEMORY_SELECTION_REPAIR: &str = concat!(
    "repair: run `kmp-mcp config memory-store <absolute-path>` with a directory this engine ",
    "can open, or `kmp-mcp config memory-store --clear` to return to automatic selection; ",
    "KMP never converts a store, so an existing one has to be moved aside by you if you want ",
    "a fresh store in its place"
);

#[cfg(test)]
mod tests {
    use super::*;

    /// The two directions have to be there, and the third one must not be.
    #[test]
    fn the_repair_offers_both_choices_and_never_a_conversion() {
        assert!(MEMORY_SELECTION_REPAIR.contains("kmp-mcp config memory-store <absolute-path>"));
        assert!(MEMORY_SELECTION_REPAIR.contains("kmp-mcp config memory-store --clear"));
        assert!(MEMORY_SELECTION_REPAIR.contains("never converts a store"));
        for forbidden in ["migrate", "upgrade the store", "overwrite"] {
            assert!(
                !MEMORY_SELECTION_REPAIR.contains(forbidden),
                "the repair must not offer to {forbidden}"
            );
        }
    }
}
