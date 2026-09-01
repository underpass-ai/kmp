//! The stretch of time a focus frames.

use std::cmp::Ordering;

use crate::view::domain::timestamp::Timestamp;
use crate::view::domain::view_error::ViewError;

/// A time window on the loom's axis. Either end may be open; a window with
/// both ends is only constructed when it ends after it begins, so a
/// backwards or zero-width frame never becomes state.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FocusWindow {
    from: Option<Timestamp>,
    to: Option<Timestamp>,
}

impl FocusWindow {
    /// Builds a window, refusing one that ends before it begins and one
    /// whose ends the kernel cannot read as instants.
    pub fn new(from: Option<Timestamp>, to: Option<Timestamp>) -> Result<Self, ViewError> {
        if let (Some(from), Some(to)) = (from.as_ref(), to.as_ref()) {
            match from.compare(to) {
                Some(Ordering::Less) => {}
                Some(Ordering::Equal | Ordering::Greater) => {
                    return Err(ViewError::Invalid(
                        "the loom does not frame a window that ends before it begins; \
                         `from` must be before `to`"
                            .to_string(),
                    ));
                }
                None => {
                    return Err(ViewError::Invalid(
                        "a focus window needs RFC3339 or persisted `unix:` timestamps".to_string(),
                    ));
                }
            }
        }
        Ok(Self { from, to })
    }

    /// Where the window opens, when it names a start.
    pub fn from(&self) -> Option<&Timestamp> {
        self.from.as_ref()
    }

    /// Where the window closes, when it names an end.
    pub fn to(&self) -> Option<&Timestamp> {
        self.to.as_ref()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_backwards_or_zero_width_focus_window_is_refused() {
        for (from, to) in [
            ("2026-08-28T00:00:00Z", "2026-08-27T00:00:00Z"),
            ("2026-08-27T00:00:00Z", "2026-08-27T00:00:00Z"),
            ("2026-08-27T02:00:00+02:00", "2026-08-27T00:00:00Z"),
        ] {
            let refused = FocusWindow::new(Some(Timestamp::new(from)), Some(Timestamp::new(to)));
            assert!(
                matches!(refused, Err(ViewError::Invalid(_))),
                "{from} -> {to}"
            );
        }
    }

    #[test]
    fn an_open_ended_window_is_a_window() {
        let open = FocusWindow::new(Some(Timestamp::new("2026-08-27T00:00:00Z")), None)
            .expect("an open end frames forward");
        assert!(open.to().is_none());
        let unreadable = FocusWindow::new(
            Some(Timestamp::new("yesterdayish")),
            Some(Timestamp::new("2026-08-27T00:00:00Z")),
        );
        assert!(matches!(unreadable, Err(ViewError::Invalid(_))));
    }
}
