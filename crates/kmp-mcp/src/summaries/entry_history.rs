use super::entry_revision::EntryRevision;

/// Every write of one memory entry, in the order the store met them.
///
/// The latest write decides what the entry is now — the same rule
/// `document` reads an about by. The earlier ones are kept for one question
/// the latest write cannot answer on its own: whether the text moved after
/// the summary was last written.
#[derive(Debug, Clone)]
pub(crate) struct EntryHistory {
    pub(crate) about: String,
    pub(crate) reference: String,
    revisions: Vec<EntryRevision>,
}

impl EntryHistory {
    pub(crate) fn new(about: String, reference: String, first: EntryRevision) -> Self {
        Self {
            about,
            reference,
            revisions: vec![first],
        }
    }

    pub(crate) fn record(&mut self, revision: EntryRevision) {
        self.revisions.push(revision);
    }

    /// What the entry is now.
    pub(crate) fn latest(&self) -> &EntryRevision {
        self.revisions.last().expect("a history has a first write")
    }

    /// Whether the text was written after the summary was.
    ///
    /// Deterministic and derived only from the log: the last revision that
    /// changed the summary, against the last revision that changed the text.
    /// A text that moved later describes a memory the summary never saw. An
    /// entry whose summary was attached by the same write that wrote its
    /// text is never stale, however many times it was rewritten since with
    /// both moving together.
    pub(crate) fn text_outlived_its_summary(&self) -> bool {
        if self.latest().summary.is_none() {
            return false;
        }
        self.last_change(|before, after| before.text != after.text)
            > self.last_change(|before, after| before.summary != after.summary)
    }

    /// The position of the last revision that moved the field `differs`
    /// reads. The first write always counts as a change, so a history that
    /// never moved answers with its own beginning.
    fn last_change(&self, differs: impl Fn(&EntryRevision, &EntryRevision) -> bool) -> usize {
        self.revisions
            .iter()
            .enumerate()
            .rev()
            .find(|(index, revision)| match index.checked_sub(1) {
                None => true,
                Some(previous) => differs(&self.revisions[previous], revision),
            })
            .map(|(index, _)| index)
            .unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn revision(text: &str, summary: Option<&str>) -> EntryRevision {
        EntryRevision {
            kind: "decision".to_string(),
            text: text.to_string(),
            summary: summary.map(str::to_string),
            summary_by: None,
        }
    }

    #[test]
    fn the_latest_write_decides_what_the_entry_is() {
        let mut history = EntryHistory::new(
            "project:a".to_string(),
            "project:a:e1".to_string(),
            revision("first", None),
        );
        history.record(revision("second", Some("a later rendering")));

        assert_eq!(history.latest().text, "second");
        assert_eq!(
            history.latest().summary.as_deref(),
            Some("a later rendering")
        );
    }

    #[test]
    fn a_text_rewritten_after_its_summary_leaves_the_summary_behind() {
        let mut history = EntryHistory::new(
            "project:a".to_string(),
            "project:a:e1".to_string(),
            revision("La válvula se congeló.", None),
        );
        history.record(revision(
            "La válvula se congeló.",
            Some("The valve froze during the night shift."),
        ));
        history.record(revision(
            "La válvula de reserva se bloqueó.",
            Some("The valve froze during the night shift."),
        ));

        assert!(history.text_outlived_its_summary());
    }

    #[test]
    fn a_summary_attached_after_the_last_text_change_is_not_stale() {
        let mut history = EntryHistory::new(
            "project:a".to_string(),
            "project:a:e1".to_string(),
            revision("La válvula se congeló.", None),
        );
        history.record(revision("La válvula de reserva se bloqueó.", None));
        history.record(revision(
            "La válvula de reserva se bloqueó.",
            Some("The reserve valve jammed."),
        ));

        assert!(!history.text_outlived_its_summary());
    }

    #[test]
    fn an_entry_without_a_summary_is_never_stale() {
        let history = EntryHistory::new(
            "project:a".to_string(),
            "project:a:e1".to_string(),
            revision("La válvula se congeló.", None),
        );

        assert!(!history.text_outlived_its_summary());
    }
}
