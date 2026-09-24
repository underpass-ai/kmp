use std::io::{Read, Write};
use std::net::TcpListener;
use std::thread::JoinHandle;
use std::time::Duration;

use serde_json::Value;

/// One scripted reply: status, extra headers, body, and a delay before
/// answering.
pub(super) struct Reply {
    pub status: u16,
    pub headers: Vec<(&'static str, String)>,
    pub body: String,
    pub delay: Duration,
}

/// Serves the replies in order, one connection each, then disappears. A
/// call beyond the script fails to connect instead of being answered. Each
/// request is returned as its raw header block and JSON body.
pub(super) fn serve(replies: Vec<Reply>) -> (reqwest::Url, JoinHandle<Vec<(String, Value)>>) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("loopback bind");
    let url = reqwest::Url::parse(&format!(
        "http://{}/v1/systemone",
        listener.local_addr().expect("address")
    ))
    .expect("fixture url");
    let handle = std::thread::spawn(move || {
        let mut received = Vec::new();
        for reply in replies {
            let (mut stream, _) = listener.accept().expect("request");
            stream
                .set_read_timeout(Some(Duration::from_secs(20)))
                .expect("timeout");
            let mut data = Vec::new();
            let header_end = loop {
                let mut byte = [0u8; 1];
                stream.read_exact(&mut byte).expect("headers");
                data.push(byte[0]);
                if data.ends_with(b"\r\n\r\n") {
                    break data.len();
                }
            };
            let headers = String::from_utf8_lossy(&data).to_string();
            let length = headers
                .lines()
                .find_map(|line| {
                    line.to_ascii_lowercase()
                        .strip_prefix("content-length:")
                        .map(|n| n.trim().parse::<usize>().expect("length"))
                })
                .expect("content length");
            data.resize(header_end + length, 0);
            stream.read_exact(&mut data[header_end..]).expect("body");
            received.push((
                headers,
                serde_json::from_slice(&data[header_end..]).expect("JSON body"),
            ));
            std::thread::sleep(reply.delay);
            let extra = reply
                .headers
                .iter()
                .map(|(name, value)| format!("{name}: {value}\r\n"))
                .collect::<String>();
            let _ = write!(
                stream,
                "HTTP/1.1 {} X\r\nContent-Type: application/json\r\n{extra}Content-Length: {}\r\nConnection: close\r\n\r\n{}",
                reply.status,
                reply.body.len(),
                reply.body
            );
        }
        received
    });
    (url, handle)
}
