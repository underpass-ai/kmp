//! Runs the path search a view intent asked for.
//!
//! The loom has no judge of its own and never writes memory. When an intent
//! asks for paths, this asks the backend the very `kmp_curate` `mode: paths`
//! question an agent would ask, over the planes the view will draw, and the
//! view keeps the answer — declared hops, proposed hops with their frozen
//! `item_id`s, the `review_token` they belong to and the declarations the
//! audit avoided. Declaring a proposed hop stays a `kmp_curate` apply by
//! whoever writes it.

use kmp_viewer::{PathsDto, ViewIntentDto};
use serde_json::{Value, json};

use crate::serving::kernel_mcp_server::KernelMcpServer;

/// What the search answered for one intent: the paths to draw, or why none
/// could be searched. An intent that did not ask for paths searches nothing.
#[derive(Debug, Default)]
pub(crate) struct SearchedPaths {
    found: Option<PathsDto>,
    note: Option<String>,
}

impl SearchedPaths {
    /// Puts the answer where the request was. A search that could not run
    /// leaves the paths the view already had and says why, the way an absent
    /// trace end leaves the trace.
    pub(crate) fn apply_to(self, intent: &mut ViewIntentDto, unhonored: &mut Vec<String>) {
        if !matches!(intent.paths, Some(Some(_))) {
            return;
        }
        match self.found {
            Some(found) => intent.paths = Some(Some(found)),
            None => intent.paths = None,
        }
        if let Some(note) = self.note {
            unhonored.push(note);
        }
    }
}

impl KernelMcpServer {
    /// Searches the paths `requested` names over `abouts`, the planes the
    /// view will draw, the first of which is the loom's about.
    pub(super) async fn search_view_paths(
        &self,
        requested: Option<&PathsDto>,
        abouts: &[String],
    ) -> SearchedPaths {
        let Some(requested) = requested else {
            return SearchedPaths::default();
        };
        let Some(about) = abouts.first() else {
            return SearchedPaths {
                found: None,
                note: Some(
                    "paths need a view woven over an about; open one with kmp_view_open first"
                        .to_string(),
                ),
            };
        };
        let arguments = curate_paths_arguments(requested, about, abouts);
        match self.backend.call_tool("kmp_curate", &arguments).await {
            Ok(response) => match found_paths(requested, &response) {
                Some(found) => SearchedPaths {
                    found: Some(found),
                    note: None,
                },
                None => SearchedPaths {
                    found: None,
                    note: Some(
                        "paths: kmp_curate answered in a shape the loom cannot draw".to_string(),
                    ),
                },
            },
            Err(error) => SearchedPaths {
                found: None,
                note: Some(format!(
                    "paths could not be searched, so the drawn paths are unchanged: {}",
                    error.message
                )),
            },
        }
    }
}

/// The `kmp_curate` call a person or agent would make for the same paths.
fn curate_paths_arguments(requested: &PathsDto, about: &str, abouts: &[String]) -> Value {
    let mut arguments = json!({"mode": "paths", "about": about, "from": requested.from});
    if let Some(to) = requested.to.as_ref() {
        arguments["to"] = json!(to);
    }
    if let Some(max_hops) = requested.max_hops {
        arguments["max_hops"] = json!(max_hops);
    }
    if abouts.len() > 1 {
        arguments["dimensions"] = json!({"scope": "abouts", "abouts": abouts});
    }
    arguments
}

/// The tool's answer as the view keeps it, with the request it answers.
fn found_paths(requested: &PathsDto, response: &Value) -> Option<PathsDto> {
    let content = response
        .get("structuredContent")
        .unwrap_or(response)
        .clone();
    let found: PathsDto = serde_json::from_value(json!({
        "from": requested.from,
        "to": requested.to,
        "max_hops": requested.max_hops,
        "review_token": content.get("review_token"),
        "summary": content.get("summary").and_then(Value::as_str).unwrap_or_default(),
        "paths": content.get("paths").cloned().unwrap_or_else(|| json!([])),
        "avoided": content.get("avoided").cloned().unwrap_or_else(|| json!([])),
        "warnings": content.get("warnings").cloned().unwrap_or_else(|| json!([])),
    }))
    .ok()?;
    Some(found)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn requested() -> PathsDto {
        PathsDto {
            from: "a:1".into(),
            to: Some("b:2".into()),
            max_hops: Some(4),
            ..PathsDto::default()
        }
    }

    #[test]
    fn the_search_is_the_call_an_agent_would_make_over_the_drawn_planes() {
        let one = curate_paths_arguments(&requested(), "a", &["a".into()]);
        assert_eq!(
            one,
            json!({"mode": "paths", "about": "a", "from": "a:1", "to": "b:2", "max_hops": 4})
        );
        let two = curate_paths_arguments(&requested(), "a", &["a".into(), "b".into()]);
        assert_eq!(
            two["dimensions"],
            json!({"scope": "abouts", "abouts": ["a", "b"]})
        );
    }

    #[test]
    fn a_curate_answer_becomes_the_drawn_paths_with_its_proposed_hops() {
        let response = json!({"structuredContent": {
            "summary": "1 paths", "review_token": "f".repeat(64), "warnings": [],
            "paths": [{"proposed": 1, "confidence": 0.7, "hops": [
                {"from": {"ref": "a:1", "about": "a", "excerpt": "x"},
                 "to": {"ref": "b:2", "about": "b", "excerpt": "y"},
                 "rel": "same_event_as", "declared": false, "reversed": false,
                 "confidence": 0.7, "item_id": "m1"}]}],
            "avoided": [], "jev": null, "next_actions": []}});
        let found = found_paths(&requested(), &response).expect("drawable");
        assert_eq!(found.from, "a:1");
        assert_eq!(found.max_hops, Some(4));
        assert_eq!(found.paths[0].hops[0].item_id.as_deref(), Some("m1"));
        assert_eq!(found.review_token.as_deref(), Some("f".repeat(64).as_str()));
    }

    #[test]
    fn a_search_that_could_not_run_leaves_the_paths_alone_and_says_why() {
        let mut intent = ViewIntentDto {
            paths: Some(Some(requested())),
            ..ViewIntentDto::default()
        };
        let mut unhonored = Vec::new();
        SearchedPaths {
            found: None,
            note: Some("no".into()),
        }
        .apply_to(&mut intent, &mut unhonored);
        assert!(intent.paths.is_none());
        assert_eq!(unhonored, ["no"]);

        let mut cleared = ViewIntentDto {
            paths: Some(None),
            ..ViewIntentDto::default()
        };
        SearchedPaths::default().apply_to(&mut cleared, &mut unhonored);
        assert_eq!(cleared.paths, Some(None), "a clear searches nothing");
    }
}
