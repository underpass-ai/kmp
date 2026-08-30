//! One removable — or merely nameable — part of an installation.
//!
//! One concept: what the survey found at a path, with everything a reader
//! needs to judge it before agreeing to lose it.

use std::path::PathBuf;

use crate::uninstall::piece_kind::PieceKind;

#[derive(Debug, Clone)]
pub struct Piece {
    pub kind: PieceKind,
    pub path: PathBuf,
    /// What the reader needs to judge it: a size, a store format, an event
    /// count, or the command that removes a registration this verb will not
    /// edit on someone's behalf.
    pub detail: String,
    /// Whether this piece's memory has a copy somewhere else. `None` for
    /// anything that is not a store.
    pub bundled_events: Option<u64>,
    /// Whether this verb may remove it at all. A binary outside the home it
    /// surveyed may belong to a package manager or to another user, and a
    /// registration lives inside a file that is not ours.
    pub ours_to_remove: bool,
}
