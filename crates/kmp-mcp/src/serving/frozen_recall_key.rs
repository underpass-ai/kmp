use kmp_application::memory::{AskMemoryQuery, WakeMemoryQuery};

/// What a recall read depended on before its page was cut: the kernel query
/// alone. The byte budget, the cursor and `repeat_core` only cut the page,
/// and every page is cut again from the frozen read, so none of them is part
/// of the key. The remote channels (wake focus, semantic, rerank) are
/// configured per process and already key their own frozen selections by
/// this same query and the evidence it read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum FrozenRecallKey {
    Wake(WakeMemoryQuery),
    Ask(AskMemoryQuery),
}

impl FrozenRecallKey {
    /// A bounded estimate of the heap the key holds, for the cache budget.
    pub(crate) fn approximate_bytes(&self) -> usize {
        // The queries hold a handful of strings and small selections; their
        // debug rendering is a faithful upper bound of what they own.
        format!("{self:?}").len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use kmp_proto::v1beta1::{AskRequest, WakeRequest};
    use kmp_proto_mapping::v1beta1::{ask_query_from_proto, wake_query_from_proto};

    #[test]
    fn keys_of_different_verbs_or_queries_never_match() {
        let wake = |about: &str| {
            FrozenRecallKey::Wake(
                wake_query_from_proto(WakeRequest {
                    about: about.into(),
                    ..WakeRequest::default()
                })
                .expect("wake query"),
            )
        };
        let ask = FrozenRecallKey::Ask(
            ask_query_from_proto(AskRequest {
                about: "project:a".into(),
                question: "why?".into(),
                ..AskRequest::default()
            })
            .expect("ask query"),
        );
        assert_eq!(wake("project:a"), wake("project:a"));
        assert_ne!(wake("project:a"), wake("project:b"));
        assert_ne!(wake("project:a"), ask);
        assert!(wake("project:a").approximate_bytes() >= "project:a".len());
    }
}
