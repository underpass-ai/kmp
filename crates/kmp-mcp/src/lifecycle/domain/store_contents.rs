use super::about_event_count::AboutEventCount;

/// What is inside one store, read without opening it for use.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StoreContents {
    /// The abouts this engine could read, most events first, and when the
    /// newest event was written (UTC, `YYYY-MM-DD HH:MM:SS`).
    Read {
        abouts: Vec<AboutEventCount>,
        last_write: Option<String>,
    },
    /// This engine cannot read it; the reason says why in a person's words.
    /// Nothing was migrated or written to find that out.
    Unreadable { reason: String },
}

impl StoreContents {
    /// The abouts in the order a person scans them: most events first, then
    /// by name, so two runs over the same store print the same list.
    pub fn read(mut abouts: Vec<AboutEventCount>, last_write: Option<String>) -> Self {
        abouts.sort_by(|left, right| {
            right
                .events()
                .cmp(&left.events())
                .then_with(|| left.about().cmp(right.about()))
        });
        Self::Read { abouts, last_write }
    }

    pub fn unreadable(reason: impl Into<String>) -> Self {
        Self::Unreadable {
            reason: reason.into(),
        }
    }

    /// Every event in the store, across abouts.
    pub fn total_events(&self) -> u64 {
        match self {
            Self::Read { abouts, .. } => abouts.iter().map(AboutEventCount::events).sum(),
            Self::Unreadable { .. } => 0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn abouts_read_most_events_first_and_ties_by_name() {
        let contents = StoreContents::read(
            vec![
                AboutEventCount::new("b", 3),
                AboutEventCount::new("c", 9),
                AboutEventCount::new("a", 3),
            ],
            None,
        );
        let StoreContents::Read { abouts, .. } = &contents else {
            panic!("read");
        };
        let order: Vec<_> = abouts.iter().map(AboutEventCount::about).collect();
        assert_eq!(order, ["c", "a", "b"]);
        assert_eq!(contents.total_events(), 15);
    }
}
