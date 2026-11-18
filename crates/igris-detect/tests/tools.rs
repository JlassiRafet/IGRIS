mod common;
use common::{FakeHost, fixture};
use igris_detect::{Login, detect};

fn agent(h: &FakeHost, kind: &str) -> igris_detect::Agent {
    detect(h)
        .agents
        .into_iter()
        .find(|a| a.kind == kind)
        .unwrap()
}

#[test]
fn herdr_version_is_read_from_the_recorded_output() {
    let h = FakeHost::linux()
        .bin("herdr")
        .cmd("herdr --version", &fixture("herdr/version.txt"));
    let v = detect(&h).herdr;
    assert_eq!(
        (v.found, v.version.as_deref(), v.ok),
        (true, Some("0.9.1"), true)
    );
}

#[test]
fn an_older_herdr_is_found_but_not_ok_and_a_missing_one_is_not_found() {
    let old = FakeHost::linux()
        .bin("herdr")
        .cmd("herdr --version", "herdr 0.8.3\n");
    let v = detect(&old).herdr;
    assert_eq!((v.found, v.ok), (true, false));
    let none = detect(&FakeHost::linux()).herdr;
    assert_eq!((none.found, none.version, none.ok), (false, None, false));
}

#[test]
fn a_herdr_that_prints_nothing_readable_has_no_version_and_is_not_ok() {
    let h = FakeHost::linux()
        .bin("herdr")
        .cmd("herdr --version", "garbage\n");
    let v = detect(&h).herdr;
    assert_eq!((v.found, v.version, v.ok), (true, None, false));
}

#[test]
fn claude_login_comes_from_auth_status_and_never_leaks_account_details() {
    let h = FakeHost::linux().bin("claude").cmd(
        "claude auth status",
        &fixture("agents/claude-auth-status.json"),
    );
    let a = agent(&h, "claude");
    assert_eq!(
        (a.found, a.login, a.method.as_deref()),
        (true, Login::LoggedIn, Some("claude.ai"))
    );
    let json = serde_json::to_string(&detect(&h)).unwrap();
    for secret in [
        "someone@example.com",
        "Example Org",
        "00000000-0000",
        "/home/user/.claude",
    ] {
        assert!(!json.contains(secret), "leaked {secret}");
    }
}

#[test]
fn claude_logged_out_and_unreadable_are_different_answers() {
    let out = FakeHost::linux()
        .bin("claude")
        .cmd("claude auth status", "{\"loggedIn\": false}");
    assert_eq!(agent(&out, "claude").login, Login::LoggedOut);
    let junk = FakeHost::linux()
        .bin("claude")
        .cmd("claude auth status", "<html>");
    assert_eq!(agent(&junk, "claude").login, Login::Unknown);
    let failed = FakeHost::linux().bin("claude");
    assert_eq!(
        agent(&failed, "claude").login,
        Login::Unknown,
        "command failed: not the same as logged out"
    );
}

#[test]
fn codex_login_comes_from_login_status() {
    let on = FakeHost::linux().bin("codex").cmd_stderr(
        "codex login status",
        &fixture("agents/codex-login-status.txt"),
    );
    let a = agent(&on, "codex");
    assert_eq!(
        (a.login, a.method.as_deref()),
        (Login::LoggedIn, Some("ChatGPT"))
    );
    let off = FakeHost::linux().bin("codex").cmd_stderr(
        "codex login status",
        &fixture("agents/codex-login-status-out.handmade.txt"),
    );
    assert_eq!(agent(&off, "codex").login, Login::LoggedOut);
}

#[test]
fn every_herdr_agent_kind_is_listed_and_only_binaries_on_path_are_found() {
    let h = FakeHost::linux().bin("gemini");
    let list = detect(&h).agents;
    assert_eq!(list.len(), 24);
    assert_eq!(
        list.iter()
            .filter(|a| a.found)
            .map(|a| a.kind.as_str())
            .collect::<Vec<_>>(),
        ["gemini"]
    );
    let g = agent(&h, "gemini");
    assert_eq!(g.login, Login::Unknown, "no known way to ask: say so");
    assert!(g.how.contains("unknown"), "{}", g.how);
}
