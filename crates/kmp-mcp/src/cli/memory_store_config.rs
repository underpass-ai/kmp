//! `kmp-mcp config memory-store` — the user memory selection, shown and saved.
//!
//! This verb never starts a host, opens a store or creates one. It writes one
//! line in the user's config file and reads back what that line, the
//! environment and the working directory together decide, so the answer to
//! "which memory will actually open" can be had before anything opens.

use std::path::Path;

use kmp_embedded::memory_selection::{self, PRECEDENCE, SelectedMemory};
use kmp_mcp::lifecycle::domain::memory_selection_repair::MEMORY_SELECTION_REPAIR as REPAIR;

/// Saves the selection, after refusing everything that must not become one.
pub(super) fn save(raw: &str) -> Result<String, String> {
    let selection = memory_selection::select_memory(raw)
        .map_err(|refusal| format!("cannot select this memory: {refusal}\n{REPAIR}"))?;
    memory_selection::save_selection(&selection)
        .map_err(|error| format!("could not save the memory selection: {error}"))?;
    describe()
}

/// Removes the selection and returns the operator to automatic selection.
pub(super) fn clear() -> Result<String, String> {
    memory_selection::clear_selection()
        .map_err(|error| format!("could not clear the memory selection: {error}"))?;
    describe()
}

/// What is saved, what is effective, and which rule decided it.
pub(super) fn describe() -> Result<String, String> {
    let config = kmp_embedded::user_config_file::user_config_path()?;
    let saved = memory_selection::saved_selection()?;
    let mut rendered = format!("KMP user memory\n\nselection file: {}\n", config.display());
    match &saved {
        Some(selection) => rendered.push_str(&format!(
            "saved selection: {}\n",
            selection.path().display()
        )),
        None => rendered.push_str("saved selection: none (automatic selection)\n"),
    }

    // Locate, never prepare: naming the memory that would open must not
    // bring it into being in whatever directory the operator is standing in.
    match kmp_embedded::locate_data_dir_from_env() {
        Ok(resolved) => {
            rendered.push_str(&format!(
                "effective memory: {}\nchosen by: {} — {}\n",
                resolved.path().display(),
                resolved.rule_name(),
                resolved.rule_sentence()
            ));
            if let Some(problem) = unopenable_effective_memory(resolved.path()) {
                rendered.push_str(&format!("problem: {problem}\n{REPAIR}\n"));
            }
        }
        Err(error) => rendered.push_str(&format!(
            "effective memory: unresolved\nproblem: {error}\n{REPAIR}\n"
        )),
    }
    rendered.push_str(PRECEDENCE);
    rendered.push('\n');
    Ok(rendered)
}

/// Why the effective memory would refuse to open, if it would. Reported, not
/// repaired: the bytes on disk are never read, moved or restamped here.
fn unopenable_effective_memory(path: &Path) -> Option<String> {
    let selection = SelectedMemory::parse(path.to_str()?).ok()?;
    memory_selection::ensure_openable(&selection)
        .err()
        .map(|refusal| refusal.to_string())
}
