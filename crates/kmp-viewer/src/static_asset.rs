use crate::http::{HttpRequest, HttpResponse};

/// One immutable asset embedded in both identity and gzip representations.
#[derive(Debug)]
pub(crate) struct StaticAsset {
    name: &'static str,
    content_type: &'static str,
    identity: &'static [u8],
    gzip: &'static [u8],
    version: &'static str,
}

impl StaticAsset {
    pub(crate) const fn new(
        name: &'static str,
        content_type: &'static str,
        identity: &'static [u8],
        gzip: &'static [u8],
        version: &'static str,
    ) -> Self {
        Self {
            name,
            content_type,
            identity,
            gzip,
            version,
        }
    }

    pub(crate) fn response(&self, request: &HttpRequest) -> HttpResponse {
        let gzip_quality = request.encoding_quality("gzip").unwrap_or(0.0);
        let identity_quality = request.identity_quality();
        if gzip_quality <= 0.0 && identity_quality <= 0.0 {
            return HttpResponse::error(406, "no acceptable asset representation");
        }
        let (body, encoding) = if gzip_quality > 0.0 && gzip_quality >= identity_quality {
            (self.gzip, Some("gzip"))
        } else {
            (self.identity, None)
        };
        let mut response = HttpResponse::static_bytes(self.content_type, body)
            .with_header("Cache-Control", "public, max-age=31536000, immutable")
            .with_header("Vary", "Accept-Encoding");
        if let Some(encoding) = encoding {
            response = response.with_header("Content-Encoding", encoding);
        }
        response
    }

    fn matches(&self, version: &str, name: &str) -> bool {
        self.version == version && self.name == name
    }
}

include!(concat!(env!("OUT_DIR"), "/static_assets.rs"));

pub(crate) fn route_static_asset(request: &HttpRequest) -> Option<HttpResponse> {
    let rest = request.path.strip_prefix("/assets/")?;
    if request.method != "GET" && request.method != "HEAD" {
        return Some(HttpResponse::error(
            405,
            "static assets are served by GET and HEAD",
        ));
    }
    let Some((version, name)) = rest.split_once('/') else {
        return Some(HttpResponse::error(404, "unknown or stale asset version"));
    };
    let Some(asset) = STATIC_ASSETS
        .iter()
        .find(|asset| asset.matches(version, name))
    else {
        return Some(HttpResponse::error(404, "unknown or stale asset version"));
    };
    let response = asset.response(request);
    Some(if request.method == "HEAD" {
        response.without_body()
    } else {
        response
    })
}

#[cfg(test)]
pub(crate) fn static_asset(name: &str) -> Option<&'static StaticAsset> {
    STATIC_ASSETS.iter().find(|asset| asset.name == name)
}

#[cfg(test)]
impl StaticAsset {
    pub(crate) fn identity(&self) -> &'static [u8] {
        self.identity
    }

    pub(crate) fn gzip(&self) -> &'static [u8] {
        self.gzip
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use super::*;

    fn request(accept_encoding: Option<&str>) -> HttpRequest {
        HttpRequest {
            method: "GET".to_string(),
            path: String::new(),
            query: BTreeMap::new(),
            host: Some("127.0.0.1".to_string()),
            cookie: None,
            accept_encoding: accept_encoding.map(str::to_string),
        }
    }

    fn header<'a>(response: &'a HttpResponse, name: &str) -> Option<&'a str> {
        response.headers.iter().find_map(|(candidate, value)| {
            candidate
                .eq_ignore_ascii_case(name)
                .then_some(value.as_str())
        })
    }

    #[test]
    fn representation_negotiation_honors_quality_and_identity_refusal() {
        let asset = static_asset("three.min.js").expect("renderer asset");
        let gzip = asset.response(&request(Some("br, gzip")));
        assert_eq!(header(&gzip, "Content-Encoding"), Some("gzip"));
        assert_eq!(gzip.body.as_slice(), asset.gzip());
        assert_eq!(header(&gzip, "Vary"), Some("Accept-Encoding"));

        let identity = asset.response(&request(Some("gzip;q=0.2, identity;q=0.8")));
        assert_eq!(header(&identity, "Content-Encoding"), None);
        assert_eq!(identity.body.as_slice(), asset.identity());

        let unacceptable = asset.response(&request(Some("gzip;q=0, identity;q=0")));
        assert_eq!(unacceptable.status, 406);
        let wildcard_refusal = asset.response(&request(Some("gzip;q=0, *;q=0")));
        assert_eq!(wildcard_refusal.status, 406);
    }

    #[test]
    fn generated_index_names_only_current_content_versions() {
        let index = crate::routes::INDEX_HTML;
        for asset in STATIC_ASSETS {
            let path = format!("/assets/{}/{}", asset.version, asset.name);
            assert!(index.contains(&path), "index omits {path}");
            assert!(!index.contains(&format!("/assets/{}\"", asset.name)));
        }
    }
}
