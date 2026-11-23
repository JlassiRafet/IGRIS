#![cfg(unix)] // the fake Herdr is a shell script
mod common;
use common::FakeHerdr;

fn text(o: &std::process::Output) -> (String, String) {
    (
        String::from_utf8_lossy(&o.stdout).into_owned(),
        String::from_utf8_lossy(&o.stderr).into_owned(),
    )
}

#[test]
fn records_argv_and_returns_fixture() {
    let fake = FakeHerdr::new();
    fake.on("pane-split", "pane-split.json");
    let args = [
        "--machine",
        "box1",
        "pane",
        "split",
        "w2:p1",
        "--direction",
        "right",
    ];
    let out = fake.run(&args);
    assert!(out.status.success());
    assert!(text(&out).0.contains("\"pane_id\":\"w2:p2\""));
    assert_eq!(fake.calls(), vec![args.map(String::from).to_vec()]);
}

#[test]
fn error_fixture_goes_to_stderr_and_exits_1() {
    let fake = FakeHerdr::new();
    fake.fail("agent-prompt", "error-agent-blocked.txt");
    let out = fake.run(&["agent", "prompt", "builder", "hi", "--wait"]);
    assert_eq!(out.status.code(), Some(1));
    let (stdout, stderr) = text(&out);
    assert!(stdout.is_empty());
    assert!(stderr.contains("\"code\":\"agent_blocked\""));
}

#[test]
fn queue_serves_in_order_then_falls_back() {
    let fake = FakeHerdr::new();
    fake.queue_ok("agent-get", "agent-get-working.json");
    fake.queue_ok("agent-get", "agent-get-done.json");
    fake.on("agent-get", "agent-get-idle.json");
    let states: Vec<String> = (0..3)
        .map(|_| text(&fake.run(&["agent", "get", "builder"])).0)
        .map(|s| {
            s.split("\"agent_status\":\"")
                .nth(1)
                .unwrap()
                .split('"')
                .next()
                .unwrap()
                .to_string()
        })
        .collect();
    assert_eq!(states, ["working", "done", "idle"]);
}

#[test]
fn unknown_command_fails_loudly() {
    let fake = FakeHerdr::new();
    let out = fake.run(&["pane", "close", "w1:p1"]);
    assert_eq!(out.status.code(), Some(97));
    assert!(text(&out).1.contains("no fixture for pane-close"));
}
