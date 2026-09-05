/// A host that is holding a piece right now, and therefore the reason it
/// cannot be removed yet.
///
/// This is not ownership. A held piece is ours and will be removable the
/// moment its holder lets go; what it needs is a restart, not a decision.
/// Uninstall never ends the process itself — killing someone's editor to
/// tidy a cache is not a trade this verb is authorised to make — so the only
/// useful thing it can do is name who to restart, early enough that the
/// reader learns it from the dry run rather than from a half-finished apply.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PieceHold {
    /// The host as a person would name it: `claude`, `codex`.
    host: String,
    /// The process holding it, so the reader can find it in `ps` and confirm
    /// it is the window they are looking at.
    pid: u32,
}

impl PieceHold {
    pub fn new(host: impl Into<String>, pid: u32) -> Self {
        Self {
            host: host.into(),
            pid,
        }
    }

    pub fn host(&self) -> &str {
        &self.host
    }

    pub fn pid(&self) -> u32 {
        self.pid
    }

    /// Why this piece stays, in the words the report prints.
    ///
    /// It names the release the host is still serving from here, because the
    /// surprising part is never the lock: it is that an updated installation
    /// still has an old engine answering, in a session that started before
    /// the update and kept the process it already had.
    pub fn reason(&self) -> String {
        format!(
            "{} (pid {}) is still using it; restart that host, then remove it",
            self.host, self.pid
        )
    }
}

#[cfg(test)]
mod tests {
    use super::PieceHold;

    #[test]
    fn a_hold_names_the_host_to_restart_and_the_process_to_find_it_by() {
        let hold = PieceHold::new("claude", 868_141);
        assert_eq!(
            hold.reason(),
            "claude (pid 868141) is still using it; restart that host, then remove it"
        );
    }

    #[test]
    fn a_hold_keeps_its_parts_readable_for_a_caller_that_formats_its_own() {
        let hold = PieceHold::new("codex", 42);
        assert_eq!(hold.host(), "codex");
        assert_eq!(hold.pid(), 42);
    }
}
