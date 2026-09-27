use kmp_proto::v1beta1::MemoryEvidence;

use super::answer_candidate_terms::{surface_counts, text_counts};
use super::lexical_profile::LexicalProfile;
use super::lexical_term::LexicalTerm;
use super::term_counts::TermCounts;
use super::varint::{Reader, push_bytes, push_signed, push_unsigned};

/// One candidate's row in the lexical sidecar (DESIGN L6 `LexFwd`): every
/// term it carries with its count in the text, the summary and the rest of
/// the direct field, and the length of each part.
///
/// The ranker depends on nothing else about a candidate: BM25 reads tf and
/// the length in the content and the direct field, and both are sums of the
/// three parts. Equal rows are therefore equal scores.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct LexicalRow {
    terms: Vec<LexicalTerm>,
    text_length: i64,
    summary_length: i64,
    extra_length: i64,
}

/// FNV-1a, 64 bits: a fingerprint that must not move between processes or
/// releases, which the standard hasher does not promise.
const FNV_OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
const FNV_PRIME: u64 = 0x0000_0100_0000_01b3;

/// The layout of an encoded row; a different one is refused.
const ROW_LAYOUT: u64 = 1;

impl LexicalRow {
    /// Reads one candidate the way the ranker reads it under `profile`.
    pub fn read(item: &MemoryEvidence, profile: &LexicalProfile) -> Self {
        let morphology = profile.morphology();
        let (content, direct, _expansion) = surface_counts(item, &morphology, profile.aliased());
        let text = text_counts(item, &morphology, profile.aliased());
        Self::from_counts(&text, &content, &direct)
    }

    pub(super) fn from_counts(
        text: &TermCounts,
        content: &TermCounts,
        direct: &TermCounts,
    ) -> Self {
        let mut names = text
            .terms()
            .chain(content.terms())
            .chain(direct.terms())
            .collect::<Vec<_>>();
        names.sort();
        names.dedup();
        let terms = names
            .into_iter()
            .map(|term| {
                let in_text = i64::from(text.count(term));
                let in_content = i64::from(content.count(term));
                LexicalTerm {
                    term: term.clone(),
                    text: in_text,
                    summary: in_content - in_text,
                    extra: i64::from(direct.count(term)) - in_content,
                }
            })
            .collect();
        let text_length = text.length() as i64;
        let content_length = content.length() as i64;
        Self {
            terms,
            text_length,
            summary_length: content_length - text_length,
            extra_length: direct.length() as i64 - content_length,
        }
    }

    /// Every term, ascending.
    pub fn terms(&self) -> &[LexicalTerm] {
        &self.terms
    }

    pub fn term(&self, term: &str) -> Option<&LexicalTerm> {
        self.terms
            .binary_search_by(|entry| entry.term.as_str().cmp(term))
            .ok()
            .map(|index| &self.terms[index])
    }

    /// The part lengths: text, summary and extra.
    pub fn lengths(&self) -> (i64, i64, i64) {
        (self.text_length, self.summary_length, self.extra_length)
    }

    pub fn content_length(&self) -> i64 {
        self.text_length + self.summary_length
    }

    pub fn direct_length(&self) -> i64 {
        self.content_length() + self.extra_length
    }

    /// The two fields BM25 reads, term by term, and their lengths, folded into
    /// 64 bits. The ranker computes the same fingerprint from its own counts
    /// ([`Self::fingerprint_of_counts`]), so equal fingerprints mean the
    /// index and the ranker read this candidate alike.
    pub fn fingerprint(&self) -> u64 {
        fingerprint(
            self.terms
                .iter()
                .filter(|term| term.is_searchable())
                .map(|term| (term.term.as_str(), term.content(), term.direct())),
            self.content_length(),
            self.direct_length(),
        )
    }

    /// The fingerprint of a candidate the ranker counted.
    pub(super) fn fingerprint_of_counts(content: &TermCounts, direct: &TermCounts) -> u64 {
        let mut names = content.terms().chain(direct.terms()).collect::<Vec<_>>();
        names.sort();
        names.dedup();
        fingerprint(
            names.into_iter().map(|term| {
                (
                    term.as_str(),
                    i64::from(content.count(term)),
                    i64::from(direct.count(term)),
                )
            }),
            content.length() as i64,
            direct.length() as i64,
        )
    }

    pub fn encode(&self) -> Vec<u8> {
        let mut buffer = Vec::with_capacity(16 + self.terms.len() * 12);
        push_unsigned(&mut buffer, ROW_LAYOUT);
        push_signed(&mut buffer, self.text_length);
        push_signed(&mut buffer, self.summary_length);
        push_signed(&mut buffer, self.extra_length);
        push_unsigned(&mut buffer, self.terms.len() as u64);
        for term in &self.terms {
            push_bytes(&mut buffer, term.term.as_bytes());
            push_signed(&mut buffer, term.text);
            push_signed(&mut buffer, term.summary);
            push_signed(&mut buffer, term.extra);
        }
        buffer
    }

    pub fn decode(bytes: &[u8]) -> Result<Self, String> {
        let mut reader = Reader::new(bytes);
        let layout = reader.unsigned()?;
        if layout != ROW_LAYOUT {
            return Err(format!("lexical row layout {layout} is not {ROW_LAYOUT}"));
        }
        let text_length = reader.signed()?;
        let summary_length = reader.signed()?;
        let extra_length = reader.signed()?;
        let count = reader.unsigned()?;
        let mut terms = Vec::with_capacity(count.min(4096) as usize);
        for _ in 0..count {
            let term = std::str::from_utf8(reader.bytes()?)
                .map_err(|error| error.to_string())?
                .to_string();
            terms.push(LexicalTerm {
                term,
                text: reader.signed()?,
                summary: reader.signed()?,
                extra: reader.signed()?,
            });
        }
        if !reader.finished() {
            return Err("trailing bytes after a lexical row".into());
        }
        Ok(Self {
            terms,
            text_length,
            summary_length,
            extra_length,
        })
    }
}

fn fingerprint<'a>(
    terms: impl Iterator<Item = (&'a str, i64, i64)>,
    content_length: i64,
    direct_length: i64,
) -> u64 {
    let mut hash = FNV_OFFSET;
    let mut feed = |bytes: &[u8]| {
        for byte in bytes {
            hash ^= u64::from(*byte);
            hash = hash.wrapping_mul(FNV_PRIME);
        }
    };
    for (term, content, direct) in terms {
        feed(term.as_bytes());
        feed(&[0xff]);
        feed(&content.to_le_bytes());
        feed(&direct.to_le_bytes());
    }
    feed(&[0xfe]);
    feed(&content_length.to_le_bytes());
    feed(&direct_length.to_le_bytes());
    hash
}

#[cfg(test)]
mod tests {
    use super::*;

    fn counts(terms: &[&str]) -> TermCounts {
        terms.iter().map(|term| (*term).to_string()).collect()
    }

    fn row() -> LexicalRow {
        LexicalRow::from_counts(
            &counts(&["valve", "froze"]),
            &counts(&["valve", "froze", "valve", "night"]),
            &counts(&["valve", "froze", "valve", "night", "operator", "log"]),
        )
    }

    #[test]
    fn the_fields_are_exact_sums_of_the_parts() {
        let row = row();
        let valve = row.term("valve").expect("fixture");
        assert_eq!((valve.text, valve.summary, valve.extra), (1, 1, 0));
        assert_eq!((valve.content(), valve.direct()), (2, 2));
        assert_eq!(row.term("log").expect("fixture").direct(), 1);
        assert_eq!(row.term("log").expect("fixture").content(), 0);
        assert_eq!(row.lengths(), (2, 2, 2));
        assert_eq!((row.content_length(), row.direct_length()), (4, 6));
    }

    #[test]
    fn a_row_round_trips_through_its_bytes() {
        let row = row();
        assert_eq!(LexicalRow::decode(&row.encode()).expect("fixture"), row);
        assert!(LexicalRow::decode(&row.encode()[..3]).is_err());
    }

    #[test]
    fn the_index_and_the_ranker_fingerprint_alike() {
        let content = counts(&["valve", "froze", "valve", "night"]);
        let direct = counts(&["valve", "froze", "valve", "night", "operator", "log"]);
        let row = LexicalRow::from_counts(&counts(&["valve"]), &content, &direct);
        assert_eq!(
            row.fingerprint(),
            LexicalRow::fingerprint_of_counts(&content, &direct)
        );
        let other = counts(&["valve", "froze", "night", "operator", "log"]);
        assert_ne!(
            row.fingerprint(),
            LexicalRow::fingerprint_of_counts(&content, &other)
        );
    }

    /// A seam the tokenizer read differently in the text alone than in the
    /// content still leaves both fields exact: the parts absorb it.
    #[test]
    fn a_term_only_the_text_reading_saw_does_not_change_the_fields() {
        let row = LexicalRow::from_counts(
            &counts(&["c10", "valve"]),
            &counts(&["valve"]),
            &counts(&["valve"]),
        );
        let seam = row.term("c10").expect("fixture");
        assert_eq!((seam.content(), seam.direct()), (0, 0));
        assert!(!seam.is_searchable());
        assert_eq!(
            row.fingerprint(),
            LexicalRow::fingerprint_of_counts(&counts(&["valve"]), &counts(&["valve"]))
        );
    }
}
