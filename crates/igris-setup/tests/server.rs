//! Raw-socket tests: real bytes over 127.0.0.1 against a running `App`.

use igris_detect::Host;
use igris_setup::{App, Exit, SetupError};
use std::io::{Read, Write};
use std::net::TcpStream;
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

struct Stub;
impl Host for Stub {
    fn os(&self) -> &str {
        "linux"
    }
    fn home(&self) -> Option<String> {
        None
    }
    fn cpu_cores(&self) -> Option<u32> {
        None
    }
    fn run(&self, _: &str, _: &[&str]) -> Option<String> {
        None
    }
    fn run_with_stderr(&self, _: &str, _: &[&str]) -> Option<String> {
        None
    }
    fn read(&self, _: &str) -> Option<String> {
        None
    }
    fn which(&self, _: &str) -> Option<String> {
        None
    }
    fn has_env(&self, _: &str) -> bool {
        false
    }
    fn http_get(&self, _: u16, _: &str) -> Option<String> {
        None
    }
}

struct Resp {
    status: u16,
    headers: Vec<(String, String)>,
    body: String,
}

impl Resp {
    fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case(name))
            .map(|(_, v)| v.as_str())
    }
}

/// Send one raw HTTP/1.1 request and parse the answer. Every response in this file is also
/// checked: nothing may ever carry a CORS header.
fn send(port: u16, method: &str, path: &str, headers: &[(&str, &str)], body: &str) -> Resp {
    send_bytes(port, method, path, headers, body.as_bytes())
}

fn send_bytes(port: u16, method: &str, path: &str, headers: &[(&str, &str)], body: &[u8]) -> Resp {
    let mut raw = format!("{method} {path} HTTP/1.1\r\nConnection: close\r\n");
    for (k, v) in headers {
        raw.push_str(&format!("{k}: {v}\r\n"));
    }
    if method == "POST" {
        raw.push_str(&format!("Content-Length: {}\r\n", body.len()));
    }
    raw.push_str("\r\n");
    let mut bytes = raw.into_bytes();
    bytes.extend_from_slice(body);
    let mut s = TcpStream::connect(("127.0.0.1", port)).unwrap();
    s.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
    s.write_all(&bytes).unwrap();
    let mut buf = Vec::new();
    s.read_to_end(&mut buf).ok();
    let split = buf
        .windows(4)
        .position(|w| w == b"\r\n\r\n")
        .map_or(buf.len(), |i| i + 4);
    let head = String::from_utf8_lossy(&buf[..split]).to_string();
    let raw = &buf[split..];
    let body_bytes = if head
        .to_ascii_lowercase()
        .contains("transfer-encoding: chunked")
    {
        dechunk(raw)
    } else {
        raw.to_vec()
    };
    let body = String::from_utf8_lossy(&body_bytes).to_string();
    let (head, body) = (head.as_str(), body.as_str());
    let mut lines = head.lines();
    let status = lines
        .next()
        .and_then(|l| l.split_whitespace().nth(1))
        .and_then(|c| c.parse().ok())
        .unwrap_or(0);
    let headers: Vec<(String, String)> = lines
        .filter_map(|l| l.split_once(':'))
        .map(|(k, v)| (k.trim().to_string(), v.trim().to_string()))
        .collect();
    assert!(
        headers
            .iter()
            .all(|(k, _)| !k.to_ascii_lowercase().starts_with("access-control-")),
        "CORS header in {headers:?}"
    );
    Resp {
        status,
        headers,
        body: body.to_string(),
    }
}

/// Decode a chunked body (hex size line, data, CRLF, repeated, ending with a zero chunk).
fn dechunk(mut raw: &[u8]) -> Vec<u8> {
    let mut out = Vec::new();
    while let Some(eol) = raw.windows(2).position(|w| w == b"\r\n") {
        let size =
            usize::from_str_radix(String::from_utf8_lossy(&raw[..eol]).trim(), 16).unwrap_or(0);
        if size == 0 || raw.len() < eol + 2 + size {
            break;
        }
        out.extend_from_slice(&raw[eol + 2..eol + 2 + size]);
        raw = &raw[(eol + 2 + size + 2).min(raw.len())..];
    }
    out
}

type Running = mpsc::Receiver<Result<Exit, SetupError>>;

/// Start an app; returns its port, its one-time token and a channel that gets `run`'s result.
fn start(idle: Duration) -> (u16, String, Running) {
    let app = App::bind(idle).unwrap();
    let port = app.port();
    let token = app.url().split("#t=").nth(1).unwrap().to_string();
    let (tx, rx) = mpsc::channel();
    thread::spawn(move || tx.send(app.run(&Stub)).ok());
    (port, token, rx)
}

fn host(port: u16) -> String {
    format!("127.0.0.1:{port}")
}

fn origin(port: u16) -> String {
    format!("http://127.0.0.1:{port}")
}

/// Log in with the token; returns the `Cookie` header value for later requests.
fn login(port: u16, token: &str) -> String {
    let (h, o) = (host(port), origin(port));
    let r = send(
        port,
        "POST",
        "/auth",
        &[("Host", &h), ("Origin", &o)],
        &format!(r#"{{"token":"{token}"}}"#),
    );
    assert_eq!(r.status, 200, "{}", r.body);
    let set = r.header("set-cookie").expect("cookie").to_string();
    set.split(';').next().unwrap().to_string()
}

fn api(port: u16, cookie: &str, body: &str) -> Resp {
    let (h, o) = (host(port), origin(port));
    send(
        port,
        "POST",
        "/api/action",
        &[("Host", &h), ("Origin", &o), ("Cookie", cookie)],
        body,
    )
}

const LONG: Duration = Duration::from_secs(60);

#[test]
fn page_loads_with_security_headers() {
    let (port, token, _rx) = start(LONG);
    let h = host(port);
    let r = send(port, "GET", "/", &[("Host", &h)], "");
    assert_eq!(r.status, 200);
    assert_eq!(r.header("cache-control"), Some("no-store"));
    assert_eq!(r.header("x-content-type-options"), Some("nosniff"));
    assert_eq!(r.header("referrer-policy"), Some("no-referrer"));
    assert_eq!(
        r.header("content-security-policy"),
        Some("default-src 'self'; frame-ancestors 'none'")
    );
    assert!(
        !r.body.contains(&token),
        "the page must not carry the token"
    );
}

#[test]
fn wrong_host_is_403() {
    let (port, _t, _rx) = start(LONG);
    for bad in ["evil.com".to_string(), format!("127.0.0.1.evil.com:{port}")] {
        let r = send(port, "GET", "/", &[("Host", &bad)], "");
        assert_eq!(r.status, 403, "{bad}");
    }
}

#[test]
fn options_is_403_and_nothing_has_cors_headers() {
    let (port, _t, _rx) = start(LONG);
    let h = host(port);
    let r = send(
        port,
        "OPTIONS",
        "/api/action",
        &[
            ("Host", &h),
            ("Origin", "http://evil.com"),
            ("Access-Control-Request-Method", "POST"),
        ],
        "",
    );
    assert_eq!(r.status, 403);
}

#[test]
fn auth_sets_the_cookie_once() {
    let (port, token, _rx) = start(LONG);
    let (h, o) = (host(port), origin(port));
    let r = send(
        port,
        "POST",
        "/auth",
        &[("Host", &h), ("Origin", &o)],
        &format!(r#"{{"token":"{token}"}}"#),
    );
    assert_eq!(r.status, 200);
    let set = r.header("set-cookie").unwrap().to_string();
    for part in ["igris_session=", "HttpOnly", "SameSite=Strict", "Path=/"] {
        assert!(set.contains(part), "{set}");
    }
    let cookie = set.split(';').next().unwrap().to_string();

    let again = send(
        port,
        "POST",
        "/auth",
        &[("Host", &h), ("Origin", &o)],
        &format!(r#"{{"token":"{token}"}}"#),
    );
    assert_eq!(again.status, 401);
    assert!(again.header("set-cookie").is_none());

    let r = api(port, &cookie, r#"{"action":"detect"}"#);
    assert_eq!(r.status, 200, "the first session keeps working");
    assert!(r.body.contains("\"machine\""));
}

#[test]
fn check_action_returns_the_view() {
    let (port, token, _rx) = start(LONG);
    let cookie = login(port, &token);
    let r = api(port, &cookie, r#"{"action":"check"}"#);
    assert_eq!(r.status, 200, "{}", r.body);
    let v: serde_json::Value = serde_json::from_str(&r.body).unwrap();
    assert!(v["lead"].is_string() && v["counts"].is_object());
    let ids: Vec<_> = v["groups"]
        .as_array()
        .unwrap()
        .iter()
        .map(|g| g["id"].as_str().unwrap())
        .collect();
    assert_eq!(ids, ["machine", "build", "decision", "chat"]);
}

#[test]
fn setup_files_are_public_with_the_right_types() {
    let (port, _t, _rx) = start(LONG);
    let h = host(port);
    for (path, ctype) in [
        ("/setup.css", "text/css; charset=utf-8"),
        ("/setup.js", "text/javascript; charset=utf-8"),
        ("/palette.js", "text/javascript; charset=utf-8"),
        ("/favicon.svg", "image/svg+xml"),
    ] {
        let r = send(port, "GET", path, &[("Host", &h)], "");
        assert_eq!(r.status, 200, "{path}");
        assert_eq!(r.header("content-type"), Some(ctype), "{path}");
        assert!(!r.body.is_empty(), "{path}");
    }
}

#[test]
fn font_files_are_public_with_the_right_type() {
    let (port, _t, _rx) = start(LONG);
    let h = host(port);
    for name in [
        "JetBrainsMono-Regular.woff2",
        "JetBrainsMono-SemiBold.woff2",
    ] {
        let r = send(port, "GET", &format!("/fonts/{name}"), &[("Host", &h)], "");
        assert_eq!(r.status, 200, "{name}");
        assert_eq!(r.header("content-type"), Some("font/woff2"), "{name}");
        assert!(
            r.body.starts_with("wOF2"),
            "{name} is not a woff2 file, body starts {:?}",
            r.body.chars().take(12).collect::<String>()
        );
    }
    let r = send(port, "GET", "/fonts/other.woff2", &[("Host", &h)], "");
    assert_eq!(r.status, 401, "unknown font paths stay closed");
}

#[test]
fn root_serves_the_setup_page() {
    let (port, _t, _rx) = start(LONG);
    let h = host(port);
    let r = send(port, "GET", "/", &[("Host", &h)], "");
    assert_eq!(r.status, 200);
    assert!(r.body.contains("<title>IGRIS setup"));
    assert!(r.body.contains(r#"id="groups""#));
}

#[test]
fn api_without_cookie_is_401() {
    let (port, _t, _rx) = start(LONG);
    let r = api(port, "igris_session=nope", r#"{"action":"detect"}"#);
    assert_eq!(r.status, 401);
}

#[test]
fn api_with_wrong_origin_is_403() {
    let (port, token, _rx) = start(LONG);
    let cookie = login(port, &token);
    let h = host(port);
    let r = send(
        port,
        "POST",
        "/api/action",
        &[
            ("Host", &h),
            ("Origin", "http://evil.com"),
            ("Cookie", &cookie),
        ],
        r#"{"action":"detect"}"#,
    );
    assert_eq!(r.status, 403);
}

#[test]
fn bad_bodies_get_400_or_413() {
    let (port, token, _rx) = start(LONG);
    let cookie = login(port, &token);
    assert_eq!(api(port, &cookie, "not json").status, 400);
    assert_eq!(api(port, &cookie, r#"{"action":"rm"}"#).status, 400);
    assert_eq!(api(port, &cookie, &"a".repeat(65_537)).status, 413);
}

#[test]
fn api_with_a_wrong_cookie_is_401() {
    let (port, token, _rx) = start(LONG);
    login(port, &token);
    let r = api(port, "igris_session=nope", r#"{"action":"detect"}"#);
    assert_eq!(r.status, 401, "a session exists, but this is not it");
}

#[test]
fn unknown_path_without_cookie_is_401() {
    let (port, _t, _rx) = start(LONG);
    let h = host(port);
    let r = send(port, "GET", "/nope", &[("Host", &h)], "");
    assert_eq!(r.status, 401);
}

#[test]
fn absolute_form_target_without_cookie_is_401() {
    let (port, _t, _rx) = start(LONG);
    let (h, o) = (host(port), origin(port));
    let target = format!("http://127.0.0.1:{port}/api/action");
    let r = send(
        port,
        "POST",
        &target,
        &[("Host", &h), ("Origin", &o)],
        r#"{"action":"detect"}"#,
    );
    assert_eq!(r.status, 401);
}

#[test]
fn duplicate_host_header_is_403() {
    let (port, _t, _rx) = start(LONG);
    let h = host(port);
    let r = send(port, "GET", "/", &[("Host", &h), ("Host", &h)], "");
    assert_eq!(r.status, 403);
}

#[test]
fn non_utf8_body_is_400() {
    let (port, token, _rx) = start(LONG);
    let cookie = login(port, &token);
    let (h, o) = (host(port), origin(port));
    let r = send_bytes(
        port,
        "POST",
        "/api/action",
        &[("Host", &h), ("Origin", &o), ("Cookie", &cookie)],
        &[0xff, 0xfe],
    );
    assert_eq!(r.status, 400);
}

#[test]
fn unknown_path_is_404() {
    let (port, token, _rx) = start(LONG);
    let cookie = login(port, &token);
    let h = host(port);
    let r = send(
        port,
        "GET",
        "/nope",
        &[("Host", &h), ("Cookie", &cookie)],
        "",
    );
    assert_eq!(r.status, 404);
}

#[test]
fn exits_when_idle() {
    let (port, _t, rx) = start(Duration::from_millis(200));
    let exit = rx
        .recv_timeout(Duration::from_secs(2))
        .expect("exits")
        .unwrap();
    assert!(matches!(exit, Exit::Idle));
    let deadline = Instant::now() + Duration::from_secs(2);
    while TcpStream::connect(("127.0.0.1", port)).is_ok() {
        assert!(Instant::now() < deadline, "port still accepting");
        thread::sleep(Duration::from_millis(50));
    }
}

#[test]
fn a_logged_in_request_resets_the_idle_clock() {
    let (port, token, rx) = start(Duration::from_millis(600));
    let cookie = login(port, &token);
    thread::sleep(Duration::from_millis(400));
    assert_eq!(api(port, &cookie, r#"{"action":"detect"}"#).status, 200);
    thread::sleep(Duration::from_millis(400));
    assert_eq!(
        api(port, &cookie, r#"{"action":"detect"}"#).status,
        200,
        "still serving 800 ms in"
    );
    let exit = rx
        .recv_timeout(Duration::from_secs(3))
        .expect("exits")
        .unwrap();
    assert!(matches!(exit, Exit::Idle));
}

#[test]
fn unauthenticated_traffic_cannot_hold_the_server_open() {
    let (port, _t, rx) = start(Duration::from_millis(500));
    let h = host(port);
    let end = Instant::now() + Duration::from_secs(4);
    let mut n = 0;
    while Instant::now() < end {
        if let Ok(exit) = rx.try_recv() {
            assert!(matches!(exit.unwrap(), Exit::Idle));
            return;
        }
        // A page load, a 403 and a 404, every 50 ms: none of them is a logged-in request.
        match n % 3 {
            0 => send(port, "GET", "/", &[("Host", &h)], ""),
            1 => send(port, "GET", "/", &[("Host", "evil.com")], ""),
            _ => send(port, "GET", "/nope", &[("Host", &h)], ""),
        };
        n += 1;
        thread::sleep(Duration::from_millis(50));
    }
    panic!("the server never went idle under unauthenticated traffic");
}

#[test]
fn it_listens_on_loopback_only() {
    let app = App::bind(LONG).unwrap();
    let port = app.port();
    thread::spawn(move || app.run(&Stub));
    // Find this machine's own non-loopback address (no packet is sent by a UDP connect).
    let local = std::net::UdpSocket::bind("0.0.0.0:0")
        .and_then(|s| s.connect("192.0.2.1:9").and_then(|_| s.local_addr()))
        .map(|a| a.ip());
    match local {
        Ok(ip) if !ip.is_loopback() => assert!(
            TcpStream::connect_timeout(&(ip, port).into(), Duration::from_secs(1)).is_err(),
            "reachable on {ip}: the server is not loopback-only"
        ),
        _ => eprintln!("no non-loopback address here; loopback-only check skipped"),
    }
}
