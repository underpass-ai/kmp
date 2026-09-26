use serde_json::Value;
use sha2::{Digest, Sha256};

use super::judgement_question::JudgementQuestion;
use super::verdict_state_digest::StateDigest;
use super::verdict_template::VerdictTemplate;

/// Where one verdict lives in the book (DESIGN L4 4a): the template prefix,
/// then sha256('kmp.jev.v1' ‖ pinned model ‖ template id and version ‖
/// question type ‖ canonical instructions ‖ sorted options ‖ sha256 of each
/// text of the state). Instructions are canonical JSON with keys sorted and
/// every string's whitespace collapsed, so a question reworded only in
/// spacing is the same question; the state enters by the digests of its
/// texts, never by the texts. The key holds no text.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub(crate) struct VerdictKey([u8; VerdictKey::BYTES]);

const DOMAIN: &[u8] = b"kmp.jev.v1";

impl VerdictKey {
    pub(crate) const BYTES: usize = VerdictTemplate::PREFIX_BYTES + 32;

    pub(crate) fn of(
        model: &str,
        template: VerdictTemplate,
        question: &JudgementQuestion,
        state: &StateDigest,
    ) -> Self {
        let mut hasher = Sha256::new();
        field(&mut hasher, DOMAIN);
        field(&mut hasher, model.as_bytes());
        field(&mut hasher, template.id.as_bytes());
        field(&mut hasher, &template.version.to_be_bytes());
        match question {
            JudgementQuestion::Noul { instructions } => {
                field(&mut hasher, b"noul");
                field(&mut hasher, canonical(instructions).as_bytes());
                field(&mut hasher, &0u32.to_be_bytes());
            }
            JudgementQuestion::Choice {
                instructions,
                options,
            } => {
                field(&mut hasher, b"choice");
                field(&mut hasher, canonical(instructions).as_bytes());
                let sorted = sorted_options(options);
                field(&mut hasher, &(sorted.len() as u32).to_be_bytes());
                for option in sorted {
                    field(&mut hasher, normalized(&option).as_bytes());
                }
            }
        }
        hasher.update(state.as_bytes());
        let mut key = [0u8; Self::BYTES];
        key[..VerdictTemplate::PREFIX_BYTES].copy_from_slice(&template.prefix());
        key[VerdictTemplate::PREFIX_BYTES..].copy_from_slice(&hasher.finalize());
        Self(key)
    }

    pub(crate) fn as_bytes(&self) -> &[u8] {
        &self.0
    }
}

/// Options in the order the book stores their probabilities: sorted, so a
/// question that lists the same options in another order shares a verdict.
pub(crate) fn sorted_options(options: &[String]) -> Vec<String> {
    let mut sorted = options.to_vec();
    sorted.sort();
    sorted
}

/// Length-prefixed, so no two field sequences hash the same bytes.
pub(super) fn field(hasher: &mut Sha256, bytes: &[u8]) {
    hasher.update((bytes.len() as u64).to_be_bytes());
    hasher.update(bytes);
}

/// Whitespace runs collapse to one space; ends are trimmed.
fn normalized(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// JSON with object keys sorted and strings normalized, whatever order the
/// map kept them in.
fn canonical(value: &Value) -> String {
    match value {
        Value::String(text) => Value::String(normalized(text)).to_string(),
        Value::Array(items) => format!(
            "[{}]",
            items.iter().map(canonical).collect::<Vec<_>>().join(",")
        ),
        Value::Object(map) => {
            let mut keys = map.keys().collect::<Vec<_>>();
            keys.sort();
            let fields = keys
                .into_iter()
                .map(|key| format!("{}:{}", Value::String(key.clone()), canonical(&map[key])))
                .collect::<Vec<_>>();
            format!("{{{}}}", fields.join(","))
        }
        other => other.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    const RERANK: VerdictTemplate = VerdictTemplate {
        id: "rerank",
        version: 1,
    };

    fn noul(instructions: Value) -> JudgementQuestion {
        JudgementQuestion::Noul { instructions }
    }

    fn key(model: &str, question: &JudgementQuestion, state: Value) -> VerdictKey {
        VerdictKey::of(model, RERANK, question, &StateDigest::of(&state))
    }

    #[test]
    fn spacing_and_key_order_do_not_change_the_key() {
        let a = noul(json!({"passage": "Gateway  fronts\nall", "question": "Which?"}));
        let b = noul(json!({"question": " Which? ", "passage": "Gateway fronts all"}));
        assert_eq!(key("jev-1", &a, json!("q")), key("jev-1", &b, json!("q")));
    }

    #[test]
    fn every_ingredient_changes_the_key() {
        let base = noul(json!({"passage": "p", "question": "q"}));
        let reference = key("jev-1", &base, json!("state"));
        assert_ne!(reference, key("jev-2", &base, json!("state")));
        assert_ne!(reference, key("jev-1", &base, json!("other state")));
        assert_ne!(
            reference,
            key(
                "jev-1",
                &noul(json!({"passage": "p2", "question": "q"})),
                json!("state")
            )
        );
        let other_template = VerdictKey::of(
            "jev-1",
            VerdictTemplate {
                id: "rerank",
                version: 2,
            },
            &base,
            &StateDigest::of(&json!("state")),
        );
        assert_ne!(reference, other_template);
        let choice = JudgementQuestion::Choice {
            instructions: json!({"passage": "p", "question": "q"}),
            options: vec!["a".into(), "b".into()],
        };
        assert_ne!(reference, key("jev-1", &choice, json!("state")));
    }

    #[test]
    fn option_order_does_not_matter_but_the_option_set_does() {
        let choice = |options: &[&str]| JudgementQuestion::Choice {
            instructions: json!("Which?"),
            options: options.iter().map(|o| o.to_string()).collect(),
        };
        assert_eq!(
            key("m", &choice(&["a", "b"]), json!(null)),
            key("m", &choice(&["b", "a"]), json!(null))
        );
        assert_ne!(
            key("m", &choice(&["a", "b"]), json!(null)),
            key("m", &choice(&["a", "c"]), json!(null))
        );
    }

    #[test]
    fn state_texts_are_exact_and_the_key_starts_with_its_template() {
        let question = noul(json!("q"));
        assert_ne!(
            key("m", &question, json!({"a": "x y"})),
            key("m", &question, json!({"a": "x  y"}))
        );
        assert_eq!(
            key("m", &question, json!({"a": "1", "b": "2"})),
            key("m", &question, json!({"b": "2", "a": "1"}))
        );
        let made = key("m", &question, json!("s"));
        assert_eq!(made.as_bytes().len(), VerdictKey::BYTES);
        assert!(made.as_bytes().starts_with(&RERANK.prefix()));
    }
}
