use std::collections::BTreeSet;

use super::release_version::ReleaseVersion;

/// Each tool that joined the surface after the first release, beside the
/// last release whose surface had none of it. Every release after that one
/// answers the tool; every release up to it did not, and is not held to it.
const TOOLS_ADDED_LATER: &[(&str, &str)] = &[
    // The fourteenth tool: every release up to 0.10.0 answered thirteen.
    ("kmp_relate", "0.10.0"),
    // The fifteenth: every release up to 0.11.0 answered fourteen.
    ("kmp_relabel", "0.11.0"),
    ("kmp_guide", "0.15.0"),
    // Condense landed after the published 0.17.0 release. The current-build
    // override below still holds this development build to its full surface.
    ("kmp_condense", "0.17.0"),
    // The summaries audit landed after the published 0.17.0 release too.
    ("kmp_summaries_audit", "0.17.0"),
    // One time-navigation verb replaced the four per-move tools after the
    // published 0.19.0 release (#544 C2).
    ("kmp_time", "0.19.0"),
    // Relation curation with TypeSafe Jev, after the published 0.20.1
    // release.
    ("kmp_curate", "0.20.1"),
];

/// Each tool that left the surface, beside the last release that still
/// answered it. An older engine is held to it; this build and later are not.
const TOOLS_REMOVED_LATER: &[(&str, &str)] = &[
    ("kmp_goto", "0.19.0"),
    ("kmp_near", "0.19.0"),
    ("kmp_rewind", "0.19.0"),
    ("kmp_forward", "0.19.0"),
];

/// The tool surface an engine of `target` is held to.
///
/// The lifecycle proof is exact: an engine is accepted only when it answers
/// the whole surface and nothing else. That surface is this build's when
/// the engine is this build, and the one the engine shipped with when it is
/// an older release — the one `setup` was asked to install, or the one the
/// doctor found in place. A 0.4.2 engine that answers thirteen tools is
/// exactly what 0.4.2 was, and holding it to a tool that did not exist yet
/// would refuse every honest older engine the day a tool is added.
pub fn expected_tool_surface(
    target: &ReleaseVersion,
    current: impl IntoIterator<Item = String>,
) -> BTreeSet<String> {
    let current = current.into_iter().collect::<BTreeSet<_>>();
    if target.represents_same_release(&ReleaseVersion::current()) {
        return current;
    }
    let not_yet = TOOLS_ADDED_LATER
        .iter()
        .filter(|(_, last_release_without)| {
            let last_without =
                ReleaseVersion::parse(last_release_without).expect("a release version");
            !target.is_newer_than(&last_without)
        })
        .map(|(tool, _)| *tool)
        .collect::<BTreeSet<_>>();
    let still_answered = TOOLS_REMOVED_LATER
        .iter()
        .filter(|(_, last_release_with)| {
            let last_with = ReleaseVersion::parse(last_release_with).expect("a release version");
            !target.is_newer_than(&last_with)
        })
        .map(|(tool, _)| (*tool).to_string());
    current
        .into_iter()
        .filter(|name| !not_yet.contains(name.as_str()))
        .chain(still_answered)
        .collect()
}

/// Whether an engine of `target` that answered `answered` passes the proof.
///
/// Up to this build the proof is exact: `answered` must be the surface
/// `target` shipped with. A release newer than this build can have added or
/// removed tools this build has never heard of, so holding it to this
/// build's surface would refuse every update that adds a tool (0.20.1 could
/// not update to 0.21.0, which added `kmp_curate`). A newer engine is held
/// to what this build can know: it answers, and every tool it answers is a
/// KMP tool. The version check beside this one still pins the release.
pub fn surface_is_accepted(
    target: &ReleaseVersion,
    answered: &BTreeSet<String>,
    current: impl IntoIterator<Item = String>,
) -> bool {
    if target.is_newer_than(&ReleaseVersion::current()) {
        return !answered.is_empty() && answered.iter().all(|tool| tool.starts_with("kmp_"));
    }
    *answered == expected_tool_surface(target, current)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn surface(names: &[&str]) -> Vec<String> {
        names.iter().map(|name| name.to_string()).collect()
    }

    #[test]
    fn an_older_engine_is_held_to_the_surface_it_shipped_with() {
        let current = surface(&[
            "kmp_ask",
            "kmp_relate",
            "kmp_relabel",
            "kmp_condense",
            "kmp_wake",
        ]);
        // Not the last release without relate itself: until the next
        // release bumps the crate, this build carries its version and
        // answers the whole surface, and the same-release rule says so.
        for older in ["0.4.2", "0.9.1"] {
            let expected =
                expected_tool_surface(&ReleaseVersion::parse(older).expect("v"), current.clone());
            assert!(
                !expected.contains("kmp_relate"),
                "{older} shipped before relate"
            );
            assert!(
                !expected.contains("kmp_relabel"),
                "{older} shipped before relabel"
            );
            assert!(
                !expected.contains("kmp_condense"),
                "{older} shipped before condense"
            );
            assert!(expected.contains("kmp_ask"));
        }
    }

    #[test]
    fn a_release_between_the_two_additions_answers_relate_and_not_relabel() {
        let current = surface(&["kmp_ask", "kmp_relate", "kmp_relabel", "kmp_wake"]);
        // Not 0.11.0 itself: until the next release bumps the crate, this
        // build carries that version and answers the whole surface.
        let between = "0.10.1";
        let expected =
            expected_tool_surface(&ReleaseVersion::parse(between).expect("v"), current.clone());
        assert!(expected.contains("kmp_relate"), "{between} carries relate");
        assert!(
            !expected.contains("kmp_relabel"),
            "{between} shipped before relabel"
        );
    }

    #[test]
    fn a_release_after_condense_and_this_build_answer_the_whole_surface() {
        let current = surface(&[
            "kmp_ask",
            "kmp_relate",
            "kmp_relabel",
            "kmp_condense",
            "kmp_wake",
        ]);
        for newer in ["0.17.1", "1.0.0"] {
            let expected =
                expected_tool_surface(&ReleaseVersion::parse(newer).expect("v"), current.clone());
            assert!(expected.contains("kmp_relate"), "{newer} carries relate");
            assert!(expected.contains("kmp_relabel"), "{newer} carries relabel");
            assert!(
                expected.contains("kmp_condense"),
                "{newer} carries condense"
            );
        }
        let this_build = expected_tool_surface(&ReleaseVersion::current(), current.clone());
        assert_eq!(this_build, current.into_iter().collect());
    }

    #[test]
    fn an_engine_before_the_time_verb_answers_the_four_moves_it_replaced() {
        let current = surface(&["kmp_ask", "kmp_time", "kmp_wake"]);
        let older = expected_tool_surface(
            &ReleaseVersion::parse("0.18.2").expect("v"),
            current.clone(),
        );
        assert!(
            !older.contains("kmp_time"),
            "0.18.2 shipped before kmp_time"
        );
        for tool in ["kmp_goto", "kmp_near", "kmp_rewind", "kmp_forward"] {
            assert!(older.contains(tool), "0.18.2 answers {tool}");
        }
        let newer = expected_tool_surface(
            &ReleaseVersion::parse("0.20.0").expect("v"),
            current.clone(),
        );
        assert_eq!(newer, current.into_iter().collect());
    }

    #[test]
    fn a_release_newer_than_this_build_may_answer_tools_this_build_never_heard_of() {
        let current = surface(&["kmp_ask", "kmp_wake"]);
        let newer = ReleaseVersion::parse("999.0.0").expect("v");
        let answered = ["kmp_ask", "kmp_wake", "kmp_future"]
            .map(str::to_string)
            .into_iter()
            .collect::<BTreeSet<_>>();
        assert!(surface_is_accepted(&newer, &answered, current.clone()));
        let dropped = ["kmp_future"].map(str::to_string).into_iter().collect();
        assert!(
            surface_is_accepted(&newer, &dropped, current.clone()),
            "a newer release may also have removed tools"
        );
        let foreign = ["kmp_ask", "shell"]
            .map(str::to_string)
            .into_iter()
            .collect();
        assert!(!surface_is_accepted(&newer, &foreign, current.clone()));
        assert!(!surface_is_accepted(
            &newer,
            &BTreeSet::new(),
            current.clone()
        ));
        let this_build = ReleaseVersion::current();
        assert!(
            !surface_is_accepted(&this_build, &answered, current.clone()),
            "this build stays exact"
        );
        assert!(surface_is_accepted(
            &this_build,
            &current.clone().into_iter().collect(),
            current
        ));
    }
}
