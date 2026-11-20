//! The abouts a store carries for the machine rather than for a project.
//!
//! The shipped guides and the worked demo are written into whichever store
//! a session opens, so an agent can read them anywhere. They are not that
//! project's memory: the maintained `.kmp/memory.jsonl` and an export of the
//! project head leave them out, and every install can write them again.

/// Every about that never travels in a project's committed bundle.
pub fn local_abouts() -> Vec<String> {
    let mut abouts = crate::guide::abouts_owned();
    abouts.push(crate::demo::ABOUT.to_string());
    abouts
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_guide_abouts_and_the_demo_stay_out_of_project_bundles() {
        assert_eq!(
            local_abouts(),
            vec!["guide:kmp", "guide:kmp-agent", "example:kmp-demo"]
        );
    }
}
