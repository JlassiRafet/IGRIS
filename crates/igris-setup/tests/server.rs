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
    let mut raw = format!("{method} {path} HTTP/1.1\r\nConnection: close\r\n");
    for (k, v) in headers {
        raw.push_str(&format!("{k}: {v}\r\n"));
    }
    if method == "POST" {
        raw.push_str(&format!("Content-Length: {}\r\n", body.len()));
    }
    raw.push_str("\r\n");
    raw.push_str(body);
    let mut s = TcpStream::connect(("127.0.0.1", port)).unwrap();
    s.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
    s.write_all(raw.as_bytes()).unwrap();
    let mut buf = Vec::new();
    s.read_to_end(&mut buf).ok();
    let text = String::from_utf8_lossy(&buf).to_string();
    let (head, body) = text.split_once("\r\n\r\n").unwrap_or((&text, ""));
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
fn a_request_resets_the_idle_clock() {
    let (port, _t, rx) = start(Duration::from_millis(600));
    let h = host(port);
    thread::sleep(Duration::from_millis(400));
    assert_eq!(send(port, "GET", "/", &[("Host", &h)], "").status, 200);
    thread::sleep(Duration::from_millis(400));
    assert_eq!(
        send(port, "GET", "/", &[("Host", &h)], "").status,
        200,
        "still serving 800 ms in"
    );
    let exit = rx
        .recv_timeout(Duration::from_secs(3))
        .expect("exits")
        .unwrap();
    assert!(matches!(exit, Exit::Idle));
}
