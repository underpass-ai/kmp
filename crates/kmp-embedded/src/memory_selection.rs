//! The memory an operator chose on purpose, and kept.
//!
//! Automatic selection answers "which memory is in front of me": the nearest
//! project, or the per-user default. That is the right answer for a checkout
//! and the wrong one for a workspace that is not a repository, where it
//! silently lands on whatever the per-user default happens to be. A saved
//! selection is the operator saying which memory this machine opens when
//! nothing more local has been said, and it survives every restart because it
//! lives in the user's config file rather than in an environment.
//!
//! It never converts anything. Selecting a store this engine cannot open is
//! refused with the reason and the repair; the bytes already on disk are not
//! read, moved, migrated or replaced.

use std::path::{Path, PathBuf};

use crate::memory_selection_refusal::SelectionRefusal;
use crate::user_config_file;

/// Root config key that owns this setting.
pub const KEY: &str = "memory_store";

/// The resolution order, said once for every surface that explains it.
pub const PRECEDENCE: &str = "precedence: the KMP_MCP_DATA_DIR environment variable, then the \
                              saved selection, then the nearest project root, then the per-user \
                              default";

/// An absolute directory a person chose to be this machine's user memory.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SelectedMemory(PathBuf);

impl SelectedMemory {
    /// The syntax a saved selection must satisfy before anything looks at
    /// the disk: a non-empty, already-expanded, absolute path that this
    /// config file can round-trip.
    pub fn parse(raw: &str) -> Result<Self, SelectionRefusal> {
        let value = raw.trim();
        if value.is_empty() {
            return Err(SelectionRefusal::Empty);
        }
        if value.contains('"') || value.contains('\n') {
            return Err(SelectionRefusal::NotQuotable(value.to_string()));
        }
        let path = Path::new(value);
        if path
            .components()
            .next()
            .is_some_and(|component| component.as_os_str() == "~")
        {
            return Err(SelectionRefusal::unexpanded("memory selection", value));
        }
        if !path.is_absolute() {
            return Err(SelectionRefusal::Relative(value.to_string()));
        }
        Ok(Self(path.to_path_buf()))
    }

    pub fn path(&self) -> &Path {
        &self.0
    }

    /// The line this selection is written as, in the config file's subset.
    pub fn rendered(&self) -> String {
        format!("{KEY} = \"{}\"", self.0.display())
    }
}

/// Whether this engine could open the chosen directory, without opening it.
///
/// A directory that does not exist yet is a valid choice: the store is
/// created on first write, which is the operator's next explicit act rather
/// than a side effect of configuring anything.
pub fn ensure_openable(selection: &SelectedMemory) -> Result<(), SelectionRefusal> {
    let path = selection.path();
    if path.exists() && !path.is_dir() {
        return Err(SelectionRefusal::NotADirectory(path.to_path_buf()));
    }
    kmp_adapter_embedded::validate_store_layout(path).map_err(|error| {
        SelectionRefusal::Incompatible {
            path: path.to_path_buf(),
            reason: error.to_string(),
        }
    })?;
    Ok(())
}

/// The selection a written value names, refusing everything that must not
/// become one. Nothing is written and no store is opened.
pub fn select_memory(raw: &str) -> Result<SelectedMemory, SelectionRefusal> {
    let selection = SelectedMemory::parse(raw)?;
    ensure_openable(&selection)?;
    Ok(selection)
}

/// What the user's config file currently selects, if anything.
pub fn saved_selection() -> Result<Option<SelectedMemory>, String> {
    let path = user_config_file::user_config_path()?;
    let text = user_config_file::read_text(&path)?;
    parse_saved_selection(&text).map_err(|error| format!("{}: {error}", path.display()))
}

/// The selection a config document carries. A value the file cannot mean is
/// an error rather than a silent fall-through to automatic selection: a
/// mistyped selection must be visible, not quietly ignored.
pub fn parse_saved_selection(text: &str) -> Result<Option<SelectedMemory>, String> {
    let Some((line, value)) = user_config_file::quoted_root_setting(text, KEY)? else {
        return Ok(None);
    };
    SelectedMemory::parse(value)
        .map(Some)
        .map_err(|refusal| format!("line {line} has invalid {KEY}: {refusal}"))
}

/// Writes the selection into the user's config file, leaving every other
/// setting in it exactly as it was.
pub fn save_selection(selection: &SelectedMemory) -> Result<(), String> {
    let path = user_config_file::user_config_path()?;
    let existing = user_config_file::read_text(&path)?;
    let updated = user_config_file::with_root_setting(&existing, KEY, &selection.rendered());
    user_config_file::write_text(&path, &updated)
}

/// Removes the selection and returns automatic selection to the operator,
/// touching no store on the way.
pub fn clear_selection() -> Result<(), String> {
    let path = user_config_file::user_config_path()?;
    let existing = user_config_file::read_text(&path)?;
    let updated = user_config_file::without_root_setting(&existing, KEY);
    user_config_file::write_text(&path, &updated)
}

#[cfg(test)]
mod tests {
    use super::*;
    use kmp_adapter_embedded::StorageEngine;

    #[test]
    fn only_an_absolute_expanded_path_can_be_saved() {
        let selection = SelectedMemory::parse("/srv/memory/kmp").expect("absolute path");
        assert_eq!(selection.path(), Path::new("/srv/memory/kmp"));
        assert_eq!(selection.rendered(), "memory_store = \"/srv/memory/kmp\"");

        assert_eq!(
            SelectedMemory::parse("   ").expect_err("empty"),
            SelectionRefusal::Empty
        );
        assert_eq!(
            SelectedMemory::parse("memory/kmp").expect_err("relative"),
            SelectionRefusal::Relative("memory/kmp".to_string())
        );
        assert!(matches!(
            SelectedMemory::parse("~/memory").expect_err("unexpanded"),
            SelectionRefusal::Unexpanded { .. }
        ));
        assert!(matches!(
            SelectedMemory::parse("/srv/mem\"ory").expect_err("unquotable"),
            SelectionRefusal::NotQuotable(_)
        ));
    }

    #[test]
    fn a_directory_that_does_not_exist_yet_is_a_valid_choice() {
        let temp = tempfile::tempdir().expect("tempdir");
        let fresh = temp.path().join("not-created-yet");
        let selection = SelectedMemory::parse(fresh.to_str().expect("path")).expect("absolute");

        ensure_openable(&selection).expect("a fresh directory is selectable");
        assert!(
            !fresh.exists(),
            "checking a selection must not bring the store into being"
        );
    }

    /// The refusal this issue exists for: an old store stays exactly where it
    /// is, byte for byte, and the selection does not happen.
    #[test]
    fn an_incompatible_store_is_refused_and_preserved_byte_for_byte() {
        let temp = tempfile::tempdir().expect("tempdir");
        let old = temp.path().join("old-store");
        std::fs::create_dir_all(old.join("store")).expect("store dir");
        std::fs::write(kmp_adapter_embedded::format_version_path(&old), "2\n").expect("stamp");
        let artifact = old.join("store").join("retired-layout.bin");
        std::fs::write(&artifact, b"irreplaceable memory").expect("legacy store");

        let refusal =
            select_memory(old.to_str().expect("path")).expect_err("an unopenable store is refused");
        assert!(matches!(refusal, SelectionRefusal::Incompatible { .. }));
        let message = refusal.to_string();
        assert!(message.contains("cannot open"), "{message}");
        assert!(message.contains("nothing was migrated"), "{message}");

        assert_eq!(
            std::fs::read(&artifact).expect("the old store is still there"),
            b"irreplaceable memory"
        );
        assert_eq!(
            std::fs::read_to_string(kmp_adapter_embedded::format_version_path(&old))
                .expect("stamp"),
            "2\n",
            "the refusal must not restamp a store it would not open"
        );
    }

    #[test]
    fn a_file_where_a_directory_was_named_is_refused_without_touching_it() {
        let temp = tempfile::tempdir().expect("tempdir");
        let file = temp.path().join("not-a-directory");
        std::fs::write(&file, b"unrelated").expect("write");

        let refusal = select_memory(file.to_str().expect("path")).expect_err("not a directory");
        assert!(matches!(refusal, SelectionRefusal::NotADirectory(_)));
        assert_eq!(std::fs::read(&file).expect("still there"), b"unrelated");
    }

    #[test]
    fn a_supported_store_is_selectable() {
        let temp = tempfile::tempdir().expect("tempdir");
        let supported = temp.path().join("supported");
        std::fs::create_dir_all(&supported).expect("dir");
        std::fs::write(
            kmp_adapter_embedded::format_version_path(&supported),
            format!("{}\n", StorageEngine::Sqlite.format_version()),
        )
        .expect("stamp");

        select_memory(supported.to_str().expect("path")).expect("a stamped store is selectable");
    }

    #[test]
    fn a_saved_selection_is_read_back_and_a_broken_one_is_reported() {
        assert_eq!(parse_saved_selection("").expect("no selection"), None);
        assert_eq!(
            parse_saved_selection("memory_store = \"/srv/memory\"\n").expect("selection"),
            Some(SelectedMemory(PathBuf::from("/srv/memory")))
        );
        assert_eq!(
            parse_saved_selection("[future]\nmemory_store = \"/srv/memory\"\n")
                .expect("a key inside a table is not this setting"),
            None
        );

        let relative = parse_saved_selection("memory_store = \"memory\"\n")
            .expect_err("a relative selection is visible, not ignored");
        assert!(
            relative.contains("line 1 has invalid memory_store"),
            "{relative}"
        );
        assert!(relative.contains("absolute path"), "{relative}");

        let unquoted = parse_saved_selection("memory_store = /srv/memory\n")
            .expect_err("an unquoted value is not TOML we accept");
        assert!(unquoted.contains("quoted value"), "{unquoted}");
    }
}
