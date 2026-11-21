//! The local server under the abuse a loopback port actually gets: idle
//! sockets, pages on other ports of the same host, cookies left by ended
//! sessions and clients that hang up on a read.

use std::sync::Arc;
use std::time::{Duration, Instant};

use kmp_embedded::EmbeddedKernel;
use kmp_viewer::{MemoryViewerServer, bind_loopback};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;

struct Viewer {
    port: u16,
    cookie: String,
    set_cookie: String,
    _kernel: EmbeddedKernel,
    _data_dir: tempfile::TempDir,
}

async fn start(limits: Option<(usize, Duration)>) -> Viewer {
    let data_dir = tempfile::tempdir().expect("temp data dir");
    let kernel = EmbeddedKernel::open(data_dir.path()).expect("kernel opens");
    let mut server = MemoryViewerServer::new(kernel.service(), None).expect("viewer capability");
    if let Some((connections, head_timeout)) = limits {
        server = server.with_connection_limits(connections, head_timeout);
    }
    let server = Arc::new(server);
    let listener = bind_loopback("127.0.0.1:0").await.expect("ephemeral bind");
    let port = listener.local_addr().expect("local addr").port();
    let invitation = server.capability_url(&format!("http://127.0.0.1:{port}/"));
    tokio::spawn(server.serve(listener));
    let path = invitation
        .strip_prefix(&format!("http://127.0.0.1:{port}"))
        .expect("invitation names this listener")
        .to_string();
    let bootstrap = send(
        port,
        &format!("GET {path} HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nConnection: close\r\n\r\n"),
    )
    .await;
    assert!(bootstrap.starts_with("HTTP/1.1 303"), "{bootstrap}");
    let set_cookie = header(&bootstrap, "Set-Cookie").expect("bootstrap sets the cookie");
    let cookie = set_cookie
        .split(';')
        .next()
        .expect("cookie pair")
        .to_string();
    Viewer {
        port,
        cookie,
        set_cookie,
        _kernel: kernel,
        _data_dir: data_dir,
    }
}

async fn send(port: u16, request: &str) -> String {
    let mut stream = TcpStream::connect(("127.0.0.1", port))
        .await
        .expect("connects");
    stream
        .write_all(request.as_bytes())
        .await
        .expect("request writes");
    let mut raw = Vec::new();
    stream.read_to_end(&mut raw).await.expect("response reads");
    String::from_utf8_lossy(&raw).to_string()
}

fn header(response: &str, name: &str) -> Option<String> {
    let head = response.split("\r\n\r\n").next()?;
    head.lines().find_map(|line| {
        let (key, value) = line.split_once(": ")?;
        key.eq_ignore_ascii_case(name).then(|| value.to_string())
    })
}

impl Viewer {
    async fn request(&self, method: &str, path: &str, extra: &str) -> String {
        send(
            self.port,
            &format!(
                "{method} {path} HTTP/1.1\r\nHost: 127.0.0.1:{}\r\nCookie: {}\r\n{extra}Connection: close\r\n\r\n",
                self.port, self.cookie
            ),
        )
        .await
    }
}

/// V-04: a socket needs no capability. A thousand silent ones used to
/// exhaust the descriptors and end the accept loop for the whole session.
#[tokio::test(flavor = "multi_thread")]
async fn idle_sockets_cannot_switch_the_viewer_off() {
    let viewer = start(Some((4, Duration::from_millis(300)))).await;
    let mut idle = Vec::new();
    for _ in 0..24 {
        idle.push(
            TcpStream::connect(("127.0.0.1", viewer.port))
                .await
                .expect("the backlog takes the idle socket"),
        );
    }
    let started = Instant::now();
    let answer = tokio::time::timeout(
        Duration::from_secs(20),
        viewer.request("GET", "/api/abouts", ""),
    )
    .await
    .expect("a real request is served while idle sockets wait");
    assert!(answer.starts_with("HTTP/1.1 200"), "{answer}");
    assert!(started.elapsed() < Duration::from_secs(20));
    // An idle socket is told it timed out and closed.
    let mut first = idle.remove(0);
    let mut raw = Vec::new();
    first
        .read_to_end(&mut raw)
        .await
        .expect("timed-out socket reads");
    assert!(
        String::from_utf8_lossy(&raw).starts_with("HTTP/1.1 408"),
        "{}",
        String::from_utf8_lossy(&raw)
    );
    drop(idle);
    let after = viewer.request("GET", "/api/abouts", "").await;
    assert!(after.starts_with("HTTP/1.1 200"), "{after}");
}

/// V-03 guard: a client that hangs up gives back its slot at once, instead
/// of keeping it, and the work behind it, for the rest of the read.
#[tokio::test(flavor = "multi_thread")]
async fn a_client_that_hangs_up_frees_its_slot() {
    let viewer = start(Some((1, Duration::from_secs(5)))).await;
    let opened = viewer
        .request("POST", "/api/view/open?id=hangup&about=about:hangup", "")
        .await;
    assert!(opened.starts_with("HTTP/1.1 200"), "{opened}");
    // A long poll waits twenty seconds for a change that never comes.
    let mut poll = TcpStream::connect(("127.0.0.1", viewer.port))
        .await
        .expect("connects");
    poll.write_all(
        format!(
            "GET /api/view?id=hangup&since=999 HTTP/1.1\r\nHost: 127.0.0.1:{}\r\nCookie: {}\r\nConnection: close\r\n\r\n",
            viewer.port, viewer.cookie
        )
        .as_bytes(),
    )
    .await
    .expect("poll writes");
    tokio::time::sleep(Duration::from_millis(200)).await;
    drop(poll);
    let started = Instant::now();
    let answer = viewer.request("GET", "/api/abouts", "").await;
    assert!(answer.starts_with("HTTP/1.1 200"), "{answer}");
    assert!(
        started.elapsed() < Duration::from_secs(5),
        "the abandoned poll held the only slot for {:?}",
        started.elapsed()
    );
}

/// V-05: cookies and "same site" ignore the port, so a page served from
/// another port of this host could POST to the view with the cookie.
#[tokio::test(flavor = "multi_thread")]
async fn a_post_from_another_page_on_this_host_is_refused() {
    let viewer = start(None).await;
    let own = format!("Origin: http://127.0.0.1:{}\r\n", viewer.port);
    for (id, extra) in [
        (
            "foreign-port",
            "Origin: http://127.0.0.1:3000\r\n".to_string(),
        ),
        ("sandboxed", "Origin: null\r\n".to_string()),
        ("same-site", "Sec-Fetch-Site: same-site\r\n".to_string()),
        ("cross-site", format!("{own}Sec-Fetch-Site: cross-site\r\n")),
    ] {
        let refused = viewer
            .request(
                "POST",
                &format!("/api/view/open?id={id}&about=about:x"),
                &extra,
            )
            .await;
        assert!(refused.starts_with("HTTP/1.1 403"), "{id}: {refused}");
        let absent = viewer
            .request("GET", &format!("/api/view?id={id}"), "")
            .await;
        assert!(
            absent.starts_with("HTTP/1.1 404"),
            "{id} opened a view: {absent}"
        );
    }
    let page = viewer
        .request(
            "POST",
            "/api/view/open?id=own-page&about=about:x",
            &format!("{own}Sec-Fetch-Site: same-origin\r\n"),
        )
        .await;
    assert!(page.starts_with("HTTP/1.1 200"), "{page}");
    // A client that is no page (curl, a script) sends no Origin; it showed
    // the session cookie to get here.
    let script = viewer
        .request("POST", "/api/view/open?id=script&about=about:x", "")
        .await;
    assert!(script.starts_with("HTTP/1.1 200"), "{script}");
}

/// V-15: no page may frame the viewer, and nothing it serves is readable
/// or openable from another origin.
#[tokio::test(flavor = "multi_thread")]
async fn every_answer_forbids_framing_and_cross_origin_use() {
    let viewer = start(None).await;
    for response in [
        viewer.request("GET", "/", "").await,
        viewer.request("GET", "/api/abouts", "").await,
        send(
            viewer.port,
            &format!(
                "GET /api/abouts HTTP/1.1\r\nHost: 127.0.0.1:{}\r\nConnection: close\r\n\r\n",
                viewer.port
            ),
        )
        .await,
    ] {
        let csp = header(&response, "Content-Security-Policy").expect("a CSP");
        for directive in [
            "default-src 'none'",
            "frame-ancestors 'none'",
            "base-uri 'none'",
            "form-action 'none'",
        ] {
            assert!(csp.contains(directive), "{directive} missing: {csp}");
        }
        assert_eq!(
            header(&response, "X-Frame-Options").as_deref(),
            Some("DENY")
        );
        assert_eq!(
            header(&response, "Cross-Origin-Resource-Policy").as_deref(),
            Some("same-origin")
        );
        assert_eq!(
            header(&response, "Cross-Origin-Opener-Policy").as_deref(),
            Some("same-origin")
        );
    }
}

/// V-11: one cookie per port, renewed by use and expiring on its own, so
/// ended sessions cannot pile up until every request head is too large.
#[tokio::test(flavor = "multi_thread")]
async fn the_session_cookie_is_named_by_port_and_expires_unused() {
    let viewer = start(None).await;
    assert!(
        viewer
            .cookie
            .starts_with(&format!("kmp_chronoloom_{}=", viewer.port)),
        "{}",
        viewer.cookie
    );
    assert!(
        viewer.set_cookie.contains("Max-Age=86400"),
        "{}",
        viewer.set_cookie
    );
    let answer = viewer.request("GET", "/api/abouts", "").await;
    let renewed = header(&answer, "Set-Cookie").expect("an authenticated answer renews the cookie");
    assert!(renewed.starts_with(&viewer.cookie), "{renewed}");
    assert!(renewed.contains("HttpOnly; SameSite=Strict; Path=/; Max-Age=86400"));
    let refused = send(
        viewer.port,
        &format!(
            "GET /api/abouts HTTP/1.1\r\nHost: 127.0.0.1:{}\r\nConnection: close\r\n\r\n",
            viewer.port
        ),
    )
    .await;
    assert!(refused.starts_with("HTTP/1.1 401"));
    assert!(header(&refused, "Set-Cookie").is_none(), "{refused}");
}
