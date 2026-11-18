use crate::Host;
use std::io::{Read, Write};
use std::net::{SocketAddr, TcpStream};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

pub struct RealHost {
    timeout: Duration,
}

impl RealHost {
    /// Commands get five seconds.
    pub fn new() -> Self {
        Self::with_timeout(Duration::from_secs(5))
    }
    pub fn with_timeout(timeout: Duration) -> Self {
        Self { timeout }
    }
}

impl Default for RealHost {
    fn default() -> Self {
        Self::new()
    }
}

impl Host for RealHost {
    fn os(&self) -> &str {
        std::env::consts::OS
    }
    fn home(&self) -> Option<String> {
        std::env::var("HOME").ok().filter(|h| h.starts_with('/'))
    }
    fn cpu_cores(&self) -> Option<u32> {
        std::thread::available_parallelism()
            .ok()
            .and_then(|n| u32::try_from(n.get()).ok())
    }
    fn run(&self, cmd: &str, args: &[&str]) -> Option<String> {
        let mut child = Command::new(cmd)
            .args(args)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .ok()?;
        // Read on a thread so a chatty command cannot fill the pipe and stall until the timeout.
        let mut pipe = child.stdout.take()?;
        let reader = std::thread::spawn(move || {
            let mut buf = Vec::new();
            pipe.read_to_end(&mut buf).ok()?;
            Some(buf)
        });
        let start = Instant::now();
        let status = loop {
            match child.try_wait() {
                Ok(Some(s)) => break Some(s),
                Ok(None) if start.elapsed() < self.timeout => {
                    std::thread::sleep(Duration::from_millis(10));
                }
                _ => {
                    let _ = child.kill();
                    let _ = child.wait();
                    break None;
                }
            }
        };
        let out = reader.join().ok()??;
        status
            .filter(|s| s.success())
            .and_then(|_| String::from_utf8(out).ok())
    }
    fn read(&self, path: &str) -> Option<String> {
        std::fs::read_to_string(path).ok()
    }
    fn has_env(&self, name: &str) -> bool {
        std::env::var_os(name).is_some_and(|v| !v.is_empty())
    }
    fn http_get(&self, port: u16, path: &str) -> Option<String> {
        const WAIT: Duration = Duration::from_millis(300);
        // A model list is small: anything bigger is not what we asked for.
        const MAX: usize = 1 << 20;
        let addr = SocketAddr::from(([127, 0, 0, 1], port));
        let mut stream = TcpStream::connect_timeout(&addr, WAIT).ok()?;
        stream.set_read_timeout(Some(WAIT)).ok()?;
        stream.set_write_timeout(Some(WAIT)).ok()?;
        let request =
            format!("GET {path} HTTP/1.0\r\nHost: 127.0.0.1:{port}\r\nConnection: close\r\n\r\n");
        stream.write_all(request.as_bytes()).ok()?;
        // Read in chunks against one overall deadline, so a server that trickles bytes cannot stall us.
        let deadline = Instant::now() + 3 * WAIT;
        let mut raw = Vec::new();
        let mut chunk = [0u8; 8192];
        loop {
            match stream.read(&mut chunk) {
                Ok(0) => break,
                Ok(n) => raw.extend_from_slice(&chunk[..n]),
                Err(_) => return None,
            }
            if raw.len() > MAX || Instant::now() > deadline {
                return None;
            }
        }
        let text = String::from_utf8(raw).ok()?;
        let (head, body) = text.split_once("\r\n\r\n")?;
        head.lines()
            .next()?
            .split_whitespace()
            .nth(1)
            .filter(|code| *code == "200")?;
        Some(body.to_string())
    }
    fn which(&self, name: &str) -> Option<String> {
        let path = std::env::var_os("PATH")?;
        std::env::split_paths(&path)
            .map(|d| d.join(name))
            .find(|p| is_executable(p))
            .map(|p| p.display().to_string())
    }
}

#[cfg(unix)]
fn is_executable(p: &std::path::Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    p.metadata()
        .is_ok_and(|m| m.is_file() && m.permissions().mode() & 0o111 != 0)
}

#[cfg(not(unix))]
fn is_executable(p: &std::path::Path) -> bool {
    p.is_file()
}
