//! The one-time token, the session id it turns into, and the idle clock.

use crate::SetupError;
use std::time::{Duration, Instant};

/// Compare two byte strings without stopping at the first difference.
pub fn ct_eq(a: &[u8], b: &[u8]) -> bool {
    a.len() == b.len() && a.iter().zip(b).fold(0u8, |d, (x, y)| d | (x ^ y)) == 0
}

/// 32 random bytes as 64 hex characters.
fn random_hex() -> Result<String, SetupError> {
    let mut b = [0u8; 32];
    getrandom::fill(&mut b).map_err(|e| SetupError::Random(e.to_string()))?;
    Ok(b.iter().map(|x| format!("{x:02x}")).collect())
}

pub struct Auth {
    token: String,
    session: Option<String>,
    redeemed: bool,
    last: Instant,
}

impl Auth {
    pub fn new(now: Instant) -> Result<Auth, SetupError> {
        Ok(Auth {
            token: random_hex()?,
            session: None,
            redeemed: false,
            last: now,
        })
    }

    pub fn token(&self) -> &str {
        &self.token
    }

    pub fn session(&self) -> Option<&str> {
        self.session.as_deref()
    }

    /// Trade the token for a session id. Works once; a wrong token changes nothing.
    pub fn redeem(&mut self, presented: &str) -> Option<String> {
        if self.redeemed || !ct_eq(presented.as_bytes(), self.token.as_bytes()) {
            return None;
        }
        let id = random_hex().ok()?;
        self.redeemed = true;
        self.session = Some(id.clone());
        Some(id)
    }

    pub fn touch(&mut self, now: Instant) {
        self.last = now;
    }

    pub fn idle(&self, now: Instant, limit: Duration) -> bool {
        now.saturating_duration_since(self.last) > limit
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, Instant};

    fn is_hex64(s: &str) -> bool {
        s.len() == 64 && s.bytes().all(|b| b.is_ascii_hexdigit())
    }

    #[test]
    fn ct_eq_checks_content_and_length() {
        assert!(ct_eq(b"abc", b"abc"));
        assert!(ct_eq(b"", b""));
        assert!(!ct_eq(b"abc", b"abd"));
        assert!(!ct_eq(b"abc", b"abcd"));
        assert!(!ct_eq(b"abc", b""));
    }

    #[test]
    fn token_is_64_hex_and_unique() {
        let (a, b) = (
            Auth::new(Instant::now()).unwrap(),
            Auth::new(Instant::now()).unwrap(),
        );
        assert!(is_hex64(a.token()));
        assert!(is_hex64(b.token()));
        assert_ne!(a.token(), b.token());
    }

    #[test]
    fn token_redeems_once() {
        let mut a = Auth::new(Instant::now()).unwrap();
        let token = a.token().to_string();
        let id = a.redeem(&token).expect("first use works");
        assert!(is_hex64(&id));
        assert_eq!(a.redeem(&token), None, "second use is refused");
        assert_eq!(a.session(), Some(id.as_str()), "the first session survives");
    }

    #[test]
    fn wrong_token_does_not_burn_it() {
        let mut a = Auth::new(Instant::now()).unwrap();
        let token = a.token().to_string();
        assert_eq!(a.redeem("nope"), None);
        assert!(a.redeem(&token).is_some());
    }

    #[test]
    fn idle_uses_the_last_touch() {
        let t0 = Instant::now();
        let limit = Duration::from_secs(10);
        let mut a = Auth::new(t0).unwrap();
        assert!(!a.idle(t0 + Duration::from_secs(9), limit));
        assert!(a.idle(t0 + Duration::from_secs(11), limit));
        a.touch(t0 + Duration::from_secs(9));
        assert!(!a.idle(t0 + Duration::from_secs(11), limit));
    }
}
