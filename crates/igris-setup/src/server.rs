//! The loopback server. Every request goes through `guard::check` first; only then does a route run.

use crate::SetupError;
use crate::action;
use crate::auth::Auth;
use crate::guard::{ReqMeta, SESSION_COOKIE, Verdict, check, is_public, normalize_path};
use igris_detect::Host;
use serde::Deserialize;
use std::io::Read;
use std::net::SocketAddr;
use std::time::{Duration, Instant};
use tiny_http::{Header, Method, Request, Response, Server};

const MAX_BODY: usize = 64 * 1024;
// The page and its files, from the one shared UI folder (`ui/`, inside this crate).
const PAGE: &str = include_str!("../ui/setup.html");
const CSS: &str = include_str!("../ui/setup.css");
const SETUP_JS: &str = include_str!("../ui/setup.js");
const PALETTE_JS: &str = include_str!("../ui/palette.js");
const FAVICON: &str = include_str!("../ui/favicon.svg");
const FONT_REGULAR: &[u8] = include_bytes!("../ui/fonts/JetBrainsMono-Regular.woff2");
const FONT_SEMIBOLD: &[u8] = include_bytes!("../ui/fonts/JetBrainsMono-SemiBold.woff2");

#[derive(Debug)]
pub enum Exit {
    /// No valid request for the idle limit.
    Idle,
    /// The server stopped on its own account.
    Stopped,
}

pub struct App {
    server: Server,
    addr: SocketAddr,
    auth: Auth,
    idle: Duration,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct AuthBody {
    token: String,
}

impl App {
    /// Bind 127.0.0.1 on a free port. Never 0.0.0.0.
    pub fn bind(idle: Duration) -> Result<App, SetupError> {
        let server = Server::http("127.0.0.1:0")
            .map_err(|e| SetupError::Io(std::io::Error::other(e.to_string())))?;
        let addr = server
            .server_addr()
            .to_ip()
            .filter(|a| a.ip().is_loopback())
            .ok_or_else(|| SetupError::Io(std::io::Error::other("not a loopback address")))?;
        Ok(App {
            server,
            addr,
            auth: Auth::new(Instant::now())?,
            idle,
        })
    }

    pub fn port(&self) -> u16 {
        self.addr.port()
    }

    /// The link to open: the token sits in the fragment, which browsers never send to a server.
    pub fn url(&self) -> String {
        format!("http://{}/#t={}", self.addr, self.auth.token())
    }

    /// Serve until idle. Requests are handled one at a time on this thread.
    pub fn run(mut self, host: &dyn Host) -> Result<Exit, SetupError> {
        loop {
            // Checked on every pass, so steady traffic cannot hold the server open.
            if self.auth.idle(Instant::now(), self.idle) {
                return Ok(Exit::Idle);
            }
            if let Some(request) = self.server.recv_timeout(Duration::from_millis(100))? {
                self.handle(request, host);
            }
        }
    }

    fn handle(&mut self, mut request: Request, host: &dyn Host) {
        let path = normalize_path(request.url()).to_string();
        let verdict = {
            check(
                &ReqMeta {
                    method: request.method().as_str(),
                    path: &path,
                    hosts: values(&request, "Host"),
                    origins: values(&request, "Origin"),
                    cookies: values(&request, "Cookie"),
                },
                self.addr.port(),
                self.auth.session(),
            )
        };
        match verdict {
            Verdict::Forbidden => return reply(request, 403, "text/plain", "forbidden", None),
            Verdict::Unauthorized => {
                return reply(request, 401, "text/plain", "unauthorized", None);
            }
            // A non-public path passed the cookie check, so only a logged-in request keeps the app alive.
            Verdict::Allow if !is_public(&path) => self.auth.touch(Instant::now()),
            Verdict::Allow => {}
        }
        let get = *request.method() == Method::Get;
        match (get, path.as_str()) {
            (true, "/") => reply(request, 200, "text/html; charset=utf-8", PAGE, None),
            (true, "/setup.css") => reply(request, 200, "text/css; charset=utf-8", CSS, None),
            (true, "/setup.js") => {
                reply(
                    request,
                    200,
                    "text/javascript; charset=utf-8",
                    SETUP_JS,
                    None,
                );
            }
            (true, "/palette.js") => {
                reply(
                    request,
                    200,
                    "text/javascript; charset=utf-8",
                    PALETTE_JS,
                    None,
                );
            }
            (true, "/favicon.svg") => reply(request, 200, "image/svg+xml", FAVICON, None),
            (true, "/fonts/JetBrainsMono-Regular.woff2") => {
                reply(request, 200, "font/woff2", FONT_REGULAR, None);
            }
            (true, "/fonts/JetBrainsMono-SemiBold.woff2") => {
                reply(request, 200, "font/woff2", FONT_SEMIBOLD, None);
            }
            (false, "/auth") => match read_body(&mut request) {
                Err(status) => reply(request, status, "text/plain", "bad body", None),
                Ok(body) => {
                    let id = serde_json::from_slice::<AuthBody>(&body)
                        .ok()
                        .and_then(|b| self.auth.redeem(&b.token));
                    match id {
                        Some(id) => {
                            self.auth.touch(Instant::now());
                            let cookie =
                                format!("{SESSION_COOKIE}={id}; HttpOnly; SameSite=Strict; Path=/");
                            reply(
                                request,
                                200,
                                "application/json",
                                r#"{"ok":true}"#,
                                Some(cookie),
                            );
                        }
                        None => reply(request, 401, "text/plain", "unauthorized", None),
                    }
                }
            },
            (false, "/api/action") => match read_body(&mut request) {
                Err(status) => reply(request, status, "text/plain", "bad body", None),
                Ok(body) => match action::parse(&body) {
                    Ok(a) => {
                        let out = action::run(&a, host).to_string();
                        reply(request, 200, "application/json", &out, None);
                    }
                    Err(e) => reply(request, 400, "text/plain", e.to_string(), None),
                },
            },
            _ => reply(request, 404, "text/plain", "not found", None),
        }
    }
}

/// Every value of one header, one entry per occurrence.
fn values<'a>(request: &'a Request, name: &'static str) -> Vec<&'a str> {
    request
        .headers()
        .iter()
        .filter(|h| h.field.equiv(name))
        .map(|h| h.value.as_str())
        .collect()
}

/// Read at most `MAX_BODY` bytes; 413 when the request is bigger.
// shortcut: tiny_http 0.12 sets no read timeout, so a client that stalls mid-body blocks this
// loop until it closes. Loopback only and Host/Origin gated; upgrade if the app ever serves more
// than one user. Recorded in decision 0009.
fn read_body(request: &mut Request) -> Result<Vec<u8>, u16> {
    if request.body_length().is_some_and(|n| n > MAX_BODY) {
        return Err(413);
    }
    let mut buf = Vec::new();
    request
        .as_reader()
        .take(MAX_BODY as u64 + 1)
        .read_to_end(&mut buf)
        .map_err(|_| 400u16)?;
    if buf.len() > MAX_BODY {
        Err(413)
    } else {
        Ok(buf)
    }
}

/// Answer, with the headers every response carries. The only place a response is built.
fn reply(
    request: Request,
    status: u16,
    ctype: &str,
    body: impl AsRef<[u8]>,
    cookie: Option<String>,
) {
    let header = |k: &str, v: &str| Header::from_bytes(k.as_bytes(), v.as_bytes()).unwrap();
    let mut r = Response::from_data(body.as_ref().to_vec())
        .with_status_code(status)
        .with_header(header("Content-Type", ctype))
        .with_header(header("Cache-Control", "no-store"))
        .with_header(header("X-Content-Type-Options", "nosniff"))
        .with_header(header("Referrer-Policy", "no-referrer"))
        .with_header(header(
            "Content-Security-Policy",
            "default-src 'self'; frame-ancestors 'none'",
        ));
    if let Some(c) = cookie {
        r = r.with_header(header("Set-Cookie", &c));
    }
    request.respond(r).ok();
}
