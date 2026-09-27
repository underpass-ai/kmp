use kmp_proto::v1beta1::MemoryEvidence;

use super::answer_candidate_terms::{surface_counts, text_counts};
use super::lexical_profile::LexicalProfile;
use super::lexical_term::LexicalTerm;
use super::term_counts::TermCounts;
use super::varint::{Reader, push_bytes, push_signed, push_unsigned};

/// One candidate's row in the lexical sidecar (DESIGN L6 `LexFwd`): every
/// term it carries with its count in each part of its surface
/// ([`LexicalTerm`]), and the length of each part, under both readings an ask
/// can take (plain, and with the alias terms the anchored gate reads).
///
/// The ranker depends on nothing else about a candidate: BM25 reads tf and
/// the length in the content and the direct field, and both are sums of the
/// parts. Equal rows are therefore equal scores.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct LexicalRow {
    terms: Vec<LexicalTerm>,
    text_length: i64,
    summary_length: i64,
    extra_length: i64,
    alias_content_length: i64,
    alias_extra_length: i64,
}

/// FNV-1a, 64 bits: a fingerprint that must not move between processes or
/// releases, which the standard hasher does not promise.
const FNV_OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
const FNV_PRIME: u64 = 0x0000_0100_0000_01b3;

/// The layout of an encoded row; a different one is refused.
const ROW_LAYOUT: u64 = 2;

/// A candidate's fields as the ranker counts them: content and direct.
type Fields<'a> = (&'a TermCounts, &'a TermCounts);

impl LexicalRow {
    /// Reads one candidate the way the ranker reads it, in `profile`'s
    /// language, under both readings.
    pub fn read(item: &MemoryEvidence, profile: &LexicalProfile) -> Self {
        let morphology = profile.morphology();
        let (content, direct, _) = surface_counts(item, &morphology, false);
        let (aliased_content, aliased_direct, _) = surface_counts(item, &morphology, true);
        let text = text_counts(item, &morphology, false);
        Self::from_counts(
            &text,
            (&content, &direct),
            (&aliased_content, &aliased_direct),
        )
    }

    pub(super) fn from_counts(text: &TermCounts, plain: Fields<'_>, aliased: Fields<'_>) -> Self {
        let (content, direct) = plain;
        let (aliased_content, aliased_direct) = aliased;
        let mut names = text
            .terms()
            .chain(content.terms())
            .chain(direct.terms())
            .chain(aliased_content.terms())
            .chain(aliased_direct.terms())
            .collect::<Vec<_>>();
        names.sort();
        names.dedup();
        let terms = names
            .into_iter()
            .map(|term| {
                let count = |counts: &TermCounts| i64::from(counts.count(term));
                let (in_text, in_content, in_direct) = (count(text), count(content), count(direct));
                let (in_aliased_content, in_aliased_direct) =
                    (count(aliased_content), count(aliased_direct));
                LexicalTerm {
                    term: term.clone(),
                    text: in_text,
                    summary: in_content - in_text,
                    extra: in_direct - in_content,
                    alias_content: in_aliased_content - in_content,
                    alias_extra: (in_aliased_direct - in_aliased_content)
                        - (in_direct - in_content),
                }
            })
            .collect();
        let length = |counts: &TermCounts| counts.length() as i64;
        Self {
            terms,
            text_length: length(text),
            summary_length: length(content) - length(text),
            extra_length: length(direct) - length(content),
            alias_content_length: length(aliased_content) - length(content),
            alias_extra_length: (length(aliased_direct) - length(aliased_content))
                - (length(direct) - length(content)),
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

    /// The part lengths: text, summary, extra, alias content, alias extra.
    pub fn lengths(&self) -> [i64; 5] {
        [
            self.text_length,
            self.summary_length,
            self.extra_length,
            self.alias_content_length,
            self.alias_extra_length,
        ]
    }

    pub fn content_length(&self, aliased: bool) -> i64 {
        self.text_length
            + self.summary_length
            + if aliased {
                self.alias_content_length
            } else {
                0
            }
    }

    pub fn direct_length(&self, aliased: bool) -> i64 {
        self.content_length(aliased)
            + self.extra_length
            + if aliased { self.alias_extra_length } else { 0 }
    }

    /// The two fields BM25 reads under a reading, term by term, and their
    /// lengths, folded into 64 bits. The ranker computes the same fingerprint
    /// from its own counts ([`Self::fingerprint_of_counts`]), so equal
    /// fingerprints mean the index and the ranker read this candidate alike.
    pub fn fingerprint(&self, aliased: bool) -> u64 {
        fingerprint(
            self.terms
                .iter()
                .filter(|term| term.is_searchable(aliased))
                .map(|term| {
                    (
                        term.term.as_str(),
                        term.content(aliased),
                        term.direct(aliased),
                    )
                }),
            self.content_length(aliased),
            self.direct_length(aliased),
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
        let mut buffer = Vec::with_capacity(16 + self.terms.len() * 14);
        push_unsigned(&mut buffer, ROW_LAYOUT);
        for length in self.lengths() {
            push_signed(&mut buffer, length);
        }
        push_unsigned(&mut buffer, self.terms.len() as u64);
        for term in &self.terms {
            push_bytes(&mut buffer, term.term.as_bytes());
            for count in [
                term.text,
                term.summary,
                term.extra,
                term.alias_content,
                term.alias_extra,
            ] {
                push_signed(&mut buffer, count);
            }
        }
        buffer
    }

    pub fn decode(bytes: &[u8]) -> Result<Self, String> {
        let mut reader = Reader::new(bytes);
        let layout = reader.unsigned()?;
        if layout != ROW_LAYOUT {
            return Err(format!("lexical row layout {layout} is not {ROW_LAYOUT}"));
        }
        let mut lengths = [0i64; 5];
        for length in &mut lengths {
            *length = reader.signed()?;
        }
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
                alias_content: reader.signed()?,
                alias_extra: reader.signed()?,
            });
        }
        if !reader.finished() {
            return Err("trailing bytes after a lexical row".into());
        }
        let [
            text_length,
            summary_length,
            extra_length,
            alias_content_length,
            alias_extra_length,
        ] = lengths;
        Ok(Self {
            terms,
            text_length,
            summary_length,
            extra_length,
            alias_content_length,
            alias_extra_length,
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
        let content = counts(&["valve", "froze", "valve", "night"]);
        let direct = counts(&["valve", "froze", "valve", "night", "operator", "log"]);
        let aliased_content = counts(&["valve", "froze", "valve", "night", "c10"]);
        let aliased_direct = counts(&[
            "valve", "froze", "valve", "night", "c10", "operator", "log", "adr18",
        ]);
        LexicalRow::from_counts(
            &counts(&["valve", "froze"]),
            (&content, &direct),
            (&aliased_content, &aliased_direct),
        )
    }

    #[test]
    fn the_fields_are_exact_sums_of_the_parts_under_both_readings() {
        let row = row();
        let valve = row.term("valve").expect("fixture");
        assert_eq!((valve.text, valve.summary, valve.extra), (1, 1, 0));
        assert_eq!((valve.content(false), valve.direct(false)), (2, 2));
        let log = row.term("log").expect("fixture");
        assert_eq!((log.content(false), log.direct(false)), (0, 1));
        let alias = row.term("c10").expect("fixture");
        assert_eq!((alias.content(false), alias.direct(false)), (0, 0));
        assert_eq!((alias.content(true), alias.direct(true)), (1, 1));
        assert!(!alias.is_searchable(false) && alias.is_held());
        let spelled = row.term("adr18").expect("fixture");
        assert_eq!((spelled.content(true), spelled.direct(true)), (0, 1));
        assert_eq!(
            (row.content_length(false), row.direct_length(false)),
            (4, 6)
        );
        assert_eq!((row.content_length(true), row.direct_length(true)), (5, 8));
    }

    #[test]
    fn a_row_round_trips_through_its_bytes() {
        let row = row();
        assert_eq!(LexicalRow::decode(&row.encode()).expect("fixture"), row);
        assert!(LexicalRow::decode(&row.encode()[..3]).is_err());
    }

    #[test]
    fn the_index_and_the_ranker_fingerprint_alike_under_each_reading() {
        let row = row();
        let plain = LexicalRow::fingerprint_of_counts(
            &counts(&["valve", "froze", "valve", "night"]),
            &counts(&["valve", "froze", "valve", "night", "operator", "log"]),
        );
        let aliased = LexicalRow::fingerprint_of_counts(
            &counts(&["valve", "froze", "valve", "night", "c10"]),
            &counts(&[
                "valve", "froze", "valve", "night", "c10", "operator", "log", "adr18",
            ]),
        );
        assert_eq!(row.fingerprint(false), plain);
        assert_eq!(row.fingerprint(true), aliased);
        assert_ne!(plain, aliased);
    }

    /// A seam the tokenizer read differently in the text alone than in the
    /// content still leaves both fields exact: the parts absorb it.
    #[test]
    fn a_term_only_the_text_reading_saw_does_not_change_the_fields() {
        let valve = counts(&["valve"]);
        let row = LexicalRow::from_counts(
            &counts(&["c10", "valve"]),
            (&valve, &valve),
            (&valve, &valve),
        );
        let seam = row.term("c10").expect("fixture");
        assert_eq!((seam.content(false), seam.direct(false)), (0, 0));
        assert!(!seam.is_held());
        assert_eq!(
            row.fingerprint(false),
            LexicalRow::fingerprint_of_counts(&valve, &valve)
        );
    }
}
