/// The original vendored renderer used by exact-parity checks.
pub(crate) fn renderer_source() -> &'static str {
    include_str!("../ui/vendor/three.min.js")
}

#[cfg(test)]
mod tests {
    use std::io::Read;

    use flate2::read::GzDecoder;

    use crate::static_asset::static_asset;

    use super::renderer_source;

    #[test]
    fn precompressed_renderer_decodes_to_every_vendored_byte() {
        let asset = static_asset("three.min.js").expect("renderer asset");
        let mut decoded = Vec::new();
        GzDecoder::new(asset.gzip())
            .read_to_end(&mut decoded)
            .expect("build-generated renderer is valid gzip");
        assert_eq!(decoded, asset.identity());
        assert_eq!(decoded, renderer_source().as_bytes());
    }
}
