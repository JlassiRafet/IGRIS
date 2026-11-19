//! The request filter: one pure function every request passes before any route runs.

/// What the filter needs to know about a request. Each header holds one entry per occurrence, so
/// a repeated header can be refused instead of "first one wins".
pub struct ReqMeta<'a> {
    pub method: &'a str,
    pub path: &'a str,
    pub hosts: Vec<&'a str>,
    pub origins: Vec<&'a str>,
    pub cookies: Vec<&'a str>,
}

#[derive(Debug, PartialEq, Eq)]
pub enum Verdict {
    Allow,
    Forbidden,
    Unauthorized,
}

pub const SESSION_COOKIE: &str = "igris_session";

/// Judge a request. Order matters: Host, then method, then Origin (POST), then the cookie
/// (`/api/`), so a bad Host is always `Forbidden` whatever else the request carries.
pub fn check(req: &ReqMeta, port: u16, session: Option<&str>) -> Verdict {
    let [host] = req.hosts[..] else {
        return Verdict::Forbidden;
    };
    let host = host.to_ascii_lowercase();
    let local = [format!("127.0.0.1:{port}"), format!("localhost:{port}")];
    if !local.contains(&host) {
        return Verdict::Forbidden;
    }
    if req.origins.len() > 1 || req.cookies.len() > 1 {
        return Verdict::Forbidden;
    }
    match req.method {
        "GET" => {}
        "POST" => {
            let [origin] = req.origins[..] else {
                return Verdict::Forbidden;
            };
            if !local
                .iter()
                .any(|h| origin.eq_ignore_ascii_case(&format!("http://{h}")))
            {
                return Verdict::Forbidden;
            }
        }
        _ => return Verdict::Forbidden,
    }
    if req.path.starts_with("/api/") && !has_session(&req.cookies, session) {
        return Verdict::Unauthorized;
    }
    Verdict::Allow
}

/// True when the Cookie header holds `igris_session=<session>` (repeated headers never get here).
fn has_session(cookies: &[&str], session: Option<&str>) -> bool {
    let (Some(session), [cookie]) = (session, cookies) else {
        return false;
    };
    cookie
        .split(';')
        .filter_map(|p| p.trim().split_once('='))
        .any(|(k, v)| k == SESSION_COOKIE && v == session)
}

#[cfg(test)]
mod tests {
    use super::*;

    const PORT: u16 = 8421;

    fn req<'a>(
        method: &'a str,
        path: &'a str,
        host: Option<&'a str>,
        origin: Option<&'a str>,
        cookie: Option<&'a str>,
    ) -> ReqMeta<'a> {
        ReqMeta {
            method,
            path,
            hosts: host.into_iter().collect(),
            origins: origin.into_iter().collect(),
            cookies: cookie.into_iter().collect(),
        }
    }

    fn page(host: Option<&str>) -> Verdict {
        check(&req("GET", "/", host, None, None), PORT, Some("abc"))
    }

    #[test]
    fn host_must_match_exactly() {
        assert_eq!(page(Some("127.0.0.1:8421")), Verdict::Allow);
        assert_eq!(page(Some("LOCALHOST:8421")), Verdict::Allow);
        for bad in [
            "127.0.0.1:9999",
            "127.0.0.1.evil.com:8421",
            "127.0.0.1:8421.evil.com",
            "evil.com",
            "[::1]:8421",
        ] {
            assert_eq!(page(Some(bad)), Verdict::Forbidden, "{bad}");
        }
        assert_eq!(page(None), Verdict::Forbidden);
    }

    #[test]
    fn duplicate_headers_are_refused() {
        let mut r = req("GET", "/", Some("127.0.0.1:8421"), None, None);
        r.hosts.push("127.0.0.1:8421");
        assert_eq!(check(&r, PORT, None), Verdict::Forbidden);

        let mut r = req(
            "POST",
            "/auth",
            Some("127.0.0.1:8421"),
            Some("http://127.0.0.1:8421"),
            None,
        );
        r.origins.push("http://127.0.0.1:8421");
        assert_eq!(check(&r, PORT, None), Verdict::Forbidden);

        let mut r = req(
            "POST",
            "/api/action",
            Some("127.0.0.1:8421"),
            Some("http://127.0.0.1:8421"),
            Some("igris_session=abc"),
        );
        r.cookies.push("igris_session=abc");
        assert_eq!(check(&r, PORT, Some("abc")), Verdict::Forbidden);
    }

    #[test]
    fn post_needs_a_matching_origin() {
        let post = |origin| {
            check(
                &req("POST", "/auth", Some("127.0.0.1:8421"), origin, None),
                PORT,
                None,
            )
        };
        assert_eq!(post(Some("http://127.0.0.1:8421")), Verdict::Allow);
        assert_eq!(post(Some("http://localhost:8421")), Verdict::Allow);
        for bad in [
            None,
            Some("null"),
            Some("http://evil.com"),
            Some("https://127.0.0.1:8421"),
            Some("http://127.0.0.1:1"),
        ] {
            assert_eq!(post(bad), Verdict::Forbidden, "{bad:?}");
        }
    }

    #[test]
    fn options_is_forbidden() {
        let r = req(
            "OPTIONS",
            "/api/action",
            Some("127.0.0.1:8421"),
            Some("http://127.0.0.1:8421"),
            Some("igris_session=abc"),
        );
        assert_eq!(check(&r, PORT, Some("abc")), Verdict::Forbidden);
    }

    #[test]
    fn api_needs_the_cookie() {
        let api = |cookie, session| {
            check(
                &req(
                    "POST",
                    "/api/action",
                    Some("127.0.0.1:8421"),
                    Some("http://127.0.0.1:8421"),
                    cookie,
                ),
                PORT,
                session,
            )
        };
        assert_eq!(api(Some("igris_session=abc"), Some("abc")), Verdict::Allow);
        assert_eq!(
            api(Some("a=1; igris_session=abc"), Some("abc")),
            Verdict::Allow
        );
        assert_eq!(api(None, Some("abc")), Verdict::Unauthorized);
        assert_eq!(
            api(Some("igris_session=zzz"), Some("abc")),
            Verdict::Unauthorized
        );
        assert_eq!(api(Some("other=abc"), Some("abc")), Verdict::Unauthorized);
        assert_eq!(api(Some("igris_session=abc"), None), Verdict::Unauthorized);
    }

    #[test]
    fn page_and_auth_need_no_cookie() {
        let h = Some("127.0.0.1:8421");
        assert_eq!(
            check(&req("GET", "/", h, None, None), PORT, None),
            Verdict::Allow
        );
        let o = Some("http://127.0.0.1:8421");
        assert_eq!(
            check(&req("POST", "/auth", h, o, None), PORT, None),
            Verdict::Allow
        );
    }

    #[test]
    fn a_bad_host_beats_a_good_cookie() {
        let r = req(
            "POST",
            "/api/action",
            Some("evil.com"),
            Some("http://evil.com"),
            Some("igris_session=abc"),
        );
        assert_eq!(check(&r, PORT, Some("abc")), Verdict::Forbidden);
    }
}
