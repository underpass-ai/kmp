use std::sync::OnceLock;

use crate::routes::{INDEX_HTML, LOOM_CORE_JS, LOOM_CSS, LOOM_JS, PIXI_JS, PIXI_UNSAFE_EVAL_JS};

const MCP_APP_BRIDGE: &str = include_str!("../ui/mcp-app-bridge.js");

/// Self-contained MCP App resource: identical renderer and view semantics to
/// the loopback adapter, with host-proxied tool calls instead of HTTP fetches.
pub fn mcp_app_html() -> &'static str {
    static HTML: OnceLock<String> = OnceLock::new();
    HTML.get_or_init(|| {
        let script = |source: &str| {
            format!(
                "<script>{}</script>",
                source.replace("</script", "<\\/script")
            )
        };
        INDEX_HTML
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
            )
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
}
