use crate::lifecycle::domain::diagnostic_severity::DiagnosticSeverity;
use crate::lifecycle::domain::lifecycle_finding::LifecycleFinding;

/// Where all the memory on this machine is, not only the one this shell would
/// open. Two of five stores on a real machine were reachable by no rule at
/// all, and nothing that shipped would ever have mentioned them.
pub(crate) fn memories_finding() -> Vec<LifecycleFinding> {
    let Some(data_home) = kmp_embedded::user_data_home() else {
        return vec![
            LifecycleFinding::new(DiagnosticSeverity::Warn, "cannot tell what memory is here")
                .with_detail(
                    "none of XDG_DATA_HOME, HOME, LOCALAPPDATA, APPDATA, or USERPROFILE is set, \
                     so there is nowhere to look",
                ),
        ];
    };
    let catalog = crate::lifecycle::FilesystemStoreCatalog::new(&data_home);
    let index = crate::lifecycle::JsonlStoreIndex::new(&data_home);
    let memories = crate::lifecycle::SurveyMemories::new(&catalog, &index).execute();
    if memories.is_empty() {
        return vec![
            LifecycleFinding::new(DiagnosticSeverity::Ok, "no memory on this machine yet")
                .with_detail(
                    "the first write creates one; where depends on where you are standing",
                ),
        ];
    }

    let opening = kmp_embedded::locate_data_dir_from_env()
        .ok()
        .map(|resolved| resolved.path().to_path_buf());
    let unreachable = memories
        .iter()
        .filter(|memory| memory.reach == crate::lifecycle::StoreReach::Unreachable)
        .count();

    let mut finding = LifecycleFinding::new(
        if unreachable > 0 {
            DiagnosticSeverity::Warn
        } else {
            DiagnosticSeverity::Ok
        },
        format!(
            "{} {} on this machine{}",
            memories.len(),
            if memories.len() == 1 {
                "memory"
            } else {
                "memories"
            },
            if unreachable > 0 {
                format!(", {unreachable} that no rule reaches")
            } else {
                String::new()
            }
        ),
    );
    for memory in &memories {
        let here = opening.as_deref() == Some(memory.path.as_path());
        finding = finding.with_detail(format!(
            "{} {} · {} · {}{}{}",
            if here { "→" } else { " " },
            memory.path.display(),
            memory.size.human(),
            memory.reach.as_str(),
            memory
                .storage
                .as_ref()
                .map(|storage| format!(" · {}", storage.label()))
                .unwrap_or_default(),
            memory
                .last_opened
                .as_deref()
                .map(|when| format!(" · last opened {when}"))
                .unwrap_or_default()
        ));
    }
    if unreachable > 0 {
        finding = finding.with_detail(
            "`unreachable` means no rule resolves to it: open it with KMP_MCP_DATA_DIR, or \
             remove exactly it with `kmp-mcp uninstall --store <absolute path>`, which refuses \
             live owners and saves the memory first",
        );
    }
    vec![finding]
}
