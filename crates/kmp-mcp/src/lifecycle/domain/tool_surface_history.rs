use std::collections::BTreeSet;

use super::release_version::ReleaseVersion;

/// The last release whose surface had no `kmp_relate`. Every release after
/// it answers the fourteenth tool; every release up to it answered thirteen.
const LAST_RELEASE_WITHOUT_RELATE: &str = "0.10.0";

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
    let last_without_relate =
        ReleaseVersion::parse(LAST_RELEASE_WITHOUT_RELATE).expect("a release version");
    if target.is_newer_than(&last_without_relate) {
        current
    } else {
        current
            .into_iter()
            .filter(|name| name != "kmp_relate")
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn surface(names: &[&str]) -> Vec<String> {
        names.iter().map(|name| name.to_string()).collect()
    }

    #[test]
    fn an_older_engine_is_held_to_the_surface_it_shipped_with() {
        let current = surface(&["kmp_ask", "kmp_relate", "kmp_wake"]);
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
            assert!(expected.contains("kmp_ask"));
        }
    }

    #[test]
    fn a_release_after_relate_and_this_build_answer_the_whole_surface() {
        let current = surface(&["kmp_ask", "kmp_relate", "kmp_wake"]);
        for newer in ["0.10.1", "0.11.0", "1.0.0"] {
            let expected =
                expected_tool_surface(&ReleaseVersion::parse(newer).expect("v"), current.clone());
            assert!(expected.contains("kmp_relate"), "{newer} carries relate");
        }
        let this_build = expected_tool_surface(&ReleaseVersion::current(), current.clone());
        assert_eq!(
            this_build.len(),
            3,
            "this build answers everything it declares"
        );
    }
}
