use std::io::Read;
use std::sync::OnceLock;

use flate2::read::GzDecoder;

/// The original vendored renderer, decoded once for both viewer surfaces.
pub(crate) fn renderer_source() -> &'static str {
    static SOURCE: OnceLock<String> = OnceLock::new();
    SOURCE.get_or_init(|| {
        let compressed = include_bytes!(concat!(env!("OUT_DIR"), "/three.min.js.gz"));
        // Only the compile-time length is retained from this source include.
        let mut source = String::with_capacity(include_str!("../ui/vendor/three.min.js").len());
        GzDecoder::new(compressed.as_slice())
            .read_to_string(&mut source)
            .expect("build-generated renderer is valid gzip and UTF-8");
        source
    })
}

#[cfg(test)]
mod tests {
    use super::renderer_source;

    #[test]
    fn cached_renderer_preserves_every_vendored_byte() {
        let decoded = renderer_source();
        assert_eq!(decoded, include_str!("../ui/vendor/three.min.js"));
        assert!(std::ptr::eq(decoded, renderer_source()));
    }
}
