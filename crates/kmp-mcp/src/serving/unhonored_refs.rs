//! The refs a view intent named that this store does not hold.
//!
//! The projections in an intent already degraded: a dimension or an overlay
//! this build cannot draw is omitted and named through `unhonored`. Its refs
//! did not — one absent ref collapsed the whole call — and
//! [#443](https://github.com/underpass-ai/kmp/issues/443) settled that
//! asymmetry for `kmp_view_apply_intent`: apply what applies, report the rest
//! through the same channel. This is where the intent degrades.
//!
//! `kmp_view_open` deliberately kept the opposite answer to the same kind of
//! absence; the reason is written down on
//! [`crate::serving::view_tools::open`].

use kmp_viewer::ViewIntentDto;

/// What one view intent pointed at that is not in this store.
#[derive(Debug, Default)]
pub(crate) struct UnhonoredRefs {
    refs: Vec<String>,
}

impl UnhonoredRefs {
    /// The refs a store read could not find, in the order the intent named
    /// them.
    pub(crate) fn new(refs: Vec<String>) -> Self {
        Self { refs }
    }

    /// The refs themselves, for scoping the reads that follow.
    pub(crate) fn as_slice(&self) -> &[String] {
        &self.refs
    }

    fn contains(&self, reference: &str) -> bool {
        self.refs.iter().any(|missing| missing == reference)
    }

    /// Strips what this store does not hold out of `intent`, and says in the
    /// caller's words what could not be honored.
    ///
    /// Every facet degrades toward the state the view already has, never
    /// toward emptiness: an absent `target.about` leaves the loom on the
    /// about it was weaving, an absent selection or trace end leaves the
    /// selection and the trace untouched, and a focus whose every ref is
    /// absent keeps the refs it had and moves only its window — the rule the
    /// dimensions keep-list already follows when every requested name was a
    /// typo.
    ///
    /// When *every* ref the intent named is absent, none of it is applied:
    /// the intent is emptied, so the aggregate answers `applied: false` over
    /// the unchanged state, and the first note says exactly that. Honoring a
    /// call's clock, window or zoom rung while honoring none of its subjects
    /// would move the loom to a frame nobody asked for — the empty view that
    /// looks like an answer, reached through a different door.
    pub(crate) fn omit_from(&self, intent: &mut ViewIntentDto) -> Vec<String> {
        let named = named_refs(intent);
        if self.refs.is_empty() || named.is_empty() {
            return Vec::new();
        }
        let mut notes = Vec::new();

        let absent_about = intent.about.clone().filter(|about| self.contains(about));
        if let Some(about) = absent_about {
            notes.push(format!(
                "`{about}` is not in this store; the loom stays on the about it was weaving"
            ));
            intent.about = None;
        }

        let mut emptied_focus = None;
        let mut dropped_refs = Vec::new();
        if let Some(focus) = intent.focus.as_mut() {
            dropped_refs = drop_absent(&mut focus.refs, self);
            if focus.refs.is_empty() && !dropped_refs.is_empty() {
                emptied_focus = Some(focus.time_range.take());
            }
        }
        match emptied_focus {
            Some(window) => {
                for reference in &dropped_refs {
                    notes.push(format!(
                        "`{reference}` is not in this store; focus.refs named nothing this store \
                         holds, so the focus keeps the refs it had"
                    ));
                }
                intent.focus_window = window;
                intent.focus = None;
            }
            None => {
                for reference in &dropped_refs {
                    notes.push(format!(
                        "`{reference}` is not in this store; it is dropped from focus.refs"
                    ));
                }
            }
        }

        let absent_selection = intent
            .selection
            .as_ref()
            .and_then(Option::as_ref)
            .filter(|selection| self.contains(selection))
            .cloned();
        if let Some(selection) = absent_selection {
            notes.push(format!(
                "`{selection}` is not in this store; the selection is unchanged"
            ));
            intent.selection = None;
        }

        let absent_ends = match intent.trace.as_ref().and_then(Option::as_ref) {
            Some(trace) => dedup([trace.from.clone(), trace.to.clone()])
                .into_iter()
                .filter(|end| self.contains(end))
                .collect::<Vec<_>>(),
            None => Vec::new(),
        };
        if !absent_ends.is_empty() {
            for end in absent_ends {
                notes.push(format!(
                    "`{end}` is not in this store; a trace needs both ends, so the trace is \
                     unchanged"
                ));
            }
            intent.trace = None;
        }

        if let Some(abouts) = intent
            .projection
            .as_mut()
            .and_then(|projection| projection.abouts.as_mut())
        {
            for about in drop_absent(abouts, self) {
                notes.push(format!(
                    "`{about}` is not in this store; it is dropped from projection.abouts"
                ));
            }
        }

        if named.iter().all(|reference| self.contains(reference)) {
            *intent = ViewIntentDto::default();
            notes.insert(
                0,
                "no part of this intent was honored: every ref it names is absent from this \
                 store, so the view did not move and its state is unchanged"
                    .to_string(),
            );
        }
        notes
    }
}

/// Removes the refs this store does not hold, and returns them in the order
/// they were named.
fn drop_absent(refs: &mut Vec<String>, missing: &UnhonoredRefs) -> Vec<String> {
    let dropped = refs
        .iter()
        .filter(|reference| missing.contains(reference))
        .cloned()
        .collect::<Vec<_>>();
    refs.retain(|reference| !missing.contains(reference));
    dropped
}

/// Every ref the intent points at, once, in the order it names them.
fn named_refs(intent: &ViewIntentDto) -> Vec<String> {
    let mut named = Vec::new();
    if let Some(about) = intent.about.as_ref() {
        remember(&mut named, about);
    }
    if let Some(focus) = intent.focus.as_ref() {
        for reference in &focus.refs {
            remember(&mut named, reference);
        }
    }
    if let Some(selection) = intent.selection.as_ref().and_then(Option::as_ref) {
        remember(&mut named, selection);
    }
    if let Some(trace) = intent.trace.as_ref().and_then(Option::as_ref) {
        remember(&mut named, &trace.from);
        remember(&mut named, &trace.to);
    }
    if let Some(abouts) = intent
        .projection
        .as_ref()
        .and_then(|projection| projection.abouts.as_ref())
    {
        for about in abouts {
            remember(&mut named, about);
        }
    }
    named
}

fn remember(named: &mut Vec<String>, reference: &str) {
    if !named.iter().any(|seen| seen == reference) {
        named.push(reference.to_string());
    }
}

fn dedup<const N: usize>(refs: [String; N]) -> Vec<String> {
    let mut kept = Vec::with_capacity(N);
    for reference in refs {
        if !kept.contains(&reference) {
            kept.push(reference);
        }
    }
    kept
}
