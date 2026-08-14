use tokio::io::{AsyncBufRead, AsyncBufReadExt, AsyncRead, AsyncReadExt};

use kmp_ports::PortError;

#[derive(Debug, PartialEq, Eq)]
pub(crate) enum RespValue {
    SimpleString(String),
    BulkString(Option<String>),
    Integer(i64),
    Error(String),
}

pub(crate) fn encode_set_command(key: &str, payload: &str, ttl_seconds: Option<u64>) -> Vec<u8> {
    let mut arguments = vec!["SET".to_string(), key.to_string(), payload.to_string()];
    if let Some(ttl_seconds) = ttl_seconds {
        arguments.push("EX".to_string());
        arguments.push(ttl_seconds.to_string());
    }

    encode_command(&arguments.iter().map(String::as_str).collect::<Vec<_>>())
}

pub(crate) fn encode_command(arguments: &[&str]) -> Vec<u8> {
    let mut command = Vec::new();
    command.extend_from_slice(format!("*{}\r\n", arguments.len()).as_bytes());
    for argument in arguments {
        command.extend_from_slice(format!("${}\r\n", argument.len()).as_bytes());
        command.extend_from_slice(argument.as_bytes());
        command.extend_from_slice(b"\r\n");
    }

    command
}

pub(crate) async fn read_response<R>(reader: &mut R) -> Result<RespValue, PortError>
where
    R: AsyncBufRead + AsyncRead + Unpin,
{
    let mut line = String::new();
    reader.read_line(&mut line).await.map_err(|error| {
        PortError::Unavailable(format!("failed to read valkey response: {error}"))
    })?;
    let line = line.trim_end_matches("\r\n");
    let (prefix, remainder) = line.split_at(1);

    match prefix {
        "+" => Ok(RespValue::SimpleString(remainder.to_string())),
        "-" => Ok(RespValue::Error(remainder.to_string())),
        ":" => remainder
            .parse::<i64>()
            .map(RespValue::Integer)
            .map_err(|error| {
                PortError::Unavailable(format!("invalid valkey integer response: {error}"))
            }),
        "$" => {
            let length = remainder.parse::<isize>().map_err(|error| {
                PortError::Unavailable(format!("invalid valkey bulk string response: {error}"))
            })?;
            if length == -1 {
                return Ok(RespValue::BulkString(None));
            }

            let mut buffer = vec![0_u8; length as usize];
            reader.read_exact(&mut buffer).await.map_err(|error| {
                PortError::Unavailable(format!(
                    "failed to read valkey bulk string payload: {error}"
                ))
            })?;
            let mut crlf = [0_u8; 2];
            reader.read_exact(&mut crlf).await.map_err(|error| {
                PortError::Unavailable(format!(
                    "failed to read valkey bulk string terminator: {error}"
                ))
            })?;

            let payload = String::from_utf8(buffer).map_err(|error| {
                PortError::Unavailable(format!("invalid valkey bulk string payload: {error}"))
            })?;
            Ok(RespValue::BulkString(Some(payload)))
        }
        other => Err(PortError::Unavailable(format!(
            "unsupported valkey RESP prefix `{other}`"
        ))),
    }
}

/// Read an MGET response: a RESP array of bulk strings.
pub(crate) async fn read_bulk_string_array<R>(
    reader: &mut R,
) -> Result<Vec<Option<String>>, PortError>
where
    R: AsyncBufRead + AsyncRead + Unpin,
{
    let mut header = String::new();
    reader.read_line(&mut header).await.map_err(|error| {
        PortError::Unavailable(format!("failed to read valkey array header: {error}"))
    })?;
    let header = header.trim_end_matches("\r\n");
    let (prefix, remainder) = header.split_at(1);

    if prefix == "-" {
        return Err(PortError::Unavailable(format!(
            "valkey rejected read: {remainder}"
        )));
    }

    if prefix != "*" {
        return Err(PortError::Unavailable(format!(
            "expected valkey array response, got prefix `{prefix}`"
        )));
    }

    let count = remainder
        .parse::<isize>()
        .map_err(|error| PortError::Unavailable(format!("invalid valkey array length: {error}")))?;

    if count <= 0 {
        return Ok(Vec::new());
    }

    let mut results = Vec::with_capacity(count as usize);
    for _ in 0..count {
        match read_response(reader).await? {
            RespValue::BulkString(payload) => results.push(payload),
            RespValue::Error(message) => {
                return Err(PortError::Unavailable(format!(
                    "valkey rejected read in array: {message}"
                )));
            }
            other => {
                return Err(PortError::Unavailable(format!(
                    "unexpected element in valkey array: {other:?}"
                )));
            }
        }
    }

    Ok(results)
}

pub(crate) fn map_valkey_response(response: RespValue) -> Result<(), PortError> {
    match response {
        RespValue::SimpleString(message) if message == "OK" => Ok(()),
        RespValue::Error(message) => Err(PortError::Unavailable(format!(
            "valkey rejected write: {message}"
        ))),
        other => Err(PortError::Unavailable(format!(
            "unexpected valkey response: {other:?}"
        ))),
    }
}

#[cfg(test)]
mod tests {
    use tokio::io::BufReader;

    use super::{
        RespValue, encode_command, encode_set_command, map_valkey_response, read_bulk_string_array,
        read_response,
    };
    use kmp_ports::PortError;

    #[test]
    fn encoders_support_plain_and_expiring_commands() {
        let plain =
            String::from_utf8(encode_command(&["GET", "node-123"])).expect("resp must be utf8");
        let expiring = String::from_utf8(encode_set_command("node-123", "payload", Some(60)))
            .expect("resp must be utf8");

        assert_eq!(plain, "*2\r\n$3\r\nGET\r\n$8\r\nnode-123\r\n");
        assert_eq!(
            expiring,
            "*5\r\n$3\r\nSET\r\n$8\r\nnode-123\r\n$7\r\npayload\r\n$2\r\nEX\r\n$2\r\n60\r\n"
        );
    }

    #[tokio::test]
    async fn read_response_supports_simple_integer_and_bulk_values() {
        assert_eq!(
            read_from_buffer(b"+OK\r\n")
                .await
                .expect("simple strings should parse"),
            RespValue::SimpleString("OK".to_string())
        );
        assert_eq!(
            read_from_buffer(b":42\r\n")
                .await
                .expect("integers should parse"),
            RespValue::Integer(42)
        );
        assert_eq!(
            read_from_buffer(b"$11\r\nhello world\r\n")
                .await
                .expect("bulk strings should parse"),
            RespValue::BulkString(Some("hello world".to_string()))
        );
        assert_eq!(
            read_from_buffer(b"$-1\r\n")
                .await
                .expect("nil bulk strings should parse"),
            RespValue::BulkString(None)
        );
    }

    #[tokio::test]
    async fn read_response_surfaces_invalid_frames() {
        let invalid_prefix = read_from_buffer(b"!wat\r\n")
            .await
            .expect_err("unsupported prefixes must fail");
        let invalid_integer = read_from_buffer(b":NaN\r\n")
            .await
            .expect_err("invalid integers must fail");

        assert_eq!(
            invalid_prefix,
            PortError::Unavailable("unsupported valkey RESP prefix `!`".to_string())
        );
        assert!(
            invalid_integer
                .to_string()
                .starts_with("invalid valkey integer response:")
        );
    }

    #[test]
    fn map_valkey_response_accepts_ok_and_rejects_other_payloads() {
        assert!(map_valkey_response(RespValue::SimpleString("OK".to_string())).is_ok());
        assert_eq!(
            map_valkey_response(RespValue::BulkString(Some("payload".to_string())))
                .expect_err("unexpected responses must fail"),
            PortError::Unavailable(
                "unexpected valkey response: BulkString(Some(\"payload\"))".to_string()
            )
        );
    }

    async fn read_from_buffer(payload: &[u8]) -> Result<RespValue, PortError> {
        let mut reader = BufReader::new(payload);
        read_response(&mut reader).await
    }

    #[tokio::test]
    async fn read_bulk_string_array_parses_mget_response() {
        let raw = b"*3\r\n$5\r\nhello\r\n$-1\r\n$5\r\nworld\r\n";
        let mut reader = BufReader::new(&raw[..]);
        let result = read_bulk_string_array(&mut reader)
            .await
            .expect("mget response should parse");

        assert_eq!(
            result,
            vec![Some("hello".to_string()), None, Some("world".to_string()),]
        );
    }

    #[tokio::test]
    async fn read_bulk_string_array_handles_empty_array() {
        let raw = b"*0\r\n";
        let mut reader = BufReader::new(&raw[..]);
        let result = read_bulk_string_array(&mut reader)
            .await
            .expect("empty array should parse");

        assert!(result.is_empty());
    }

    #[tokio::test]
    async fn read_bulk_string_array_handles_all_nil() {
        let raw = b"*2\r\n$-1\r\n$-1\r\n";
        let mut reader = BufReader::new(&raw[..]);
        let result = read_bulk_string_array(&mut reader)
            .await
            .expect("all-nil array should parse");

        assert_eq!(result, vec![None, None]);
    }

    #[tokio::test]
    async fn read_bulk_string_array_rejects_error_response() {
        let raw = b"-ERR no such key\r\n";
        let mut reader = BufReader::new(&raw[..]);
        let error = read_bulk_string_array(&mut reader)
            .await
            .expect_err("error response should fail");

        assert!(error.to_string().contains("no such key"));
    }
}
