//! The memory a returned citation stands for.
//!
//! A `kmp_ask` response addresses what it cites through envelopes and
//! evidence nodes; a reader judges the memory, not the address it arrived
//! under. [`normalize`] removes two layers:
//!
//! 1. the envelope: one leading `entry:`, otherwise one leading `detail:`,
//!    otherwise nothing — exactly one, so `entry:detail:x` is `detail:x`;
//! 2. the evidence node: a remainder starting with `evidence:` is evidence of
//!    the entry written after that prefix and cites it. The node's suffix goes
//!    too: `:current`, or `:relation:<n>` with `<n>` decimal digits or 16
//!    lowercase hex digits. Guide evidence carries no suffix.
//!
//! Once normalized, an entry and its evidence node are the same memory, and a
//! response often returns both. [`retrieved`] keeps each memory where the
//! reader first meets it: counted twice, one hit would earn nDCG twice and
//! push it above 1.
//!
//! `scripts/performance/memory_bench/domain/refs.py` ports this line for line
//! and `judged/metric_parity.json` (`refs`) pins both to one table.

const ENTRY_PREFIX: &str = "entry:";
const DETAIL_PREFIX: &str = "detail:";
const EVIDENCE_PREFIX: &str = "evidence:";
const CURRENT_SUFFIX: &str = ":current";
const RELATION_MARK: &str = ":relation:";

/// The entry ref a returned `proof.evidence[].id` or `because[].ref` cites.
pub fn normalize(value: &str) -> String {
    let node = value
        .strip_prefix(ENTRY_PREFIX)
        .or_else(|| value.strip_prefix(DETAIL_PREFIX))
        .unwrap_or(value);
    match evidence_subject(node) {
        Some(subject) if !subject.is_empty() => subject.to_string(),
        _ => node.to_string(),
    }
}

/// The memories `proof.evidence[].id` returns, normalized, each at its first
/// occurrence in response order.
pub fn retrieved<'a>(ids: impl IntoIterator<Item = &'a str>) -> Vec<String> {
    let mut seen = std::collections::BTreeSet::new();
    ids.into_iter()
        .map(normalize)
        .filter(|memory| seen.insert(memory.clone()))
        .collect()
}

/// The entry an `evidence:` node is evidence of, or `None` for any other ref.
fn evidence_subject(node: &str) -> Option<&str> {
    let subject = node.strip_prefix(EVIDENCE_PREFIX)?;
    if let Some(entry) = subject.strip_suffix(CURRENT_SUFFIX) {
        return Some(entry);
    }
    match subject.rsplit_once(RELATION_MARK) {
        Some((entry, ordinal)) if is_relation_ordinal(ordinal) => Some(entry),
        _ => Some(subject),
    }
}

fn is_relation_ordinal(ordinal: &str) -> bool {
    let digits = !ordinal.is_empty() && ordinal.chars().all(|c| c.is_ascii_digit());
    let hash = ordinal.len() == 16 && ordinal.chars().all(|c| matches!(c, '0'..='9' | 'a'..='f'));
    digits || hash
}

#[cfg(test)]
mod tests {
    use super::{normalize, retrieved};

    #[test]
    fn a_memory_is_retrieved_where_it_is_first_met() {
        assert_eq!(
            retrieved([
                "entry:p:x:entry:decision:a",
                "detail:evidence:p:x:entry:decision:b:current",
                "detail:evidence:p:x:entry:decision:a:current",
                "entry:p:x:entry:decision:b",
            ]),
            ["p:x:entry:decision:a", "p:x:entry:decision:b"]
        );
    }

    #[test]
    fn envelopes_and_evidence_nodes_cite_their_entry() {
        let entry = "project:x:entry:decision:d";
        for returned in [
            "entry:project:x:entry:decision:d",
            "detail:evidence:project:x:entry:decision:d:current",
            "detail:evidence:project:x:entry:decision:d:relation:1",
            "detail:evidence:project:x:entry:decision:d:relation:d115992883596a55",
            "evidence:project:x:entry:decision:d:current",
            entry,
        ] {
            assert_eq!(normalize(returned), entry, "{returned}");
        }
        assert_eq!(
            normalize("detail:evidence:guide:kmp-agent:verb:time"),
            "guide:kmp-agent:verb:time"
        );
        assert_eq!(normalize("entry:detail:x"), "detail:x");
        assert_eq!(
            normalize("evidence:x:relation:D115992883596A55"),
            "x:relation:D115992883596A55"
        );
        assert_eq!(normalize("detail:evidence::current"), "evidence::current");
    }
}
