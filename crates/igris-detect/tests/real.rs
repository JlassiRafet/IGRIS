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
    assert!(
        h.home()
            .is_some_and(|p| std::path::Path::new(&p).is_absolute())
    );
    assert_eq!(h.os(), std::env::consts::OS);
}

fn serve_once(reply: &'static [u8]) -> u16 {
    let l = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let port = l.local_addr().unwrap().port();
    std::thread::spawn(move || {
        if let Ok((mut s, _)) = l.accept() {
            use std::io::{Read, Write};
            let mut buf = [0u8; 512];
            let _ = s.read(&mut buf);
            let _ = s.write_all(reply);
        }
    });
    port
}

#[test]
fn http_get_returns_the_body_of_a_200() {
    let port =
        serve_once(b"HTTP/1.0 200 OK\r\nContent-Type: application/json\r\n\r\n{\"ok\":true}");
    assert_eq!(
        RealHost::new().http_get(port, "/x").as_deref(),
        Some("{\"ok\":true}")
    );
}

#[test]
fn http_get_is_none_for_other_statuses_and_closed_ports() {
    let port = serve_once(b"HTTP/1.0 404 Not Found\r\n\r\nnope");
    assert_eq!(RealHost::new().http_get(port, "/x"), None);
    let closed = {
        let l = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        l.local_addr().unwrap().port()
    };
    assert_eq!(RealHost::new().http_get(closed, "/x"), None);
}

#[test]
fn http_get_gives_up_on_a_server_that_never_answers() {
    let l = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let port = l.local_addr().unwrap().port();
    let _hold = std::thread::spawn(move || {
        let _conn = l.accept();
        std::thread::sleep(Duration::from_secs(5));
    });
    let t = Instant::now();
    assert_eq!(RealHost::new().http_get(port, "/x"), None);
    assert!(t.elapsed() < Duration::from_secs(3), "{:?}", t.elapsed());
}

#[test]
fn http_get_refuses_an_oversized_body() {
    let port = {
        let l = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = l.local_addr().unwrap().port();
        std::thread::spawn(move || {
            use std::io::{Read, Write};
            if let Ok((mut s, _)) = l.accept() {
                let mut b = [0u8; 512];
                let _ = s.read(&mut b);
                let _ = s.write_all(b"HTTP/1.0 200 OK\r\n\r\n");
                let chunk = vec![b'a'; 64 * 1024];
                for _ in 0..40 {
                    if s.write_all(&chunk).is_err() {
                        break;
                    }
                }
            }
        });
        port
    };
    assert_eq!(
        RealHost::new().http_get(port, "/x"),
        None,
        "over 1 MiB is not a model list"
    );
}

#[test]
fn http_get_gives_up_on_a_server_that_trickles_bytes() {
    let l = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let port = l.local_addr().unwrap().port();
    std::thread::spawn(move || {
        use std::io::{Read, Write};
        if let Ok((mut s, _)) = l.accept() {
            let mut b = [0u8; 512];
            let _ = s.read(&mut b);
            let _ = s.write_all(b"HTTP/1.0 200 OK\r\n\r\n");
            for _ in 0..60 {
                if s.write_all(b"x").is_err() {
                    break;
                }
                std::thread::sleep(Duration::from_millis(100));
            }
        }
    });
    let t = Instant::now();
    assert_eq!(RealHost::new().http_get(port, "/x"), None);
    assert!(t.elapsed() < Duration::from_secs(2), "{:?}", t.elapsed());
}

#[test]
fn run_with_stderr_appends_what_a_tool_prints_on_stderr() {
    let h = RealHost::new();
    let args = ["-c", "echo out; echo status >&2"];
    assert_eq!(
        h.run("sh", &args).as_deref(),
        Some("out\n"),
        "run keeps stdout clean"
    );
    assert_eq!(
        h.run_with_stderr("sh", &args).as_deref(),
        Some("out\nstatus\n")
    );
    assert_eq!(
        h.run_with_stderr("sh", &["-c", "echo no >&2; exit 1"]),
        None
    );
}
