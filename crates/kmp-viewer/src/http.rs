//! A deliberately small HTTP/1.1 surface: parse one GET, write one response,
//! close. The viewer is a local, read-only window over the kernel — it does
//! not need routing frameworks, keep-alive, or bodies, and every dependency
//! it does not take is one the embedded binary does not carry.

use std::collections::BTreeMap;

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;

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
}

#[derive(Debug)]
pub(crate) struct HttpResponse {
    pub(crate) status: u16,
    pub(crate) content_type: &'static str,
    pub(crate) body: Vec<u8>,
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
            body: body.as_bytes().to_vec(),
            headers: Vec::new(),
            omit_body: false,
        }
    }

    pub(crate) fn css(body: &'static str) -> Self {
        Self {
            status: 200,
            content_type: "text/css; charset=utf-8",
            body: body.as_bytes().to_vec(),
            headers: Vec::new(),
            omit_body: false,
        }
    }

    pub(crate) fn javascript(body: &'static str) -> Self {
        Self {
            status: 200,
            content_type: "text/javascript; charset=utf-8",
            body: body.as_bytes().to_vec(),
            headers: Vec::new(),
            omit_body: false,
        }
    }

    pub(crate) fn json<T: serde::Serialize>(value: &T) -> Self {
        match serde_json::to_vec(value) {
            Ok(body) => Self {
                status: 200,
                content_type: "application/json",
                body,
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
            body: body.to_string().into_bytes(),
            headers: Vec::new(),
            omit_body: false,
        }
    }

    pub(crate) fn redirect(location: &'static str) -> Self {
        Self {
            status: 303,
            content_type: "text/plain; charset=utf-8",
            body: Vec::new(),
            headers: vec![("Location", location.to_string())],
            omit_body: false,
        }
    }
}

/// Reads and parses one request head. Returns `Err` with the response the
/// client deserves when the request cannot or should not be served.
pub(crate) async fn read_request(stream: &mut TcpStream) -> Result<HttpRequest, HttpResponse> {
    let mut head = Vec::with_capacity(1024);
    let mut chunk = [0u8; 1024];
    while !contains_head_terminator(&head) {
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

    let head_text = String::from_utf8_lossy(&head);
    let mut lines = head_text.split("\r\n");
    let request_line = lines.next().unwrap_or_default();
    let mut parts = request_line.split_ascii_whitespace();
    let method = parts.next().unwrap_or_default().to_string();
    let target = parts.next().unwrap_or_default();
    if method.is_empty() || target.is_empty() {
        return Err(HttpResponse::error(400, "malformed request line"));
    }

    let (path, raw_query) = match target.split_once('?') {
        Some((path, query)) => (path, query),
        None => (target, ""),
    };

    let mut host = None;
    let mut cookies = Vec::new();
    for line in lines {
        if line.is_empty() {
            break;
        }
        if let Some((name, value)) = line.split_once(':') {
            if name.trim().eq_ignore_ascii_case("host") {
                host = Some(value.trim().to_string());
            } else if name.trim().eq_ignore_ascii_case("cookie") {
                cookies.push(value.trim());
            }
        }
    }

    Ok(HttpRequest {
        method,
        path: percent_decode(path),
        query: parse_query(raw_query),
        host,
        cookie: (!cookies.is_empty()).then(|| cookies.join("; ")),
    })
}

/// Writes the response with the headers every viewer response carries: no
/// caching (the memory moves underneath), no sniffing, and a CSP that keeps
/// the page self-contained — the same stance the UI is built under.
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
         Cache-Control: no-store\r\n\
         X-Content-Type-Options: nosniff\r\n\
         Referrer-Policy: no-referrer\r\n\
         Content-Security-Policy: default-src 'none'; script-src 'self'; \
         style-src 'self'; connect-src 'self'; img-src 'self' data:\r\n",
        response.status,
        reason,
        response.content_type,
        response.body.len(),
    );
    for (name, value) in &response.headers {
        head.push_str(name);
        head.push_str(": ");
        head.push_str(value);
        head.push_str("\r\n");
    }
    head.push_str("\r\n");
    stream.write_all(head.as_bytes()).await?;
    if !response.omit_body {
        stream.write_all(&response.body).await?;
    }
    stream.flush().await
}

/// The viewer answers only to names that resolve to this machine. A browser
/// lured to `evil.example` pointing at 127.0.0.1 (DNS rebinding) sends that
/// name as `Host` — and is refused here.
pub(crate) fn host_is_local(host: Option<&str>) -> bool {
    let Some(host) = host else {
        return false;
    };
    let name = strip_port(host.trim());
    matches!(name, "localhost" | "127.0.0.1" | "[::1]")
}

fn strip_port(host: &str) -> &str {
    if let Some(end) = host.strip_prefix('[').and_then(|_| host.find(']')) {
        return &host[..=end];
    }
    match host.rsplit_once(':') {
        Some((name, port)) if !port.is_empty() && port.bytes().all(|b| b.is_ascii_digit()) => name,
        _ => host,
    }
}

fn contains_head_terminator(head: &[u8]) -> bool {
    head.windows(4).any(|window| window == b"\r\n\r\n")
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
        assert!(host_is_local(Some("localhost:7317")));
        assert!(host_is_local(Some("127.0.0.1")));
        assert!(host_is_local(Some("[::1]:7317")));
        assert!(!host_is_local(Some("evil.example")));
        assert!(!host_is_local(Some("evil.example:7317")));
        assert!(!host_is_local(None));
    }
}
