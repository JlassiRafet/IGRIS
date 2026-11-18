use igris_detect::{Host, RealHost};
use std::time::{Duration, Instant};

#[test]
fn run_returns_stdout_of_a_command_that_exits_zero() {
    assert_eq!(
        RealHost::new().run("echo", &["hi"]).as_deref(),
        Some("hi\n")
    );
}

#[test]
fn a_missing_or_failing_command_is_none() {
    let h = RealHost::new();
    assert_eq!(h.run("igris-no-such-command", &[]), None);
    assert_eq!(h.run("false", &[]), None);
}

#[test]
fn a_hung_command_is_killed_at_the_timeout() {
    let h = RealHost::with_timeout(Duration::from_millis(300));
    let t = Instant::now();
    assert_eq!(h.run("sleep", &["10"]), None);
    assert!(t.elapsed() < Duration::from_secs(3), "{:?}", t.elapsed());
}

#[test]
fn large_output_does_not_deadlock() {
    let out = RealHost::new().run("head", &["-c", "300000", "/dev/zero"]);
    assert_eq!(out.map(|s| s.len()), Some(300_000));
}

#[test]
fn read_and_machine_basics() {
    let h = RealHost::new();
    assert_eq!(h.read("/igris/no/such/file"), None);
    assert!(h.read("/proc/self/stat").is_some() || cfg!(not(target_os = "linux")));
    assert!(h.cpu_cores().is_some_and(|n| n > 0));
    assert!(h.home().is_some_and(|p| p.starts_with('/')));
    assert_eq!(h.os(), std::env::consts::OS);
}
