use axum::http::{HeaderMap, StatusCode};
use serde_json::{Map, Value, json};

pub const CURRENT_PROTOCOL_VERSION: &str = "2026-07-28";
pub const PROTOCOL_VERSION_HEADER: &str = "mcp-protocol-version";
pub const METHOD_HEADER: &str = "mcp-method";
pub const NAME_HEADER: &str = "mcp-name";

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProtocolError {
    pub status: StatusCode,
    pub code: i64,
    pub message: String,
}

impl ProtocolError {
    fn bad_request(code: i64, message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::BAD_REQUEST,
            code,
            message: message.into(),
        }
    }

    fn not_found(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::NOT_FOUND,
            code: -32601,
            message: message.into(),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RequestDialect {
    Legacy,
    Current,
}

pub fn validate_request(
    headers: &HeaderMap,
    request: &Value,
) -> Result<RequestDialect, ProtocolError> {
    let object = request.as_object().ok_or_else(|| {
        ProtocolError::bad_request(-32600, "JSON-RPC batch and scalar requests are unsupported")
    })?;
    if object.get("jsonrpc").and_then(Value::as_str) != Some("2.0") {
        return Err(ProtocolError::bad_request(
            -32600,
            "request must declare jsonrpc 2.0",
        ));
    }
    let method = object
        .get("method")
        .and_then(Value::as_str)
        .ok_or_else(|| ProtocolError::bad_request(-32600, "missing JSON-RPC method"))?;
    if !matches!(
        method,
        "initialize" | "notifications/initialized" | "tools/list" | "tools/call"
    ) {
        return Err(ProtocolError::not_found(format!(
            "unsupported JSON-RPC method `{method}`"
        )));
    }

    let Some(version) = header(headers, PROTOCOL_VERSION_HEADER)? else {
        return Ok(RequestDialect::Legacy);
    };
    if version != CURRENT_PROTOCOL_VERSION {
        return Err(ProtocolError::bad_request(
            -32020,
            format!(
                "unsupported MCP protocol version `{version}`; supported versions: {CURRENT_PROTOCOL_VERSION}"
            ),
        ));
    }
    let metadata = object
        .get("params")
        .and_then(Value::as_object)
        .and_then(|params| params.get("_meta"))
        .and_then(Value::as_object)
        .ok_or_else(|| {
            ProtocolError::bad_request(-32602, "current MCP requests require params._meta")
        })?;
    if metadata
        .get("io.modelcontextprotocol/protocolVersion")
        .and_then(Value::as_str)
        != Some(CURRENT_PROTOCOL_VERSION)
    {
        return Err(ProtocolError::bad_request(
            -32020,
            "MCP-Protocol-Version must match params._meta protocolVersion",
        ));
    }
    if !metadata
        .get("io.modelcontextprotocol/clientCapabilities")
        .is_some_and(Value::is_object)
    {
        return Err(ProtocolError::bad_request(
            -32602,
            "current MCP requests require clientCapabilities",
        ));
    }
    if header(headers, METHOD_HEADER)?.as_deref() != Some(method) {
        return Err(ProtocolError::bad_request(
            -32020,
            "Mcp-Method must match the JSON-RPC method",
        ));
    }
    if method == "tools/call" {
        let body_name = object
            .get("params")
            .and_then(Value::as_object)
            .and_then(|params| params.get("name"))
            .and_then(Value::as_str)
            .ok_or_else(|| ProtocolError::bad_request(-32602, "tools/call requires params.name"))?;
        if header(headers, NAME_HEADER)?.as_deref() != Some(body_name) {
            return Err(ProtocolError::bad_request(
                -32020,
                "Mcp-Name must match params.name",
            ));
        }
    }
    validate_accept(headers)?;
    Ok(RequestDialect::Current)
}

pub fn jsonrpc_error(id: Value, code: i64, message: &str) -> Value {
    json!({
        "jsonrpc": "2.0",
        "id": id,
        "error": {"code": code, "message": message}
    })
}

pub fn add_current_response_metadata(mut response: Value) -> Value {
    let Some(result) = response.get_mut("result").and_then(Value::as_object_mut) else {
        return response;
    };
    if result.contains_key("protocolVersion") {
        result.insert(
            "protocolVersion".to_string(),
            Value::String(CURRENT_PROTOCOL_VERSION.to_string()),
        );
    }
    let metadata = result
        .entry("_meta")
        .or_insert_with(|| Value::Object(Map::new()));
    if !metadata.is_object() {
        *metadata = Value::Object(Map::new());
    }
    metadata
        .as_object_mut()
        .expect("metadata is an object")
        .insert(
            "io.modelcontextprotocol/serverInfo".to_string(),
            json!({
                "name": "underpass-kmp-mcp-http",
                "version": env!("CARGO_PKG_VERSION")
            }),
        );
    response
}

fn validate_accept(headers: &HeaderMap) -> Result<(), ProtocolError> {
    let value = header(headers, "accept")?.unwrap_or_default();
    let accepts_json = value
        .split(',')
        .any(|part| part.trim().starts_with("application/json"));
    let accepts_stream = value
        .split(',')
        .any(|part| part.trim().starts_with("text/event-stream"));
    if accepts_json && accepts_stream {
        Ok(())
    } else {
        Err(ProtocolError::bad_request(
            -32020,
            "current MCP requests must accept application/json and text/event-stream",
        ))
    }
}

fn header(headers: &HeaderMap, name: &str) -> Result<Option<String>, ProtocolError> {
    headers
        .get(name)
        .map(|value| {
            value
                .to_str()
                .map(str::to_string)
                .map_err(|_| ProtocolError::bad_request(-32020, format!("invalid {name} header")))
        })
        .transpose()
}

#[cfg(test)]
mod tests {
    use axum::http::HeaderValue;

    use super::*;

    fn current_headers(method: &str) -> HeaderMap {
        let mut headers = HeaderMap::new();
        headers.insert(
            PROTOCOL_VERSION_HEADER,
            HeaderValue::from_static(CURRENT_PROTOCOL_VERSION),
        );
        headers.insert(
            METHOD_HEADER,
            HeaderValue::from_str(method).expect("method header"),
        );
        headers.insert(
            "accept",
            HeaderValue::from_static("application/json, text/event-stream"),
        );
        headers
    }

    fn request(method: &str) -> Value {
        json!({
            "jsonrpc":"2.0", "id":1, "method":method,
            "params":{"_meta":{
                "io.modelcontextprotocol/protocolVersion":CURRENT_PROTOCOL_VERSION,
                "io.modelcontextprotocol/clientCapabilities":{}
            }}
        })
    }

    #[test]
    fn current_headers_and_body_must_agree() {
        let headers = current_headers("tools/list");
        assert_eq!(
            validate_request(&headers, &request("tools/list")),
            Ok(RequestDialect::Current)
        );
        let mismatch = validate_request(&headers, &request("initialize")).expect_err("mismatch");
        assert_eq!(mismatch.code, -32020);
    }

    #[test]
    fn missing_version_header_uses_legacy_compatibility() {
        let headers = HeaderMap::new();
        let legacy = json!({"jsonrpc":"2.0","id":1,"method":"tools/list","params":{}});
        assert_eq!(
            validate_request(&headers, &legacy),
            Ok(RequestDialect::Legacy)
        );
    }
}
