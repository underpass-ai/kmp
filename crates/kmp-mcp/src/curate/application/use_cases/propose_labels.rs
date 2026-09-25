use crate::curate::application::curate_material::CurateMaterial;
use crate::curate::application::jev_usage::JevUsage;
use crate::curate::application::label_plan::label_request;
use crate::curate::domain::curate_thresholds::{NONE, PARTNER_AT};
use crate::curate::domain::proposed_label::ProposedLabel;
use crate::serving::judgement_answer::JudgementAnswer;
use crate::serving::ports::judgement_model::JudgementModel;

/// Label memberships the judge would add to a few facts, chosen from what
/// their about already uses. Writes nothing: `kmp_relabel` applies them.
pub(crate) struct ProposeLabels<'a> {
    pub judgement: &'a dyn JudgementModel,
}

impl ProposeLabels<'_> {
    pub(crate) async fn run(
        &self,
        material: &CurateMaterial,
        focus: &[String],
    ) -> Result<(Vec<ProposedLabel>, JevUsage), String> {
        let mut usage = JevUsage {
            model: self.judgement.model().to_string(),
            requests: 0,
            input_tokens: 0,
        };
        let (request, keys) = label_request(material, focus);
        if keys.is_empty() {
            return Ok((Vec::new(), usage));
        }
        let response = self.judgement.evaluate(&request).await?;
        usage.requests += response.requests;
        usage.input_tokens += response.input_tokens;
        let mut proposed = keys
            .iter()
            .filter_map(
                |(question, (reference, key))| match response.answers.get(question) {
                    Some(JudgementAnswer::Choice {
                        choice, confidence, ..
                    }) if choice != NONE && *confidence >= PARTNER_AT => Some(ProposedLabel {
                        reference: reference.clone(),
                        key: key.clone(),
                        value: choice.clone(),
                        confidence: *confidence,
                    }),
                    _ => None,
                },
            )
            .collect::<Vec<_>>();
        proposed.sort_by(|left, right| right.confidence.total_cmp(&left.confidence));
        Ok((proposed, usage))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::curate::application::use_cases::scripted_judgement::Scripted;
    use crate::curate::domain::curate_fact::CurateFact;
    use std::sync::Mutex;

    fn fact(reference: &str, labels: &[(&str, &str)]) -> CurateFact {
        CurateFact {
            reference: reference.into(),
            about: "a".into(),
            text: format!("text {reference}"),
            occurred: None,
            labels: labels
                .iter()
                .map(|(key, value)| ((*key).into(), (*value).into()))
                .collect(),
        }
    }

    fn material() -> CurateMaterial {
        CurateMaterial {
            facts: vec![
                fact("db", &[("journal", "j"), ("component", "database")]),
                fact("ci", &[("journal", "j"), ("component", "ci")]),
                fact("new", &[("journal", "j")]),
            ],
            declared: Vec::new(),
            pairs: Vec::new(),
            selection: "fp".into(),
            past: Vec::new(),
        }
    }

    #[tokio::test]
    async fn a_fact_is_offered_the_values_its_about_uses_for_keys_it_lacks() {
        let model = Scripted {
            noul: 0.9,
            choice: "database",
            confidence: 0.8,
            calls: Mutex::new(0),
        };
        let (proposed, usage) = ProposeLabels { judgement: &model }
            .run(&material(), &["new".to_string()])
            .await
            .expect("judged");
        assert_eq!(proposed.len(), 1, "journal has one value and is skipped");
        assert_eq!(
            (proposed[0].key.as_str(), proposed[0].value.as_str()),
            ("component", "database")
        );
        assert_eq!(usage.requests, 1);
        let (none, _) = ProposeLabels { judgement: &model }
            .run(&material(), &["db".to_string()])
            .await
            .expect("judged");
        assert!(none.is_empty(), "db already has a component");
    }
}
