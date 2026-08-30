use crate::lifecycle::domain::piece::Piece;
use crate::lifecycle::ports::installation_catalog::InstallationCatalog;

/// Use case: remove one piece, or say why it did not.
///
/// It removes exactly the path it was given and never walks outward: no
/// sibling store, no parent directory, nothing that was not printed in the
/// dry run the operator read before saying `--apply`.
pub struct RemovePiece<'a> {
    installation: &'a dyn InstallationCatalog,
}

impl<'a> RemovePiece<'a> {
    pub fn new(installation: &'a dyn InstallationCatalog) -> Self {
        Self { installation }
    }

    pub fn execute(&self, piece: &Piece) -> Result<(), String> {
        if let Some(reason) = piece.refusal() {
            return Err(reason);
        }
        if !self.installation.exists(&piece.path) {
            return Ok(());
        }
        self.installation.remove_path(&piece.path)
    }
}
