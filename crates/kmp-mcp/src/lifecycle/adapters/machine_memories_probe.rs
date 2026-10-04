use crate::lifecycle::domain::diagnostic_severity::DiagnosticSeverity;
use crate::lifecycle::domain::lifecycle_finding::LifecycleFinding;
use crate::lifecycle::domain::memory_inventory_entry::MemoryInventoryEntry;
use crate::lifecycle::domain::store_contents::StoreContents;
use crate::lifecycle::domain::store_reach::StoreReach;

use super::machine_inventory_probe::machine_inventory;

/// How many abouts `info` names per store; `kmp-mcp memories` names them all.
const ABOUTS_SHOWN: usize = 8;

/// Where all the memory on this machine is, not only the one this shell would
/// open. Two of five stores on a real machine were reachable by no rule at
/// all, and nothing that shipped would ever have mentioned them.
pub(crate) fn memories_finding() -> Vec<LifecycleFinding> {
    let Some(memories) = machine_inventory() else {
        return vec![
            LifecycleFinding::new(DiagnosticSeverity::Warn, "cannot tell what memory is here")
                .with_detail(
                    "none of XDG_DATA_HOME, HOME, LOCALAPPDATA, APPDATA, or USERPROFILE is set, \
                     so there is nowhere to look",
                ),
        ];
    };
    if memories.is_empty() {
        return vec![
            LifecycleFinding::new(DiagnosticSeverity::Ok, "no memory on this machine yet")
                .with_detail(
                    "the first write creates one; where depends on where you are standing",
                ),
        ];
    }

    let unreachable = memories
        .iter()
        .filter(|memory| memory.record.reach == StoreReach::Unreachable)
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
        finding = finding
            .with_detail(store_line(memory))
            .with_detail(contents_line(&memory.contents));
    }
    if unreachable > 0 {
        finding = finding.with_detail(
            "`unreachable` means no rule resolves to it: open it with KMP_MCP_DATA_DIR, or \
             remove exactly it with `kmp-mcp uninstall --store <absolute path>`, which refuses \
             live owners and saves the memory first",
        );
    }
    finding = finding.with_detail(
        "`kmp-mcp memories` lists every about of every store; `kmp-mcp memories register \
         <absolute path>` adds a store no command has opened here",
    );
    vec![finding]
}

fn store_line(memory: &MemoryInventoryEntry) -> String {
    let record = &memory.record;
    format!(
        "{} {} · {} · {}{}{}",
        if memory.opened_here { "→" } else { " " },
        record.path.display(),
        record.size.human(),
        record.reach.as_str(),
        record
            .storage
            .as_ref()
            .map(|storage| format!(" · {}", storage.label()))
            .unwrap_or_default(),
        record
            .last_opened
            .as_deref()
            .map(|when| format!(" · last opened {when}"))
            .unwrap_or_default()
    )
}

/// One line per store: its project abouts, most events first, capped; the
/// guide abouts every store carries are folded into a count so they never
/// crowd out the memory a person wrote.
fn contents_line(contents: &StoreContents) -> String {
    let (abouts, last_write) = match contents {
        StoreContents::Read { abouts, last_write } => (abouts, last_write),
        StoreContents::Unreadable { reason } => return reason.clone(),
    };
    let (guide, own): (Vec<_>, Vec<_>) = abouts
        .iter()
        .partition(|about| crate::guide::is_guide_about(about.about()));
    let mut parts: Vec<String> = own
        .iter()
        .take(ABOUTS_SHOWN)
        .map(|about| format!("{} {}", about.about(), about.events()))
        .collect();
    if own.len() > ABOUTS_SHOWN {
        parts.push(format!("… {} more", own.len() - ABOUTS_SHOWN));
    }
    if !guide.is_empty() {
        parts.push(format!(
            "+ {} guide {}",
            guide.len(),
            if guide.len() == 1 { "about" } else { "abouts" }
        ));
    }
    let listed = if parts.is_empty() {
        "no events yet".to_string()
    } else {
        parts.join(" · ")
    };
    format!(
        "{} events{}: {listed}",
        contents.total_events(),
        last_write
            .as_deref()
            .map(|when| format!(", last write {when}"))
            .unwrap_or_default()
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lifecycle::domain::about_event_count::AboutEventCount;

    #[test]
    fn info_caps_the_abouts_and_folds_the_guide_away() {
        let mut abouts: Vec<_> = (0..11)
            .map(|index| AboutEventCount::new(format!("project:p{index:02}"), 100 - index))
            .collect();
        abouts.push(AboutEventCount::new("guide:kmp-agent", 900));
        abouts.push(AboutEventCount::new("guide:kmp", 400));
        let line = contents_line(&StoreContents::read(
            abouts,
            Some("2026-10-04 09:00:00".to_string()),
        ));

        assert!(line.contains("last write 2026-10-04 09:00:00"), "{line}");
        assert!(line.contains("project:p00 100"), "{line}");
        assert!(line.contains("project:p07 93"), "{line}");
        assert!(!line.contains("project:p08"), "{line}");
        assert!(line.contains("… 3 more"), "{line}");
        assert!(line.contains("+ 2 guide abouts"), "{line}");
        assert!(!line.contains("guide:kmp-agent"), "{line}");
    }

    #[test]
    fn an_unreadable_store_says_why_instead_of_listing_nothing() {
        let line = contents_line(&StoreContents::unreadable(
            "unsupported format-2 artifact: contents not readable by this engine",
        ));
        assert!(
            line.contains("contents not readable by this engine"),
            "{line}"
        );
    }
}
