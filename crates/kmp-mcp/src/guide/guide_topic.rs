//! The topics `kmp_guide` serves: one short card each, and the arguments of
//! the tools it teaches. A topic is a plain lookup; nothing is recorded about
//! who asked for it, and nothing is read from a store.

/// One row of the guide index.
pub(crate) struct GuideTopic {
    pub(crate) name: &'static str,
    pub(crate) purpose: &'static str,
    /// The worked card, compiled in from `cards/` so every install serves
    /// it, with or without a plugin tree. Those files are copies of
    /// `plugins/kmp/guide/cards/`, the editorial source of the stored guide;
    /// `tests/guide_topics.rs` keeps the two identical.
    pub(crate) text: &'static str,
    /// `(tool, argument path)` pairs whose prose this topic serves. A path
    /// covers itself and everything below it; `""` is the whole tool. An
    /// argument belongs to the topic with the longest matching path, so every
    /// documented argument is served by exactly one topic.
    pub(crate) serves: &'static [(&'static str, &'static str)],
}

/// Inspect, and the trace arguments both trace modes share: the call itself
/// and its proof and body options. The modes' own search arguments are the
/// trace topic's.
const AUDIT: [(&str, &str); 7] = [
    ("kmp_inspect", ""),
    ("kmp_trace", ""),
    ("kmp_trace", "search.proof"),
    ("kmp_trace", "search.proof_refs"),
    ("kmp_trace", "search.compact"),
    ("kmp_trace", "search.max_body_record_bytes"),
    ("kmp_trace", "search.expect_selection"),
];

const TOPICS: [GuideTopic; 14] = [
    GuideTopic {
        name: "wake",
        purpose: "Resume known work",
        text: include_str!("cards/wake.md"),
        serves: &[("kmp_wake", "")],
    },
    GuideTopic {
        name: "write",
        purpose: "Record separate facts with evidence and labels",
        text: include_str!("cards/write.md"),
        serves: &[("kmp_write_memory", "")],
    },
    GuideTopic {
        name: "relations",
        purpose: "Link memories with a relation, its why and evidence",
        text: include_str!("cards/relations.md"),
        serves: &[
            ("kmp_write_memory", "memories[].connect_to"),
            ("kmp_write_memory", "relations"),
        ],
    },
    GuideTopic {
        name: "ingest",
        purpose: "Write one canonical memory document",
        text: include_str!("cards/ingest.md"),
        serves: &[("kmp_ingest", "")],
    },
    GuideTopic {
        name: "relabel",
        purpose: "Change the labels of a stored memory",
        text: include_str!("cards/relabel.md"),
        serves: &[("kmp_relabel", "")],
    },
    GuideTopic {
        name: "ask",
        purpose: "Retrieve semantic evidence or stop at UNKNOWN",
        text: include_str!("cards/ask.md"),
        serves: &[("kmp_ask", "")],
    },
    GuideTopic {
        name: "time",
        purpose: "Navigate history with the right clock",
        text: include_str!("cards/time.md"),
        serves: &[("kmp_time", "")],
    },
    GuideTopic {
        name: "audit",
        purpose: "Inspect a claim and read its proof",
        text: include_str!("cards/audit.md"),
        serves: &AUDIT,
    },
    GuideTopic {
        name: "trace",
        purpose: "Trace a path to known destinations or seek evidence",
        text: include_str!("cards/trace.md"),
        serves: &[("kmp_trace", "search")],
    },
    GuideTopic {
        name: "condense",
        purpose: "Write a compact card or a search summary for a stored body",
        text: include_str!("cards/condense.md"),
        serves: &[
            ("kmp_condense", ""),
            ("kmp_summaries_audit", ""),
            ("kmp_write_memory", "search_summaries"),
        ],
    },
    GuideTopic {
        name: "relate",
        purpose: "Compare explicitly selected abouts",
        text: include_str!("cards/relate.md"),
        serves: &[("kmp_relate", "")],
    },
    GuideTopic {
        name: "curate",
        purpose: "Find missing and doubtful relations with Jev",
        text: include_str!("cards/curate.md"),
        serves: &[("kmp_curate", "")],
    },
    GuideTopic {
        name: "view",
        purpose: "Frame memory in ChronoLoom",
        text: include_str!("cards/view.md"),
        serves: &[
            ("kmp_view_open", ""),
            ("kmp_view_apply_intent", ""),
            ("kmp_view_get_state", ""),
        ],
    },
    GuideTopic {
        name: "guide",
        purpose: "How kmp_guide works and how to repair the stored guide",
        text: include_str!("cards/guide.md"),
        serves: &[("kmp_guide", "")],
    },
];

impl GuideTopic {
    pub(crate) fn all() -> &'static [Self] {
        &TOPICS
    }

    /// The canonical node of this card in the stored agent guide.
    pub(crate) fn card_ref(&self) -> String {
        format!("guide:kmp-agent:card:{}", self.name)
    }

    pub(crate) fn named(name: &str) -> Option<&'static Self> {
        TOPICS.iter().find(|topic| topic.name == name)
    }

    /// The topic that explains `path` of `tool`: the longest declared path
    /// that covers it. `path` may carry item indexes (`memories[0].kind`), as
    /// validation feedback does; they are read as `[]`.
    pub(crate) fn explaining(tool: &str, path: &str) -> Option<&'static Self> {
        let path = without_indexes(path);
        TOPICS
            .iter()
            .flat_map(|topic| topic.serves.iter().map(move |served| (topic, served)))
            .filter(|(_, (served_tool, prefix))| *served_tool == tool && covers(prefix, &path))
            .max_by_key(|(_, (_, prefix))| prefix.len())
            .map(|(topic, _)| topic)
    }

    /// The topic that explains `tool` as a whole.
    pub(crate) fn of_tool(tool: &str) -> Option<&'static Self> {
        Self::explaining(tool, "")
    }

    /// Whether this topic serves `path` of `tool`.
    pub(crate) fn serves_path(&self, tool: &str, path: &str) -> bool {
        Self::explaining(tool, path).is_some_and(|topic| topic.name == self.name)
    }

    /// The tools this topic serves arguments of, in declaration order.
    pub(crate) fn tools(&self) -> Vec<&'static str> {
        let mut tools = Vec::new();
        for (tool, _) in self.serves {
            if !tools.contains(tool) {
                tools.push(*tool);
            }
        }
        tools
    }
}

fn covers(prefix: &str, path: &str) -> bool {
    prefix.is_empty()
        || path == prefix
        || path
            .strip_prefix(prefix)
            .is_some_and(|rest| rest.starts_with('.') || rest.starts_with('['))
}

/// `memories[3].connect_to[0]` → `memories[].connect_to[]`.
fn without_indexes(path: &str) -> String {
    let mut out = String::with_capacity(path.len());
    let mut in_index = false;
    for character in path.chars() {
        match character {
            '[' => {
                in_index = true;
                out.push('[');
            }
            ']' => {
                in_index = false;
                out.push(']');
            }
            digit if in_index && digit.is_ascii_digit() => {}
            other => out.push(other),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_argument_belongs_to_the_longest_declared_path() {
        let topic = |tool, path| GuideTopic::explaining(tool, path).map(|t| t.name);
        assert_eq!(
            topic("kmp_write_memory", "memories[].evidence"),
            Some("write")
        );
        assert_eq!(
            topic("kmp_write_memory", "memories[2].connect_to[0].class"),
            Some("relations")
        );
        assert_eq!(topic("kmp_write_memory", "relations"), Some("relations"));
        assert_eq!(topic("kmp_write_memory", "relationsx"), Some("write"));
        assert_eq!(
            topic("kmp_write_memory", "search_summaries[].ref"),
            Some("condense")
        );
        assert_eq!(topic("kmp_trace", "search.seek[].via"), Some("trace"));
        assert_eq!(topic("kmp_trace", "search.proof_refs"), Some("audit"));
        assert_eq!(topic("kmp_trace", "budget.depth"), Some("audit"));
        assert_eq!(topic("kmp_unknown", ""), None);
        assert_eq!(
            GuideTopic::of_tool("kmp_relabel").map(|t| t.name),
            Some("relabel")
        );
        assert!(GuideTopic::named("write").is_some());
        assert!(GuideTopic::named("nope").is_none());
        let audit = GuideTopic::named("audit").expect("audit");
        assert_eq!(audit.tools(), vec!["kmp_inspect", "kmp_trace"]);
        assert!(audit.serves_path("kmp_trace", "search.compact.language"));
        assert!(!audit.serves_path("kmp_trace", "search.select"));
    }
}
