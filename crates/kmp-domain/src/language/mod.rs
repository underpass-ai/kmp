//! What the kernel knows about the languages a memory is written in.
//!
//! Nothing here translates or rewrites anything. It reads which language a
//! body of text is in from the words a writer does not choose, and folds a
//! word into the one form two writers would agree on. The retrieval layer
//! stems on top of this; the write path lints on top of it. Both need the
//! same reading, which is why it lives below either of them.

mod identifiers;
mod language_vocabulary;
mod search_tokens;

pub use identifiers::{identifiers, surface_tokens};
pub use language_vocabulary::{KERNEL_LANGUAGE, LanguageVocabulary};
pub use search_tokens::{fold_search_term, informative_tokens};
