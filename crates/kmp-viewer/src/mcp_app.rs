use std::sync::OnceLock;

use crate::routes::{
    INDEX_HTML, LOOM_CORE_JS, LOOM_CSS, LOOM_JS, LOOM_MODULES, PIXI_JS, PIXI_UNSAFE_EVAL_JS,
};

const MCP_APP_BRIDGE: &str = include_str!("../ui/mcp-app-bridge.js");

/// Self-contained MCP App resource: identical renderer and view semantics to
/// the loopback adapter, with host-proxied tool calls instead of HTTP
/// fetches. The script-tag modules are inlined in the exact order
/// `index.html` loads them, so both faces run the same composition.
pub fn mcp_app_html() -> &'static str {
    static HTML: OnceLock<String> = OnceLock::new();
    HTML.get_or_init(|| {
        let script = |source: &str| {
            format!(
                "<script>{}</script>",
                source.replace("</script", "<\\/script")
            )
        };
        let mut html = INDEX_HTML
            .replace(
                "<link rel=\"stylesheet\" href=\"/assets/loom.css\">",
                &format!("<style>{LOOM_CSS}</style>"),
            )
            .replace(
                "<script src=\"/assets/pixi.min.js\" defer></script>",
                &format!("{}{}", script(MCP_APP_BRIDGE), script(PIXI_JS)),
            )
            .replace(
                "<script src=\"/assets/pixi-unsafe-eval.min.js\" defer></script>",
                &script(PIXI_UNSAFE_EVAL_JS),
            )
            .replace(
                "<script src=\"/assets/loom-core.js\" defer></script>",
                &script(LOOM_CORE_JS),
            )
            .replace(
                "<script src=\"/assets/loom.js\" defer></script>",
                &script(LOOM_JS),
            );
        for (name, source) in LOOM_MODULES {
            html = html.replace(
                &format!("<script src=\"/assets/{name}\" defer></script>"),
                &script(source),
            );
        }
        html
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn app_resource_is_self_contained_for_a_zero_domain_csp() {
        let html = mcp_app_html();
        assert!(html.starts_with("<!doctype html>"));
        assert!(!html.contains("src=\"/assets/"));
        assert!(!html.contains("href=\"/assets/"));
        assert!(html.contains("ui/initialize"));
        assert!(html.contains("kmp_view_read_projection"));
    }

    #[test]
    fn the_shared_loom_always_names_who_controls_the_view() {
        let html = mcp_app_html();
        assert!(html.contains("human-controlled view"));
        assert!(html.contains("moved the loom"));
        assert!(html.contains("undo.hidden = true"));
        assert!(html.contains("undo.hidden = false"));
    }

    #[test]
    fn agent_trace_frames_its_path_before_drawing_it() {
        let html = mcp_app_html();
        assert!(html.contains("framed = await frameRefs(refs)"));
        assert!(html.contains(
            "const inspect = await api(\"/api/node\", { about: model.about, id: ref, raw: \"1\" })"
        ));
        assert!(html.contains("await KMP_APP.data.loadProjection()"));
        assert!(
            html.contains("view.full = KMP_LOOM.extentIncluding(view.full, lo - pad, hi + pad)")
        );
        assert!(html.contains("await KMP_APP.sync.frameRefs(trace.nodes.map((node) => node.id))"));
        assert!(
            html.contains("runTrace({ framePath: !explicitRange, preserveWindow: explicitRange })")
        );
    }

    /// #463: the shared loom's browser half reconciles a full snapshot — it
    /// normalizes the cleared facets, drops a stale explicit range on a
    /// ref-only focus, and always applies the snapshot's search.
    #[test]
    fn a_new_agent_snapshot_replaces_stale_browser_filters() {
        let html = mcp_app_html();
        assert!(html.contains("const facets = KMP_LOOM.agentStateFacets(state)"));
        assert!(html.contains("view.focusRange = null"));
        assert!(html.contains("KMP_APP.panels.setSearch(facets.search)"));
    }

    #[test]
    fn every_projection_query_uses_the_clock_visible_on_the_loom() {
        let html = mcp_app_html();
        assert_eq!(
            html.matches("fetchProjection(\n        model.about,\n        view.clock,")
                .count(),
            1
        );
        assert_eq!(
            html.matches("fetchProjection(\n        about,\n        view.clock,")
                .count(),
            1
        );
    }
}
