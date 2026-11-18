use crate::Host;
use std::io::Read;
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
}
