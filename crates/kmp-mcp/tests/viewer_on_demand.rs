//! A stdio session listens on nothing until someone asks for the viewer.
//! The first `kmp_view_open` binds it; every later request reuses it.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpStream;
use std::process::{Child, ChildStdout, Command, Stdio};

use serde_json::{Value, json};

const ABOUT: &str = "test:viewer-on-demand";

struct Session {
    child: Child,
    stdout: BufReader<ChildStdout>,
    next_id: u64,
    _data: tempfile::TempDir,
    _home: tempfile::TempDir,
}

impl Session {
    fn start(viewer_addr: Option<&str>) -> Self {
        let data = tempfile::tempdir().expect("store dir");
        let home = tempfile::tempdir().expect("home dir");
        let mut command = Command::new(env!("CARGO_BIN_EXE_kmp-mcp"));
        command
            .env("KMP_MCP_DATA_DIR", data.path())
            .env("HOME", home.path())
            .env("XDG_DATA_HOME", home.path().join("data"))
            .env("XDG_CONFIG_HOME", home.path().join("config"))
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null());
        match viewer_addr {
            Some(addr) => command.env("KMP_VIEWER_ADDR", addr),
            None => command.env_remove("KMP_VIEWER_ADDR"),
        };
        let mut child = command.spawn().expect("stdio session starts");
        let stdout = BufReader::new(child.stdout.take().expect("stdout"));
        Self {
            child,
            stdout,
            next_id: 1,
            _data: data,
            _home: home,
        }
    }

    fn call(&mut self, method: &str, params: Value) -> Value {
        let request =
            json!({"jsonrpc": "2.0", "id": self.next_id, "method": method, "params": params});
        self.next_id += 1;
        let stdin = self.child.stdin.as_mut().expect("stdin");
        writeln!(stdin, "{request}").expect("request written");
        stdin.flush().expect("request flushed");
        let mut line = String::new();
        self.stdout.read_line(&mut line).expect("response read");
        serde_json::from_str(&line).unwrap_or_else(|error| panic!("{line:?}: {error}"))
    }

    fn tool(&mut self, name: &str, arguments: Value) -> Value {
        let response = self.call("tools/call", json!({"name": name, "arguments": arguments}));
        let result = &response["result"];
        assert!(
            response.get("error").is_none() && result["isError"] != json!(true),
            "{name} failed: {response}"
        );
        result
            .get("structuredContent")
            .cloned()
            .or_else(|| {
                result["content"][0]["text"]
                    .as_str()
                    .and_then(|text| serde_json::from_str(text).ok())
            })
            .unwrap_or_else(|| panic!("{name} returned no structured result: {response}"))
    }

    fn seed_and_wake(&mut self) {
        self.call(
            "initialize",
            json!({"protocolVersion": "2025-06-18", "capabilities": {}, "clientInfo": {"name": "test", "version": "0"}}),
        );
        assert!(self.call("tools/list", json!({}))["result"]["tools"].is_array());
        self.tool(
            "kmp_ingest",
            json!({
                "about": ABOUT,
                "idempotency_key": "viewer-on-demand-seed",
                "memory": {
                    "dimensions": [{"id": "timeline:test", "kind": "timeline"}],
                    "entries": [{
                        "id": format!("{ABOUT}:observation:seed"),
                        "kind": "observation",
                        "text": "a memory worth looking at",
                        "coordinates": [{"dimension": "timeline", "scope_id": "timeline:test", "sequence": 1}]
                    }]
                }
            }),
        );
        self.tool("kmp_wake", json!({"about": ABOUT}));
    }

    fn open_view(&mut self) -> String {
        let opened = self.tool("kmp_view_open", json!({"about": ABOUT}));
        assert_eq!(opened["viewer_available"], json!(true), "{opened}");
        opened["url"]
            .as_str()
            .expect("the open hands out a link")
            .to_string()
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        drop(self.child.stdin.take());
        let _ = self.child.wait();
    }
}

fn free_loopback_port() -> u16 {
    std::net::TcpListener::bind("127.0.0.1:0")
        .expect("a free port")
        .local_addr()
        .expect("its address")
        .port()
}

fn first_answer(url: &str) -> String {
    let rest = url.strip_prefix("http://").expect("an http link");
    let (host, path) = rest.split_once('/').expect("a path");
    let mut stream = TcpStream::connect(host).expect("the viewer listens");
    write!(
        stream,
        "GET /{path} HTTP/1.1\r\nHost: {host}\r\nConnection: close\r\n\r\n"
    )
    .expect("request");
    let mut answer = String::new();
    stream.read_to_string(&mut answer).expect("answer");
    answer
}

#[test]
fn the_viewer_listens_only_after_it_is_requested_and_then_stays() {
    let port = free_loopback_port();
    let addr = format!("127.0.0.1:{port}");
    let mut session = Session::start(Some(&addr));
    session.seed_and_wake();
    assert!(
        TcpStream::connect(&addr).is_err(),
        "nothing may listen on {addr} before the viewer is requested"
    );

    let url = session.open_view();
    assert!(url.starts_with(&format!("http://{addr}/?k=")), "{url}");
    let answer = first_answer(&url);
    assert!(
        answer.starts_with("HTTP/1.1 303"),
        "the link works: {answer}"
    );

    let again = session.open_view();
    assert_eq!(
        again, url,
        "a second open reuses the listener and its capability"
    );
    let state = session.tool("kmp_view_get_state", json!({}));
    assert_eq!(state["url"].as_str(), Some(url.as_str()));
}

#[test]
fn reading_the_view_state_does_not_start_the_viewer() {
    let port = free_loopback_port();
    let addr = format!("127.0.0.1:{port}");
    let mut session = Session::start(Some(&addr));
    session.seed_and_wake();
    // A view opened by another face of the process (the browser report, an
    // MCP App) is readable without a listener.
    let missing = session.call(
        "tools/call",
        json!({"name": "kmp_view_get_state", "arguments": {"view_id": "nobody"}}),
    );
    assert!(missing.get("result").is_some() || missing.get("error").is_some());
    assert!(
        TcpStream::connect(&addr).is_err(),
        "get_state started the viewer"
    );
}

#[test]
fn a_busy_named_address_comes_back_as_a_reason_not_a_crash() {
    let occupied = std::net::TcpListener::bind("127.0.0.1:0").expect("occupy a port");
    let addr = occupied.local_addr().expect("address").to_string();
    let mut session = Session::start(Some(&addr));
    session.seed_and_wake();
    let opened = session.tool("kmp_view_open", json!({"about": ABOUT}));
    assert_eq!(opened["viewer_available"], json!(false), "{opened}");
    assert!(opened.get("url").is_none(), "{opened}");
    let reason = opened["unhonored"][0].as_str().unwrap_or_default();
    assert!(
        reason.contains(&addr) && reason.contains("could not bind"),
        "the result names the address and why: {opened}"
    );
}

#[test]
fn a_busy_default_address_falls_forward_to_a_session_port_on_first_use() {
    let occupied = match std::net::TcpListener::bind(kmp_viewer::DEFAULT_VIEWER_ADDR) {
        Ok(listener) => Some(listener),
        Err(error) if error.kind() == std::io::ErrorKind::AddrInUse => None,
        Err(error) => panic!("the default viewer port could not be occupied: {error}"),
    };
    let mut session = Session::start(None);
    session.seed_and_wake();
    let url = session.open_view();
    drop(occupied);
    assert!(
        url.starts_with("http://127.0.0.1:") && !url.starts_with("http://127.0.0.1:7317/"),
        "the session advertises its own port: {url}"
    );
    assert!(first_answer(&url).starts_with("HTTP/1.1 303"));
}

#[test]
fn a_named_address_that_is_not_loopback_is_refused_at_start() {
    let data = tempfile::tempdir().expect("store dir");
    let home = tempfile::tempdir().expect("home dir");
    let output = Command::new(env!("CARGO_BIN_EXE_kmp-mcp"))
        .env("KMP_VIEWER_ADDR", "0.0.0.0:7400")
        .env("KMP_MCP_DATA_DIR", data.path())
        .env("HOME", home.path())
        .env("XDG_DATA_HOME", home.path().join("data"))
        .env("XDG_CONFIG_HOME", home.path().join("config"))
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .output()
        .expect("binary runs");
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("0.0.0.0:7400") && stderr.contains("loopback"),
        "{stderr}"
    );
}
