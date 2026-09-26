use std::collections::BTreeMap;

use super::judgement_answer::JudgementAnswer;
use super::judgement_question::JudgementQuestion;
use super::verdict_key::sorted_options;

const FORMAT: u8 = 1;
const NOUL: u8 = 0;
const CHOICE: u8 = 1;
const Q16: f64 = 65_535.0;

/// One judged answer as the book keeps it: probabilities quantized to Q16
/// once, when the provider's answer arrives, so every later reading — this
/// process or another, today or next month — gets the same bits. It holds
/// no text: a choice is the index of its option among the question's sorted
/// options.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct Verdict {
    answer: QuantizedAnswer,
    pub input_tokens: u32,
    pub created_at: i64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum QuantizedAnswer {
    Noul {
        yes: u16,
    },
    Choice {
        choice: u16,
        probabilities: Vec<u16>,
        confidence: u16,
    },
}

fn quantize(probability: f64) -> u16 {
    (probability.clamp(0.0, 1.0) * Q16).round() as u16
}

/// Four decimals: finer than Jev's own answers (hundredths) and coarser
/// than Q16's step, so a 0.81 reads back as 0.81 — not as
/// 0.8100022888532845 in every answer an agent reads.
fn dequantize(q: u16) -> f64 {
    (f64::from(q) / Q16 * 10_000.0).round() / 10_000.0
}

impl Verdict {
    /// The answer to `question`, quantized; `None` when it does not answer
    /// that question (another type, or a choice outside its options).
    pub(crate) fn of(
        question: &JudgementQuestion,
        answer: &JudgementAnswer,
        input_tokens: u32,
        created_at: i64,
    ) -> Option<Self> {
        let answer = match (question, answer) {
            (JudgementQuestion::Noul { .. }, JudgementAnswer::Noul { yes }) => {
                QuantizedAnswer::Noul {
                    yes: quantize(*yes),
                }
            }
            (
                JudgementQuestion::Choice { options, .. },
                JudgementAnswer::Choice {
                    choice,
                    probabilities,
                    confidence,
                },
            ) => {
                let sorted = sorted_options(options);
                let choice = sorted.iter().position(|option| option == choice)?;
                QuantizedAnswer::Choice {
                    choice: u16::try_from(choice).ok()?,
                    probabilities: sorted
                        .iter()
                        .map(|option| quantize(probabilities.get(option).copied().unwrap_or(0.0)))
                        .collect(),
                    confidence: quantize(*confidence),
                }
            }
            _ => return None,
        };
        Some(Self {
            answer,
            input_tokens,
            created_at,
        })
    }

    /// The answer this verdict gives `question`, dequantized; `None` when
    /// the verdict cannot be for it.
    pub(crate) fn answer(&self, question: &JudgementQuestion) -> Option<JudgementAnswer> {
        match (question, &self.answer) {
            (JudgementQuestion::Noul { .. }, QuantizedAnswer::Noul { yes }) => {
                Some(JudgementAnswer::Noul {
                    yes: dequantize(*yes),
                })
            }
            (
                JudgementQuestion::Choice { options, .. },
                QuantizedAnswer::Choice {
                    choice,
                    probabilities,
                    confidence,
                },
            ) => {
                let sorted = sorted_options(options);
                if sorted.len() != probabilities.len() {
                    return None;
                }
                Some(JudgementAnswer::Choice {
                    choice: sorted.get(usize::from(*choice))?.clone(),
                    probabilities: sorted
                        .into_iter()
                        .zip(probabilities.iter().map(|q| dequantize(*q)))
                        .collect::<BTreeMap<_, _>>(),
                    confidence: dequantize(*confidence),
                })
            }
            _ => None,
        }
    }

    /// Little-endian: format, kind, the quantized answer, input tokens and
    /// creation time in Unix seconds.
    pub(crate) fn encode(&self) -> Vec<u8> {
        let mut bytes = vec![FORMAT];
        match &self.answer {
            QuantizedAnswer::Noul { yes } => {
                bytes.push(NOUL);
                bytes.extend_from_slice(&yes.to_le_bytes());
            }
            QuantizedAnswer::Choice {
                choice,
                probabilities,
                confidence,
            } => {
                bytes.push(CHOICE);
                bytes.extend_from_slice(&(probabilities.len() as u16).to_le_bytes());
                bytes.extend_from_slice(&choice.to_le_bytes());
                for q in probabilities {
                    bytes.extend_from_slice(&q.to_le_bytes());
                }
                bytes.extend_from_slice(&confidence.to_le_bytes());
            }
        }
        bytes.extend_from_slice(&self.input_tokens.to_le_bytes());
        bytes.extend_from_slice(&self.created_at.to_le_bytes());
        bytes
    }

    /// `None` for bytes of another format or a truncated value.
    pub(crate) fn decode(bytes: &[u8]) -> Option<Self> {
        let mut reader = Reader(bytes);
        if reader.u8()? != FORMAT {
            return None;
        }
        let answer = match reader.u8()? {
            NOUL => QuantizedAnswer::Noul { yes: reader.u16()? },
            CHOICE => {
                let count = usize::from(reader.u16()?);
                let choice = reader.u16()?;
                let probabilities = (0..count)
                    .map(|_| reader.u16())
                    .collect::<Option<Vec<_>>>()?;
                if usize::from(choice) >= count {
                    return None;
                }
                QuantizedAnswer::Choice {
                    choice,
                    probabilities,
                    confidence: reader.u16()?,
                }
            }
            _ => return None,
        };
        let input_tokens = u32::from_le_bytes(reader.take()?);
        let created_at = i64::from_le_bytes(reader.take()?);
        reader.0.is_empty().then_some(Self {
            answer,
            input_tokens,
            created_at,
        })
    }
}

struct Reader<'a>(&'a [u8]);

impl Reader<'_> {
    fn take<const N: usize>(&mut self) -> Option<[u8; N]> {
        let (head, rest) = self.0.split_first_chunk::<N>()?;
        self.0 = rest;
        Some(*head)
    }

    fn u8(&mut self) -> Option<u8> {
        self.take::<1>().map(|[byte]| byte)
    }

    fn u16(&mut self) -> Option<u16> {
        self.take().map(u16::from_le_bytes)
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn noul() -> JudgementQuestion {
        JudgementQuestion::Noul {
            instructions: json!("q"),
        }
    }

    fn choice() -> JudgementQuestion {
        JudgementQuestion::Choice {
            instructions: json!("q"),
            options: vec!["none".into(), "b".into(), "a".into()],
        }
    }

    #[test]
    fn a_yes_no_answer_is_quantized_once_and_reads_back_stably() {
        let verdict =
            Verdict::of(&noul(), &JudgementAnswer::Noul { yes: 0.8123456 }, 7, 42).expect("fits");
        let read = verdict.answer(&noul()).expect("answers");
        let JudgementAnswer::Noul { yes } = read.clone() else {
            panic!("noul")
        };
        assert!((yes - 0.8123456).abs() < 1e-4);
        let again = Verdict::of(&noul(), &read, 7, 42).expect("fits");
        assert_eq!(
            again.answer(&noul()),
            Some(read),
            "a read answer reads back as itself"
        );
        let hundredths = Verdict::of(&noul(), &JudgementAnswer::Noul { yes: 0.81 }, 0, 0)
            .expect("fits")
            .answer(&noul());
        assert_eq!(hundredths, Some(JudgementAnswer::Noul { yes: 0.81 }));
        assert_eq!(Verdict::decode(&verdict.encode()), Some(verdict.clone()));
        assert_eq!(verdict.encode().len(), 1 + 1 + 2 + 4 + 8);
        assert_eq!(verdict.answer(&choice()), None);
    }

    #[test]
    fn a_choice_keeps_its_option_by_sorted_index_and_every_probability() {
        let answer = JudgementAnswer::Choice {
            choice: "b".into(),
            probabilities: BTreeMap::from([
                ("a".to_string(), 0.1),
                ("b".to_string(), 0.7),
                ("none".to_string(), 0.2),
            ]),
            confidence: 0.9,
        };
        let verdict = Verdict::of(&choice(), &answer, 100, 1).expect("fits");
        let decoded = Verdict::decode(&verdict.encode()).expect("decodes");
        let Some(JudgementAnswer::Choice {
            choice: chosen,
            probabilities,
            confidence,
        }) = decoded.answer(&choice())
        else {
            panic!("choice")
        };
        assert_eq!(chosen, "b");
        assert_eq!(probabilities.len(), 3);
        assert!((probabilities["b"] - 0.7).abs() < 1e-4);
        assert!((confidence - 0.9).abs() < 1e-4);
        assert_eq!(decoded.input_tokens, 100);
        let outside = JudgementAnswer::Choice {
            choice: "z".into(),
            probabilities: BTreeMap::new(),
            confidence: 1.0,
        };
        assert_eq!(Verdict::of(&choice(), &outside, 0, 0), None);
    }

    #[test]
    fn foreign_or_truncated_bytes_are_not_a_verdict() {
        let bytes = Verdict::of(&noul(), &JudgementAnswer::Noul { yes: 1.0 }, 0, 0)
            .expect("fits")
            .encode();
        assert_eq!(Verdict::decode(&bytes[..bytes.len() - 1]), None);
        let mut longer = bytes.clone();
        longer.push(0);
        assert_eq!(Verdict::decode(&longer), None);
        let mut other_format = bytes;
        other_format[0] = 9;
        assert_eq!(Verdict::decode(&other_format), None);
        assert_eq!(
            Verdict::decode(&[1, 1, 2, 0, 5, 0]),
            None,
            "choice index out of range"
        );
    }
}
