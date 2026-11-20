//! A deliberately small HTTP/1.1 surface: parse one bounded request head,
//! write one length-delimited response, and close. The viewer is a local,
//! read-only window over the kernel and does not need a routing framework,
//! persistent connections, or request bodies.

use std::collections::BTreeMap;

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;

use crate::response_body::ResponseBody;

/// Upper bound on the request head. A local viewer request is a short GET;
/// anything larger is a client that should not be talking to this port.
const MAX_REQUEST_HEAD_BYTES: usize = 16 * 1024;

#[derive(Debug)]
pub(crate) struct HttpRequest {
    pub(crate) method: String,
    pub(crate) path: String,
    pub(crate) query: BTreeMap<String, String>,
    pub(crate) host: Option<String>,
    pub(crate) cookie: Option<String>,
    pub(crate) accept_encoding: Option<String>,
    /// `Origin`, as a browser sends it on every POST.
    pub(crate) origin: Option<String>,
    /// `Sec-Fetch-Site`, as current browsers send it on every request.
    pub(crate) fetch_site: Option<String>,
}

impl HttpRequest {
    pub(crate) fn param(&self, key: &str) -> Option<&str> {
        self.query
            .get(key)
            .map(String::as_str)
            .filter(|value| !value.trim().is_empty())
    }

    pub(crate) fn cookie(&self, name: &str) -> Option<&str> {
        self.cookie.as_deref().and_then(|header| {
            header.split(';').find_map(|pair| {
                let (candidate, value) = pair.trim().split_once('=')?;
                (candidate == name).then_some(value)
            })
        })
    }

    /// Why a state-changing request is refused because it came from another
    /// page, or `None` when it came from this one.
    ///
    /// `SameSite` does not cover this: cookies and "same site" ignore the
    /// port, so a page served from another port of this loopback host sends
    /// the session cookie with a form or `fetch` POST. Browsers name the
    /// page that sent a request in `Origin` and `Sec-Fetch-Site`; either one
    /// naming another page refuses it. A client that sends neither (curl, a
    /// script) is not a browser page and has already shown the session
    /// cookie to get this far.
    pub(crate) fn cross_origin_refusal(&self) -> Option<HttpResponse> {
        if let Some(site) = self.fetch_site.as_deref()
            && !matches!(
                site.trim().to_ascii_lowercase().as_str(),
                "same-origin" | "none"
            )
        {
            return Some(HttpResponse::error(
                403,
                "the shared view changes only from the viewer's own page",
            ));
        }
        let origin = self.origin.as_deref()?.trim();
        let own = self
            .host
            .as_deref()
            .map(|host| format!("http://{}", host.trim()));
        if own.is_some_and(|own| own.eq_ignore_ascii_case(origin)) {
            None
        } else {
            Some(HttpResponse::error(
                403,
                "the shared view changes only from the viewer's own page",
            ))
        }
    }

    pub(crate) fn encoding_quality(&self, encoding: &str) -> Option<f32> {
        let mut wildcard = None;
        for item in self.accept_encoding.as_deref()?.split(',') {
            let (name, quality) = encoding_item(item);
            if name.eq_ignore_ascii_case(encoding) {
                return Some(quality);
            }
            if name == "*" {
                wildcard = Some(quality);
            }
        }
        wildcard
    }

    pub(crate) fn identity_quality(&self) -> f32 {
        let Some(header) = self.accept_encoding.as_deref() else {
            return 1.0;
        };
        let explicit = header.split(',').find_map(|item| {
            let (name, quality) = encoding_item(item);
            name.eq_ignore_ascii_case("identity").then_some(quality)
        });
        explicit.unwrap_or_else(|| {
            if self.encoding_quality("*") == Some(0.0) {
                0.0
            } else {
                1.0
            }
        })
    }
}

fn encoding_item(item: &str) -> (&str, f32) {
    let mut parts = item.trim().split(';');
    let name = parts.next().unwrap_or_default().trim();
    let quality = parts
        .find_map(|parameter| {
            let (key, value) = parameter.trim().split_once('=')?;
            key.trim().eq_ignore_ascii_case("q").then_some(value.trim())
        })
        .and_then(|value| value.parse::<f32>().ok())
        .unwrap_or(1.0)
        .clamp(0.0, 1.0);
    (name, quality)
}

#[derive(Debug)]
pub(crate) struct HttpResponse {
    pub(crate) status: u16,
    pub(crate) content_type: &'static str,
    pub(crate) body: ResponseBody,
    pub(crate) headers: Vec<(&'static str, String)>,
    /// A HEAD answer: the same head a GET would send, with the body withheld.
    /// The body stays here so `Content-Length` keeps describing what a GET
    /// would return, which is what makes the two answers agree.
    pub(crate) omit_body: bool,
}

impl HttpResponse {
    pub(crate) fn without_body(mut self) -> Self {
        self.omit_body = true;
        self
    }

    pub(crate) fn with_header(mut self, name: &'static str, value: impl Into<String>) -> Self {
        self.headers.push((name, value.into()));
        self
    }
}

impl HttpResponse {
    pub(crate) fn html(body: &'static str) -> Self {
        Self {
            status: 200,
            content_type: "text/html; charset=utf-8",
            body: ResponseBody::Static(body.as_bytes()),
            headers: Vec::new(),
            omit_body: false,
        }
    }

    pub(crate) fn json<T: serde::Serialize>(value: &T) -> Self {
        match serde_json::to_vec(value) {
            Ok(body) => Self {
                status: 200,
                content_type: "application/json",
                body: ResponseBody::Owned(body),
                headers: Vec::new(),
                omit_body: false,
            },
            Err(error) => Self::error(500, &format!("response serialization failed: {error}")),
        }
    }

    pub(crate) fn error(status: u16, message: &str) -> Self {
        let body = serde_json::json!({ "error": message });
        Self {
            status,
            content_type: "application/json",
            body: ResponseBody::Owned(body.to_string().into_bytes()),
            headers: Vec::new(),
            omit_body: false,
        }
    }

    pub(crate) fn redirect(location: &'static str) -> Self {
        Self {
            status: 303,
            content_type: "text/plain; charset=utf-8",
            body: ResponseBody::Static(&[]),
            headers: vec![("Location", location.to_string())],
            omit_body: false,
        }
    }

    pub(crate) fn static_bytes(content_type: &'static str, body: &'static [u8]) -> Self {
        Self {
            status: 200,
            content_type,
            body: ResponseBody::Static(body),
            headers: Vec::new(),
            omit_body: false,
        }
    }
}

/// Reads and parses one request head. Returns `Err` with the response the
/// client deserves when the request cannot or should not be served.
pub(crate) async fn read_request(stream: &mut TcpStream) -> Result<HttpRequest, HttpResponse> {
    let mut head = Vec::with_capacity(1024);
    let mut chunk = [0u8; 1024];
    while head_terminator(&head).is_none() {
        if head.len() > MAX_REQUEST_HEAD_BYTES {
            return Err(HttpResponse::error(431, "request head too large"));
        }
        let read = stream
            .read(&mut chunk)
            .await
            .map_err(|error| HttpResponse::error(400, &format!("read failed: {error}")))?;
        if read == 0 {
            return Err(HttpResponse::error(400, "connection closed mid-request"));
        }
        head.extend_from_slice(&chunk[..read]);
    }
    let head_end = head_terminator(&head).expect("terminator was found") + 4;
    if head_end > MAX_REQUEST_HEAD_BYTES {
        return Err(HttpResponse::error(431, "request head too large"));
    }

    let head_text = String::from_utf8_lossy(&head[..head_end]);
    let mut lines = head_text.split("\r\n");
    let request_line = lines.next().unwrap_or_default();
    let mut parts = request_line.split_ascii_whitespace();
    let method = parts.next().unwrap_or_default().to_string();
    let target = parts.next().unwrap_or_default();
    let version = parts.next().unwrap_or_default();
    if method.is_empty() || target.is_empty() || !version.starts_with("HTTP/1.") {
        return Err(HttpResponse::error(400, "malformed request line"));
    }

    let (path, raw_query) = match target.split_once('?') {
        Some((path, query)) => (path, query),
        None => (target, ""),
    };

    let mut host = None;
    let mut origin = None;
    let mut fetch_site = None;
    let mut cookies = Vec::new();
    let mut accept_encodings = Vec::new();
    let mut content_length = None;
    let mut transfer_encoding = false;
    for line in lines {
        if line.is_empty() {
            break;
        }
        if let Some((name, value)) = line.split_once(':') {
            if name.trim().eq_ignore_ascii_case("host") {
                host = Some(value.trim().to_string());
            } else if name.trim().eq_ignore_ascii_case("origin") {
                origin = Some(value.trim().to_string());
            } else if name.trim().eq_ignore_ascii_case("sec-fetch-site") {
                fetch_site = Some(value.trim().to_string());
            } else if name.trim().eq_ignore_ascii_case("cookie") {
                cookies.push(value.trim());
            } else if name.trim().eq_ignore_ascii_case("accept-encoding") {
                accept_encodings.push(value.trim());
            } else if name.trim().eq_ignore_ascii_case("content-length") {
                let parsed = value.trim().parse::<usize>().map_err(|_| {
                    HttpResponse::error(400, "malformed Content-Length is not accepted")
                })?;
                if content_length.is_some_and(|previous| previous != parsed) {
                    return Err(HttpResponse::error(
                        400,
                        "conflicting Content-Length headers are not accepted",
                    ));
                }
                content_length = Some(parsed);
            } else if name.trim().eq_ignore_ascii_case("transfer-encoding") {
                transfer_encoding = true;
            }
        }
    }
    if transfer_encoding {
        return Err(HttpResponse::error(
            400,
            "Transfer-Encoding is not accepted; viewer requests have no body",
        ));
    }
    if content_length.is_some_and(|length| length != 0) {
        return Err(HttpResponse::error(
            400,
            "viewer requests have no body; Content-Length must be zero",
        ));
    }

    Ok(HttpRequest {
        method,
        path: percent_decode(path),
        query: parse_query(raw_query),
        host,
        cookie: (!cookies.is_empty()).then(|| cookies.join("; ")),
        accept_encoding: (!accept_encodings.is_empty()).then(|| accept_encodings.join(",")),
        origin,
        fetch_site,
    })
}

/// Writes the response with the security headers every viewer response
/// carries. Dynamic/private responses default to no-store; static assets
/// supply their immutable policy explicitly.
pub(crate) async fn write_response(
    stream: &mut TcpStream,
    response: &HttpResponse,
) -> std::io::Result<()> {
    let reason = reason_phrase(response.status);
    let mut head = format!(
        "HTTP/1.1 {} {}\r\n\
         Content-Type: {}\r\n\
         Content-Length: {}\r\n\
         Connection: close\r\n\
         X-Content-Type-Options: nosniff\r\n\
         Referrer-Policy: no-referrer\r\n\
         Content-Security-Policy: default-src 'none'; script-src 'self'; \
         style-src 'self'; connect-src 'self'; img-src 'self' data:; \
         frame-ancestors 'none'; base-uri 'none'; form-action 'none'\r\n\
         X-Frame-Options: DENY\r\n\
         Cross-Origin-Resource-Policy: same-origin\r\n\
         Cross-Origin-Opener-Policy: same-origin\r\n",
        response.status,
        reason,
        response.content_type,
        response.body.len(),
    );
    if !response
        .headers
        .iter()
        .any(|(name, _)| name.eq_ignore_ascii_case("Cache-Control"))
    {
        head.push_str("Cache-Control: private, no-store\r\n");
    }
    for (name, value) in &response.headers {
        head.push_str(name);
        head.push_str(": ");
        head.push_str(value);
        head.push_str("\r\n");
    }
    head.push_str("\r\n");
    stream.write_all(head.as_bytes()).await?;
    if !response.omit_body {
        stream.write_all(response.body.as_slice()).await?;
    }
    stream.flush().await
}

/// The viewer answers only to names that resolve to this machine: the three
/// loopback names, and the loopback address it is actually bound to, which
/// is the one its printed link names. A browser lured to `evil.example`
/// pointing at 127.0.0.1 (DNS rebinding) sends that name as `Host` — and is
/// refused here.
pub(crate) fn host_is_local(host: Option<&str>, bound: Option<std::net::IpAddr>) -> bool {
    let Some(name) = host.and_then(|host| strip_port(host.trim())) else {
        return false;
    };
    if matches!(name, "localhost" | "127.0.0.1" | "[::1]") {
        return true;
    }
    let literal = name
        .strip_prefix('[')
        .and_then(|inner| inner.strip_suffix(']'))
        .unwrap_or(name);
    bound.is_some_and(|bound| {
        bound.is_loopback() && literal.parse::<std::net::IpAddr>().ok() == Some(bound)
    })
}

/// The name part of a `Host` value, or `None` when what follows the name is
/// not a port. A bracketed IPv6 literal ends at `]`, which must close the
/// value or be followed by `:port`; `[::1]evil.example` names nothing.
fn strip_port(host: &str) -> Option<&str> {
    let is_port = |port: &str| !port.is_empty() && port.bytes().all(|b| b.is_ascii_digit());
    if host.starts_with('[') {
        let end = host.find(']')?;
        let rest = &host[end + 1..];
        return (rest.is_empty() || rest.strip_prefix(':').is_some_and(is_port))
            .then_some(&host[..=end]);
    }
    match host.rsplit_once(':') {
        Some((name, port)) if is_port(port) => Some(name),
        Some(_) => None,
        None => Some(host),
    }
}

fn head_terminator(head: &[u8]) -> Option<usize> {
    head.windows(4).position(|window| window == b"\r\n\r\n")
}

fn parse_query(raw: &str) -> BTreeMap<String, String> {
    raw.split('&')
        .filter(|pair| !pair.is_empty())
        .map(|pair| match pair.split_once('=') {
            Some((key, value)) => (percent_decode(key), percent_decode(value)),
            None => (percent_decode(pair), String::new()),
        })
        .collect()
}

fn percent_decode(value: &str) -> String {
    let bytes = value.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        match bytes[index] {
            b'%' if index + 3 <= bytes.len() => {
                let high = hex_value(bytes[index + 1]);
                let low = hex_value(bytes[index + 2]);
                match (high, low) {
                    (Some(high), Some(low)) => {
                        out.push(high * 16 + low);
                        index += 3;
                    }
                    _ => {
                        out.push(b'%');
                        index += 1;
                    }
                }
            }
            b'+' => {
                out.push(b' ');
                index += 1;
            }
            byte => {
                out.push(byte);
                index += 1;
            }
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn hex_value(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

fn reason_phrase(status: u16) -> &'static str {
    match status {
        200 => "OK",
        303 => "See Other",
        400 => "Bad Request",
        401 => "Unauthorized",
        403 => "Forbidden",
        404 => "Not Found",
        405 => "Method Not Allowed",
        406 => "Not Acceptable",
        408 => "Request Timeout",
        409 => "Conflict",
        431 => "Request Header Fields Too Large",
        500 => "Internal Server Error",
        503 => "Service Unavailable",
        _ => "Response",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn percent_decoding_round_trips_reserved_characters() {
        assert_eq!(percent_decode("about%3Acheckout"), "about:checkout");
        assert_eq!(percent_decode("a%2Fb%3Fc%3Dd"), "a/b?c=d");
        assert_eq!(percent_decode("space+and%20percent"), "space and percent");
        assert_eq!(percent_decode("dangling%2"), "dangling%2");
        assert_eq!(percent_decode("%zz"), "%zz");
    }

    #[test]
    fn only_local_hosts_are_accepted() {
        assert!(host_is_local(Some("localhost:7317"), None));
        assert!(host_is_local(Some("127.0.0.1"), None));
        assert!(host_is_local(Some("[::1]:7317"), None));
        assert!(host_is_local(Some("[::1]"), None));
        assert!(!host_is_local(Some("evil.example"), None));
        assert!(!host_is_local(Some("evil.example:7317"), None));
        assert!(!host_is_local(None, None));
        // What follows `]` must be nothing or a port.
        assert!(!host_is_local(Some("[::1].evil.example"), None));
        assert!(!host_is_local(Some("[::1]evil.example:80"), None));
        assert!(!host_is_local(Some("[::1]:"), None));
        assert!(!host_is_local(Some("localhost:7317x"), None));
    }

    #[test]
    fn the_bound_loopback_address_answers_to_its_own_link() {
        let bound = "127.0.0.2".parse().ok();
        assert!(host_is_local(Some("127.0.0.2:7420"), bound));
        assert!(!host_is_local(Some("127.0.0.3:7420"), bound));
        assert!(!host_is_local(Some("127.0.0.2:7420"), None));
        let public = "192.0.2.1".parse().ok();
        assert!(!host_is_local(Some("192.0.2.1:7420"), public));
    }

    fn post(origin: Option<&str>, fetch_site: Option<&str>) -> HttpRequest {
        HttpRequest {
            method: "POST".to_string(),
            path: "/api/view/open".to_string(),
            query: BTreeMap::new(),
            host: Some("127.0.0.1:7317".to_string()),
            cookie: None,
            accept_encoding: None,
            origin: origin.map(str::to_string),
            fetch_site: fetch_site.map(str::to_string),
        }
    }

    #[test]
    fn a_state_change_from_another_page_is_refused() {
        assert!(post(None, None).cross_origin_refusal().is_none());
        assert!(
            post(Some("http://127.0.0.1:7317"), None)
                .cross_origin_refusal()
                .is_none()
        );
        assert!(
            post(Some("http://127.0.0.1:7317"), Some("same-origin"))
                .cross_origin_refusal()
                .is_none()
        );
        for (origin, site) in [
            (Some("http://127.0.0.1:3000"), None),
            (Some("http://localhost:7317"), None),
            (Some("null"), None),
            (None, Some("same-site")),
            (None, Some("cross-site")),
            (Some("http://127.0.0.1:7317"), Some("same-site")),
        ] {
            let refused = post(origin, site).cross_origin_refusal();
            assert_eq!(
                refused.map(|response| response.status),
                Some(403),
                "{origin:?} {site:?}"
            );
        }
    }
}
