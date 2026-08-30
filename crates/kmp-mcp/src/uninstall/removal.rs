//! Taking one piece away.
//!
//! One concept: the deletion itself, reached only after the survey named the
//! piece, the guard proved nobody holds it and the rescue made its copy.

use crate::uninstall::piece::Piece;
use crate::uninstall::rescue::refusal;

/// Removes one piece, or says why it did not.
///
/// It removes exactly the path it was given and never walks outward: no
/// sibling store, no parent directory, nothing that was not printed in the
/// dry run the operator read before saying `--apply`.
pub fn remove(piece: &Piece) -> Result<(), String> {
    if let Some(reason) = refusal(piece) {
        return Err(reason);
    }
    let path = &piece.path;
    if !path.exists() {
        return Ok(());
    }
    let result = if path.is_dir() {
        std::fs::remove_dir_all(path)
    } else {
        std::fs::remove_file(path)
    };
    result.map_err(|error| format!("could not remove `{}`: {error}", path.display()))
}
