//! `kmp-mcp uninstall` — the inverse `/kmp:setup` never had.
//!
//! There was no supported way to remove an installation, or one store inside
//! it, and the last instruction left to a user was `rm -rf` against paths they
//! had to work out themselves. On one machine an install already spans two
//! engine copies, two stores on two formats, a committed bundle, a Claude Code
//! plugin cache and a Codex prompt directory — a list nobody can enumerate
//! from memory, which is the whole reason a verb has to.
//!
//! Two rules shape everything here.
//!
//! **Nothing is removed until it has been shown.** The dry run is the
//! default, and it prints exactly what `--apply` would take: path, kind, size.
//! A destructive command whose first run destroys is one people learn to fear
//! and then avoid.
//!
//! **Memory is saved before it is removed.** Every store is exported into the
//! working directory first and the file is named out loud, so the last thing
//! an uninstall does before deleting memory is hand it back. A copy made at
//! the moment of removal beats a bundle committed at some earlier point,
//! because only one of the two is certainly current. If the export cannot be
//! made — a live session holding the store, a directory that will not take a
//! file — the store stays. `--purge` is how someone says they want it gone
//! without a copy.

mod description;
mod discovery;
mod piece;
mod piece_kind;
mod removal;
mod removal_guard;
mod report;
mod rescue;
mod roots;
mod store_lease;
mod survey;
mod test_support;

pub use piece::Piece;
pub use piece_kind::PieceKind;
pub use removal::remove;
pub use removal_guard::StoreRemovalGuard;
pub use report::report;
pub use rescue::{refusal, rescue_path};
pub use roots::Roots;
pub use store_lease::{StoreSessionLease, store_leases_dir};
pub use survey::{selected_store, survey};
