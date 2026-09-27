use crate::DomainError;

/// Work limits, independent of transport pagination and rendering tokens.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TraceSearchLimits {
    pub nodes: u32,
    pub edges: u32,
    pub depth: u32,
    pub states: u32,
}

impl TraceSearchLimits {
    pub fn validate(self) -> Result<(), DomainError> {
        if !(1..=4096).contains(&self.nodes)
            || !(1..=32768).contains(&self.edges)
            || !(1..=1024).contains(&self.depth)
            || !(1..=32768).contains(&self.states)
        {
            return Err(DomainError::InvalidState(
                "trace search requires max_nodes 1..4096, max_edges 1..32768, max_depth 1..1024 and max_states 1..32768"
                    .into(),
            ));
        }
        Ok(())
    }
}

impl TraceSearchLimits {
    /// The allowance of a single-destination trace (DESIGN L7): a bounded
    /// bidirectional search over every stored relation. Four times the
    /// bounded search's default on every limit, so a far destination in a
    /// long chain is met before a limit stops it.
    pub const fn single_destination() -> Self {
        Self {
            nodes: 1024,
            edges: 8192,
            depth: 512,
            states: 16384,
        }
    }

    /// The largest allowance [`Self::validate`] accepts, which a partial
    /// single-destination trace offers as `search.widen`.
    pub const fn widest() -> Self {
        Self {
            nodes: 4096,
            edges: 32768,
            depth: 1024,
            states: 32768,
        }
    }
}

impl Default for TraceSearchLimits {
    fn default() -> Self {
        Self {
            nodes: 256,
            edges: 2048,
            depth: 128,
            states: 4096,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_single_destination_allowance_sits_between_the_default_and_the_widest() {
        let (default, single, widest) = (
            TraceSearchLimits::default(),
            TraceSearchLimits::single_destination(),
            TraceSearchLimits::widest(),
        );
        for limits in [default, single, widest] {
            assert!(limits.validate().is_ok());
        }
        assert_eq!(single.nodes, 1024);
        assert!(default.nodes < single.nodes && single.nodes < widest.nodes);
        assert!(default.edges < single.edges && single.edges < widest.edges);
        assert!(default.depth < single.depth && single.depth < widest.depth);
        assert!(default.states < single.states && single.states < widest.states);
        assert!(
            TraceSearchLimits {
                nodes: widest.nodes + 1,
                ..widest
            }
            .validate()
            .is_err()
        );
    }
}
