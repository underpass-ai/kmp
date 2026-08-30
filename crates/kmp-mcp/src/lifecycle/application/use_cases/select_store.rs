use std::path::Path;

use crate::lifecycle::domain::piece::Piece;
use crate::lifecycle::domain::piece_kind::PieceKind;
use crate::lifecycle::ports::installation_catalog::InstallationCatalog;

/// Use case: build the one-piece plan for `uninstall --store`.
///
/// The selector is deliberately absolute and resolves to one canonical store
/// identity. The report therefore names the exact path protected by the
/// lease even when the caller reached it through a symlink or a `.`
/// component.
pub struct SelectStore<'a> {
    installation: &'a dyn InstallationCatalog,
}

impl<'a> SelectStore<'a> {
    pub fn new(installation: &'a dyn InstallationCatalog) -> Self {
        Self { installation }
    }

    pub fn execute(&self, path: &Path) -> Result<Piece, String> {
        if !path.is_absolute() {
            return Err(format!(
                "--store requires an absolute path; `{}` is relative",
                path.display()
            ));
        }
        let path = self.installation.canonicalize(path).map_err(|error| {
            format!(
                "could not resolve selected store `{}`: {error}",
                path.display()
            )
        })?;
        if !self.installation.is_directory(&path)
            || !self.installation.is_file(&path.join("FORMAT_VERSION"))
        {
            return Err(format!(
                "`{}` is not a KMP store: expected a directory containing FORMAT_VERSION",
                path.display()
            ));
        }
        let format = self
            .installation
            .store_stamp(&path)
            .unwrap_or_else(|| "?".to_string());
        Ok(Piece {
            kind: PieceKind::Store,
            detail: format!(
                "{} · store format {format}",
                self.installation.size_of(&path).human()
            ),
            bundled_events: bundle_event_count_beside(self.installation, &path),
            path,
            ours_to_remove: true,
        })
    }
}

fn bundle_event_count_beside(installation: &dyn InstallationCatalog, store: &Path) -> Option<u64> {
    if store.file_name()? != std::ffi::OsStr::new(".kernel") {
        return None;
    }
    let bundle = store.parent()?.join(kmp_embedded::PROJECT_BUNDLE_PATH);
    installation.bundle_event_count(&bundle)
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::SelectStore;
    use crate::lifecycle::adapters::native_installation_catalog::NativeInstallationCatalog;
    use crate::lifecycle::domain::piece_kind::PieceKind;

    fn store_at(path: &Path, format: &str) {
        std::fs::create_dir_all(path.join("store")).expect("store dir");
        std::fs::write(path.join("FORMAT_VERSION"), format).expect("format stamp");
        std::fs::write(path.join("store/kernel.sqlite3"), vec![0u8; 2_048]).expect("store file");
    }

    #[test]
    fn selective_uninstall_requires_one_real_absolute_store() {
        let base = tempfile::tempdir().expect("temp");
        let store = base.path().join("memory");
        store_at(&store, "2");

        let select = SelectStore::new(&NativeInstallationCatalog);
        assert!(select.execute(Path::new("memory")).is_err());
        assert!(select.execute(&base.path().join("missing")).is_err());

        let selected = select.execute(&store).expect("the exact store is selected");
        assert_eq!(selected.kind, PieceKind::Store);
        assert_eq!(
            selected.path,
            std::fs::canonicalize(store).expect("canonical store")
        );
    }
}
