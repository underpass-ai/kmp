use std::fmt::Write as _;
use std::fs::{self, File};
use std::io::{self, Write as _};
use std::path::{Path, PathBuf};

use flate2::{Compression, write::GzEncoder};
use sha2::{Digest, Sha256};

const ASSETS: &[(&str, &str)] = &[
    ("loom.css", "text/css; charset=utf-8"),
    ("loom-loading.css", "text/css; charset=utf-8"),
    ("loom-shell.css", "text/css; charset=utf-8"),
    ("three.min.js", "text/javascript; charset=utf-8"),
    ("loom-core.js", "text/javascript; charset=utf-8"),
    ("loom-provenance.js", "text/javascript; charset=utf-8"),
    ("loom-evidence.js", "text/javascript; charset=utf-8"),
    ("loom-observability.js", "text/javascript; charset=utf-8"),
    ("loom-theme.js", "text/javascript; charset=utf-8"),
    ("loom-navigator.js", "text/javascript; charset=utf-8"),
    ("loom-camera.js", "text/javascript; charset=utf-8"),
    ("loom-planes.js", "text/javascript; charset=utf-8"),
    ("loom-relation-lines.js", "text/javascript; charset=utf-8"),
    ("loom-three.js", "text/javascript; charset=utf-8"),
    ("loom-scene-model.js", "text/javascript; charset=utf-8"),
    ("loom-layers.js", "text/javascript; charset=utf-8"),
    ("loom-control.js", "text/javascript; charset=utf-8"),
    ("loom-catalogue.js", "text/javascript; charset=utf-8"),
    ("loom-time-controls.js", "text/javascript; charset=utf-8"),
    ("loom-state.js", "text/javascript; charset=utf-8"),
    ("loom-loading.js", "text/javascript; charset=utf-8"),
    ("loom-loading-view.js", "text/javascript; charset=utf-8"),
    ("loom-api.js", "text/javascript; charset=utf-8"),
    ("loom-panels.js", "text/javascript; charset=utf-8"),
    ("loom-viewport.js", "text/javascript; charset=utf-8"),
    ("loom-data.js", "text/javascript; charset=utf-8"),
    ("loom-selection.js", "text/javascript; charset=utf-8"),
    ("loom-sync.js", "text/javascript; charset=utf-8"),
    ("loom-scene.js", "text/javascript; charset=utf-8"),
    ("loom-gestures.js", "text/javascript; charset=utf-8"),
    ("loom.js", "text/javascript; charset=utf-8"),
];

const MCP_APP_BRIDGE: &str = "ui/mcp-app-bridge.js";

fn main() -> io::Result<()> {
    let output = PathBuf::from(std::env::var_os("OUT_DIR").expect("Cargo output directory"));
    let mut generated = String::from("pub(crate) const STATIC_ASSETS: &[StaticAsset] = &[\n");
    let mut index = fs::read_to_string("ui/index.html")?;
    let original_index = index.clone();

    println!("cargo:rerun-if-changed=ui/index.html");
    println!("cargo:rerun-if-changed={MCP_APP_BRIDGE}");
    for (name, content_type) in ASSETS {
        let source = asset_source(name);
        println!("cargo:rerun-if-changed={}", source.display());
        let identity = fs::read(&source)?;
        let version = hex_digest(&identity);
        let gzip_name = format!("asset-{name}.gz");
        let mut encoder =
            GzEncoder::new(File::create(output.join(&gzip_name))?, Compression::best());
        encoder.write_all(&identity)?;
        encoder.finish()?;
        writeln!(
            generated,
            "    StaticAsset::new({name:?}, {content_type:?}, include_bytes!(concat!(env!(\"CARGO_MANIFEST_DIR\"), {source:?})), include_bytes!(concat!(env!(\"OUT_DIR\"), {gzip:?})), {version:?}),",
            source = format!("/{}", source.display()),
            gzip = format!("/{gzip_name}"),
        )
        .expect("writing to String cannot fail");
        index = index.replace(
            &format!("/assets/{name}"),
            &format!("/assets/{version}/{name}"),
        );
    }
    generated.push_str("];\n");
    fs::write(output.join("static_assets.rs"), generated)?;
    fs::write(output.join("index.html"), index)?;
    fs::write(
        output.join("chronoloom-mcp-app.html"),
        mcp_app_html(original_index)?,
    )?;
    Ok(())
}

fn asset_source(name: &str) -> PathBuf {
    if name == "three.min.js" {
        PathBuf::from("ui/vendor/three.min.js")
    } else {
        Path::new("ui").join(name)
    }
}

fn hex_digest(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn mcp_app_html(mut html: String) -> io::Result<String> {
    let script = |source: &str| {
        format!(
            "<script>{}</script>",
            source.replace("</script", "<\\/script")
        )
    };
    let bridge = fs::read_to_string(MCP_APP_BRIDGE)?;
    for (name, _) in ASSETS {
        let source = fs::read_to_string(asset_source(name))?;
        let element = if name.ends_with(".css") {
            format!("<link rel=\"stylesheet\" href=\"/assets/{name}\">")
        } else {
            format!("<script src=\"/assets/{name}\" defer></script>")
        };
        let inline = if name == &"three.min.js" {
            format!("{}{}", script(&bridge), script(&source))
        } else if name.ends_with(".css") {
            format!("<style>{source}</style>")
        } else {
            script(&source)
        };
        html = html.replace(&element, &inline);
    }
    Ok(html)
}
