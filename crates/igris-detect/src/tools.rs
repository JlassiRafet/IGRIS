use crate::Host;
use igris_core::config::KINDS;
use serde::Serialize;

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct HerdrInfo {
    pub found: bool,
    pub version: Option<String>,
    pub ok: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Login {
    LoggedIn,
    LoggedOut,
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Agent {
    pub kind: String,
    pub found: bool,
    pub login: Login,
    pub method: Option<String>,
    pub how: String,
}

/// The oldest Herdr IGRIS has been tested against.
const MIN_HERDR: (u32, u32, u32) = (0, 9, 1);

fn parse_version(text: &str) -> Option<(String, (u32, u32, u32))> {
    let v = text.split_whitespace().nth(1)?;
    let mut n = v.split('.').map(|p| p.parse::<u32>().ok());
    let t = (n.next()??, n.next()??, n.next()??);
    n.next().is_none().then(|| (v.to_string(), t))
}

pub fn herdr(host: &dyn Host) -> HerdrInfo {
    let found = host.which("herdr").is_some();
    let parsed = host
        .run("herdr", &["--version"])
        .and_then(|o| parse_version(&o));
    HerdrInfo {
        found,
        ok: found && parsed.as_ref().is_some_and(|(_, t)| *t >= MIN_HERDR),
        version: parsed.map(|(v, _)| v),
    }
}

/// Claude Code: `claude auth status` prints JSON that also holds the account email and
/// organisation, so only the two fields we need are ever read.
fn claude_login(host: &dyn Host) -> (Login, Option<String>) {
    let Some(v) = host
        .run("claude", &["auth", "status"])
        .and_then(|o| serde_json::from_str::<serde_json::Value>(&o).ok())
    else {
        return (Login::Unknown, None);
    };
    match v["loggedIn"].as_bool() {
        Some(true) => (
            Login::LoggedIn,
            v["authMethod"].as_str().map(str::to_string),
        ),
        Some(false) => (Login::LoggedOut, None),
        None => (Login::Unknown, None),
    }
}

fn codex_login(host: &dyn Host) -> (Login, Option<String>) {
    let Some(out) = host.run("codex", &["login", "status"]) else {
        return (Login::Unknown, None);
    };
    if out.contains("Not logged in") {
        (Login::LoggedOut, None)
    } else if let Some(rest) = out.split("Logged in using ").nth(1) {
        (
            Login::LoggedIn,
            Some(rest.lines().next().unwrap_or("").trim().to_string()),
        )
    } else {
        (Login::Unknown, None)
    }
}

pub fn agents(host: &dyn Host) -> Vec<Agent> {
    KINDS
        .iter()
        .map(|kind| {
            let found = host.which(kind).is_some();
            let (login, method, how) = match (*kind, found) {
                (_, false) => (Login::Unknown, None, "not on PATH"),
                ("claude", _) => {
                    let (l, m) = claude_login(host);
                    (l, m, "claude auth status")
                }
                ("codex", _) => {
                    let (l, m) = codex_login(host);
                    (l, m, "codex login status")
                }
                _ => (
                    Login::Unknown,
                    None,
                    "binary on PATH only; login state unknown",
                ),
            };
            Agent {
                kind: (*kind).to_string(),
                found,
                login,
                method,
                how: how.to_string(),
            }
        })
        .collect()
}
