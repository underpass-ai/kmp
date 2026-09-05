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
    current
        .into_iter()
        .filter(|name| !not_yet.contains(name.as_str()))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn surface(names: &[&str]) -> Vec<String> {
        names.iter().map(|name| name.to_string()).collect()
    }

    #[test]
    fn an_older_engine_is_held_to_the_surface_it_shipped_with() {
        let current = surface(&["kmp_ask", "kmp_relate", "kmp_relabel", "kmp_wake"]);
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
    fn a_release_after_relabel_and_this_build_answer_the_whole_surface() {
        let current = surface(&["kmp_ask", "kmp_relate", "kmp_relabel", "kmp_wake"]);
        for newer in ["0.12.0", "1.0.0"] {
            let expected =
                expected_tool_surface(&ReleaseVersion::parse(newer).expect("v"), current.clone());
            assert!(expected.contains("kmp_relate"), "{newer} carries relate");
            assert!(expected.contains("kmp_relabel"), "{newer} carries relabel");
        }
        let this_build = expected_tool_surface(&ReleaseVersion::current(), current.clone());
        assert_eq!(
            this_build.len(),
            4,
            "this build answers everything it declares"
        );
    }
}
